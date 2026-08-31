//! OpenRouter integration: fetch the list of models available to the user's
//! account. The API key is supplied by the user at runtime (see the `set_key`
//! command) and is never hardcoded.

use super::errors::AppError;

/// A model available on OpenRouter, as shown in the picker.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Model {
    /// The model id (e.g. `openai/gpt-4o`), used when sending a chat request.
    pub id: String,
    /// A human-friendly name for display.
    pub name: String,
    /// The provider, derived from the id prefix (e.g. `openai/gpt-4o` →
    /// `openai`). The backend is the single source of truth for this.
    pub provider: String,
    /// The model's context window in tokens, if reported.
    #[serde(default)]
    pub context_length: Option<u64>,
}

/// Derives the provider from a model id's prefix (e.g. `z-ai/glm-4.5` →
/// `z-ai`). Ids without a prefix map to `other`.
fn provider_of(id: &str) -> String {
    match id.split_once('/') {
        Some((provider, _)) if !provider.is_empty() => provider.to_string(),
        _ => "other".to_string(),
    }
}

/// The shape of a single entry in OpenRouter's `/models` response.
#[derive(serde::Deserialize)]
struct RawModel {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    context_length: Option<u64>,
}

/// The top-level envelope of OpenRouter's `/models` response.
#[derive(serde::Deserialize)]
struct ModelsResponse {
    data: Vec<RawModel>,
}

/// Fetches the models available on OpenRouter for the given API key.
///
/// This is a network call, so it must be run on a blocking thread (see the
/// `list_models` command). The key is sent in the `Authorization` header and
/// is never logged or persisted by this function.
pub fn fetch_models(api_key: &str) -> Result<Vec<Model>, AppError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Http(e.to_string()))?;

    let response = client
        .get("https://openrouter.ai/api/v1/models")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send()
        .map_err(|e| AppError::Http(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http(format!("OpenRouter returned {status}")));
    }

    let body: ModelsResponse = response.json().map_err(|e| AppError::Http(e.to_string()))?;

    Ok(body
        .data
        .into_iter()
        .map(|m| {
            let name = m.name.unwrap_or_else(|| m.id.clone());
            Model {
                provider: provider_of(&m.id),
                id: m.id,
                name,
                context_length: m.context_length,
            }
        })
        .collect())
}

/// A single chat message, as sent to and received from OpenRouter.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ChatMessage {
    /// The author of the message: `system`, `user`, or `assistant`.
    pub role: String,
    /// The message text.
    pub content: String,
}

/// The body of a chat-completion request to OpenRouter.
#[derive(serde::Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
}

/// The shape of a single choice in OpenRouter's chat-completion response.
#[derive(serde::Deserialize)]
struct RawChoice {
    message: RawChatMessage,
}

/// The shape of the assistant message inside a chat-completion choice.
#[derive(serde::Deserialize)]
struct RawChatMessage {
    content: Option<String>,
}

/// The top-level envelope of OpenRouter's chat-completion response.
#[derive(serde::Deserialize)]
struct ChatResponse {
    choices: Vec<RawChoice>,
}

/// Sends a chat completion to OpenRouter and returns the assistant's reply.
///
/// This is a network call, so it must be run on a blocking thread (see the
/// `chat` command). The key is sent in the `Authorization` header and is never
/// logged or persisted by this function.
pub fn chat_completion(
    api_key: &str,
    model: &str,
    messages: &[ChatMessage],
) -> Result<String, AppError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| AppError::Http(e.to_string()))?;

    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
        })
        .send()
        .map_err(|e| AppError::Http(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http(format!("OpenRouter returned {status}")));
    }

    let body: ChatResponse = response.json().map_err(|e| AppError::Http(e.to_string()))?;

    body.choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .ok_or_else(|| AppError::Http("OpenRouter returned no reply".to_string()))
}

/// Masks an API key for safe display, keeping only the last four characters.
/// Returns a placeholder when the key is too short to mask meaningfully.
pub fn mask_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.len() < 8 {
        return "••••".to_string();
    }
    let suffix = &trimmed[trimmed.len() - 4..];
    format!("••••{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_key_keeps_last_four() {
        assert_eq!(mask_key("sk-or-v1-abcdef123456"), "••••3456");
    }

    #[test]
    fn mask_key_short_returns_placeholder() {
        assert_eq!(mask_key("short"), "••••");
        assert_eq!(mask_key(""), "••••");
    }

    #[test]
    fn parses_models_response() {
        let json = r#"{
            "data": [
                {"id": "openai/gpt-4o", "name": "GPT-4o", "context_length": 128000},
                {"id": "anthropic/claude-3.5-sonnet"}
            ]
        }"#;
        let parsed: ModelsResponse = serde_json::from_str(json).unwrap();
        let models: Vec<Model> = parsed
            .data
            .into_iter()
            .map(|m| {
                let name = m.name.unwrap_or_else(|| m.id.clone());
                Model {
                    provider: provider_of(&m.id),
                    id: m.id,
                    name,
                    context_length: m.context_length,
                }
            })
            .collect();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "openai/gpt-4o");
        assert_eq!(models[0].name, "GPT-4o");
        assert_eq!(models[0].provider, "openai");
        assert_eq!(models[0].context_length, Some(128000));
        // A model without a name falls back to its id.
        assert_eq!(models[1].name, "anthropic/claude-3.5-sonnet");
        assert_eq!(models[1].provider, "anthropic");
        assert_eq!(models[1].context_length, None);
    }

    #[test]
    fn provider_of_uses_id_prefix() {
        assert_eq!(provider_of("z-ai/glm-4.5"), "z-ai");
        assert_eq!(provider_of("openai/gpt-4o"), "openai");
        // Ids without a prefix (or with an empty prefix) map to `other`.
        assert_eq!(provider_of("gpt-4o"), "other");
        assert_eq!(provider_of("/gpt-4o"), "other");
    }

    #[test]
    fn parses_chat_response() {
        let json = r#"{
            "choices": [
                {"message": {"role": "assistant", "content": "Hello there!"}}
            ]
        }"#;
        let parsed: ChatResponse = serde_json::from_str(json).unwrap();
        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content);
        assert_eq!(content.as_deref(), Some("Hello there!"));
    }

    #[test]
    fn chat_response_without_content_is_none() {
        let json = r#"{ "choices": [ {"message": {"role": "assistant"}} ] }"#;
        let parsed: ChatResponse = serde_json::from_str(json).unwrap();
        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content);
        assert_eq!(content, None);
    }
}
