//! OpenRouter integration: fetch the list of models available to the user's
//! account. The API key is supplied by the user at runtime (see the `set_key`
//! command) and is never hardcoded.

use std::io::Read;

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
    /// The author of the message: `system`, `user`, `assistant`, or `tool`.
    pub role: String,
    /// The message text. For `tool` messages this is the tool's result.
    pub content: String,
    /// The id of the tool call this message is the result of. Required for
    /// `tool` messages; never set otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// The tool calls the assistant requested. Only set on `assistant`
    /// messages that asked for tools; never set otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// A tool call requested by the assistant.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    /// The id OpenRouter assigns to the call; echoed back in the `tool`
    /// message that carries the result.
    pub id: String,
    /// The tool's name (e.g. `get_local_time`).
    pub name: String,
    /// The tool's arguments, as a JSON string (the API's wire format).
    pub arguments: String,
}

/// A tool the agent may call, in the OpenRouter (OpenAI-compatible) schema.
#[derive(serde::Serialize, Clone)]
struct ToolSpec {
    #[serde(rename = "type")]
    kind: String,
    function: ToolSpecFunction,
}

#[derive(serde::Serialize, Clone)]
struct ToolSpecFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// The body of a chat-completion request to OpenRouter.
#[derive(serde::Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    /// The tools the model may call. Omitted when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolSpec>,
    /// When true, OpenRouter responds with a server-sent-events stream of
    /// incremental chunks instead of a single JSON body.
    stream: bool,
    /// Request token-usage statistics in the final stream chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

/// Options for streaming: request usage stats in the final chunk.
#[derive(serde::Serialize)]
struct StreamOptions {
    include_usage: bool,
}

/// Token usage for a chat completion, as reported by the API.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TokenUsage {
    /// Tokens in the prompt (system + history + tools).
    pub prompt_tokens: u64,
    /// Tokens in the completion (the model's reply).
    pub completion_tokens: u64,
    /// Total tokens (prompt + completion).
    pub total_tokens: u64,
}

/// The assistant's reply to a chat completion.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ChatReply {
    /// The visible reply text.
    pub content: String,
    /// The model's chain-of-thought, if it produced any (reasoning models).
    #[serde(default)]
    pub reasoning: Option<String>,
    /// Token usage for the final round, if the API reported it.
    #[serde(default)]
    pub usage: Option<TokenUsage>,
}

/// A single incremental chunk from a streaming chat completion.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ChatChunk {
    /// The incremental visible reply text (may be empty).
    pub content: String,
    /// The incremental chain-of-thought text (may be empty).
    pub reasoning: String,
    /// Incremental tool-call fragments (may be empty). Each fragment carries
    /// the index of the tool call it belongs to; `id` and `name` arrive in
    /// the first fragment, `arguments` is streamed across fragments.
    #[serde(default)]
    pub tool_calls: Vec<ToolCallDelta>,
}

/// An incremental fragment of a tool call within a streaming chunk.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ToolCallDelta {
    /// The index of the tool call this fragment belongs to.
    pub index: usize,
    /// The tool call's id (present only in the first fragment).
    #[serde(default)]
    pub id: Option<String>,
    /// The tool's name (present only in the first fragment).
    #[serde(default)]
    pub name: Option<String>,
    /// The incremental arguments JSON (concatenated across fragments).
    #[serde(default)]
    pub arguments: String,
}

/// The shape of a single choice in a streaming chat-completion chunk.
#[derive(serde::Deserialize)]
struct RawStreamChoice {
    delta: RawStreamDelta,
}

/// The shape of the assistant delta inside a streaming choice.
#[derive(serde::Deserialize)]
struct RawStreamDelta {
    #[serde(default)]
    content: Option<String>,
    /// The model's chain-of-thought, present only for reasoning models.
    #[serde(default)]
    reasoning: Option<String>,
    /// Incremental tool-call fragments, present when the model calls a tool.
    #[serde(default)]
    tool_calls: Vec<RawToolCallDelta>,
}

/// The wire shape of a tool-call fragment inside a streaming delta.
#[derive(serde::Deserialize)]
struct RawToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<RawToolCallFunction>,
}

/// The wire shape of the function part of a tool-call fragment.
#[derive(serde::Deserialize)]
struct RawToolCallFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// The shape of a single server-sent-events data payload from a streaming
/// chat completion.
#[derive(serde::Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<RawStreamChoice>,
    /// Token usage, present only in the final chunk when `include_usage` is set.
    #[serde(default)]
    usage: Option<RawUsage>,
}

/// Token usage reported by the API in the final stream chunk.
#[derive(serde::Deserialize, Debug, Clone, PartialEq, Eq)]
struct RawUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

/// The tools in the OpenRouter (OpenAI-compatible) request schema, derived
/// from the static tool metadata in `tools.rs`.
fn tool_specs() -> Vec<ToolSpec> {
    super::tools::tool_metas()
        .into_iter()
        .map(|t| ToolSpec {
            kind: "function".to_string(),
            function: ToolSpecFunction {
                name: t.name.to_string(),
                description: t.description.to_string(),
                parameters: t.parameters,
            },
        })
        .collect()
}

/// Sends a streaming chat completion to OpenRouter, invoking `on_chunk` for
/// each incremental chunk as it arrives. Returns the accumulated reply.
///
/// This is a network call, so it must be run on a blocking thread (see the
/// `chat` command). The key is sent in the `Authorization` header and is never
/// logged or persisted by this function.
fn chat_completion_stream(
    api_key: &str,
    model: &str,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    on_chunk: &mut dyn FnMut(&ChatChunk),
) -> Result<StreamReply, AppError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::Http(e.to_string()))?;

    let mut response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .json(&ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            tools: tools.to_vec(),
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
        })
        .send()
        .map_err(|e| AppError::Http(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http(format!("OpenRouter returned {status}")));
    }

    let mut content = String::new();
    let mut reasoning = String::new();
    // Accumulated tool calls, keyed by their stream index. The index is the
    // model's own ordering of the calls in this reply.
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    // Token usage, reported by the API in the final chunk (when
    // `include_usage` is set). Kept as `Option` so a missing field is a
    // non-error.
    let mut usage: Option<TokenUsage> = None;
    // Read the stream incrementally. Server-sent events are newline-delimited,
    // but a line can be split across chunk boundaries, so a buffer holds the
    // trailing partial line until the next chunk completes it.
    let mut buffer = String::new();
    let mut done = false;
    let mut read_buf = [0u8; 8192];
    while !done {
        let n = response
            .read(&mut read_buf)
            .map_err(|e| AppError::Http(e.to_string()))?;
        if n == 0 {
            break; // end of stream
        }
        buffer.push_str(&String::from_utf8_lossy(&read_buf[..n]));
        while let Some(newline) = buffer.find('\n') {
            let line: String = buffer.drain(..=newline).collect();
            let line = line.trim_end_matches(['\n', '\r']);
            // Server-sent events: data lines carry a JSON payload; the stream
            // ends with a line of two dashes.
            let Some(payload) = line.strip_prefix("data: ") else {
                continue;
            };
            if payload == "[DONE]" {
                done = true;
                break;
            }
            let Some((chunk, chunk_usage)) = parse_stream_chunk(payload)? else {
                continue;
            };
            // Capture usage from the final chunk (the only chunk that carries
            // it).
            if let Some(u) = chunk_usage {
                usage = Some(u);
            }
            on_chunk(&chunk);
            content.push_str(&chunk.content);
            reasoning.push_str(&chunk.reasoning);
            for tc in &chunk.tool_calls {
                merge_tool_call_delta(&mut tool_calls, tc);
            }
        }
    }

    let reasoning = (!reasoning.trim().is_empty()).then_some(reasoning);
    Ok(StreamReply {
        content,
        reasoning,
        tool_calls,
        usage,
    })
}

/// The accumulated result of one streamed chat-completion round: the reply
/// text, any reasoning, the complete tool calls the model requested, and
/// token usage (present only in the final round).
struct StreamReply {
    content: String,
    reasoning: Option<String>,
    tool_calls: Vec<ToolCall>,
    usage: Option<TokenUsage>,
}

/// Folds one tool-call fragment into the accumulated tool calls, growing the
/// arguments string across fragments.
fn merge_tool_call_delta(calls: &mut Vec<ToolCall>, delta: &ToolCallDelta) {
    while calls.len() <= delta.index {
        calls.push(ToolCall {
            id: String::new(),
            name: String::new(),
            arguments: String::new(),
        });
    }
    let call = &mut calls[delta.index];
    if let Some(id) = &delta.id {
        call.id = id.clone();
    }
    if let Some(name) = &delta.name {
        call.name = name.clone();
    }
    call.arguments.push_str(&delta.arguments);
}

/// Sends a chat completion to OpenRouter with the agent's tools available,
/// and runs the tool loop: when the model calls a tool, the call is executed
/// locally, its result is appended to the conversation, and the model is
/// asked again — until it produces a plain text reply. `on_chunk` is invoked
/// for every incremental chunk of every round, and `on_tool` for each tool
/// call as it is executed (name and result, for the UI).
///
/// This is a network call, so it must be run on a blocking thread (see the
/// `chat` command). The key is sent in the `Authorization` header and is never
/// logged or persisted by this function.
#[allow(clippy::too_many_arguments)]
pub fn chat_with_tools(
    api_key: &str,
    model: &str,
    root: &std::path::Path,
    memory_root: &std::path::Path,
    messages: &[ChatMessage],
    agent_prompt: Option<&str>,
    on_chunk: &mut dyn FnMut(&ChatChunk),
    on_tool: &mut dyn FnMut(&str, &str),
) -> Result<ChatReply, AppError> {
    // Cap the number of tool rounds so a model that keeps calling tools
    // cannot loop forever (each round is a full network round trip).
    const MAX_TOOL_ROUNDS: usize = 5;
    let tools = tool_specs();
    // Tell the model its working directory so it can reason about relative
    // paths when calling the file and directory tools. A custom agent prompt,
    // when present, is layered on top so the model retains tool context.
    let base_prompt = format!(
        "You are an agent working inside the project folder `{}`. \
         File and directory tool paths are relative to that folder. \
         You can also run PowerShell commands in that folder with the \
         run_command tool. Use the tools to inspect and modify the \
         project rather than guessing at its contents. \
         You have a persistent memory (the `memory` tool) that survives \
         across chat sessions: use it to record decisions, conventions, and \
         context worth remembering, and consult it at the start of a task \
         before assuming you have no prior context.",
        root.display()
    );
    // Cap the agent prompt so a very long custom prompt cannot blow up the
    // context window. The cap is generous (4000 chars ≈ 1000 tokens) but
    // prevents accidental or malicious prompt bloat.
    const MAX_AGENT_PROMPT_CHARS: usize = 4000;
    let system_content = match agent_prompt {
        Some(prompt) if !prompt.trim().is_empty() => {
            let trimmed = prompt.trim();
            let capped: String = trimmed.chars().take(MAX_AGENT_PROMPT_CHARS).collect();
            format!("{base_prompt}\n\n{capped}")
        }
        _ => base_prompt,
    };
    let mut conversation: Vec<ChatMessage> = vec![ChatMessage {
        role: "system".to_string(),
        content: system_content,
        tool_call_id: None,
        tool_calls: None,
    }];
    conversation.extend_from_slice(messages);

    for _ in 0..MAX_TOOL_ROUNDS {
        let reply = chat_completion_stream(api_key, model, &conversation, &tools, on_chunk)?;

        if reply.tool_calls.is_empty() {
            return Ok(ChatReply {
                content: reply.content,
                reasoning: reply.reasoning,
                usage: reply.usage,
            });
        }

        // Record the assistant's tool-call message, then execute each call
        // and append its result as a `tool` message.
        conversation.push(ChatMessage {
            role: "assistant".to_string(),
            content: reply.content,
            tool_call_id: None,
            tool_calls: Some(reply.tool_calls.clone()),
        });
        for call in &reply.tool_calls {
            let result = super::tools::execute_tool(root, memory_root, &call.name, &call.arguments)
                .unwrap_or_else(|e| e.to_string());
            on_tool(&call.name, &result);
            conversation.push(ChatMessage {
                role: "tool".to_string(),
                content: result,
                tool_call_id: Some(call.id.clone()),
                tool_calls: None,
            });
        }
    }

    // The model kept calling tools for the maximum number of rounds; return
    // the last text it produced (possibly empty) rather than looping on.
    let last = conversation
        .iter()
        .rev()
        .find(|m| m.role == "assistant")
        .map(|m| m.content.clone())
        .unwrap_or_default();
    Ok(ChatReply {
        content: last,
        reasoning: None,
        usage: None,
    })
}

/// Summarizes a conversation into a concise summary, to be used as a
/// replacement for the full history when the context window is nearly full.
/// This is a non-streaming call (the summary is short and the user is not
/// watching it arrive token by token).
pub fn summarize_conversation(
    api_key: &str,
    model: &str,
    messages: &[ChatMessage],
) -> Result<String, AppError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| AppError::Http(e.to_string()))?;

    // Build a request that asks the model to summarize the conversation.
    let summary_prompt = ChatMessage {
        role: "system".to_string(),
        content: "Summarize the conversation above concisely. Preserve key \
                  decisions, file paths, code changes, and any unresolved \
                  questions. Keep it under 500 words. Do not include the \
                  summary instructions in your output."
            .to_string(),
        tool_call_id: None,
        tool_calls: None,
    };
    let mut all_messages = messages.to_vec();
    all_messages.push(summary_prompt);

    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&ChatRequest {
            model: model.to_string(),
            messages: all_messages,
            tools: Vec::new(),
            stream: false,
            stream_options: None,
        })
        .send()
        .map_err(|e| AppError::Http(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http(format!(
            "OpenRouter returned {status} during summarization"
        )));
    }

    let body: serde_json::Value = response.json().map_err(|e| AppError::Http(e.to_string()))?;
    let content = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();

    if content.is_empty() {
        return Err(AppError::Http(
            "summarization returned empty content".to_string(),
        ));
    }
    Ok(content)
}

/// Parses a single server-sent-events data payload into a `(ChatChunk,
/// Option<TokenUsage>)` tuple, or `None` when the payload carries no
/// incremental content (e.g. a role-only delta or an empty choice). The final
/// chunk may carry `usage` even when its delta is empty, so a usage-only
/// payload is still returned.
fn parse_stream_chunk(payload: &str) -> Result<Option<(ChatChunk, Option<TokenUsage>)>, AppError> {
    let chunk: StreamChunk =
        serde_json::from_str(payload).map_err(|e| AppError::Http(e.to_string()))?;
    let usage = chunk.usage.map(|u| TokenUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
    });
    let Some(delta) = chunk.choices.into_iter().next().map(|c| c.delta) else {
        // No choices: this is a usage-only final chunk (or an empty payload).
        return Ok(usage.map(|u| {
            (
                ChatChunk {
                    content: String::new(),
                    reasoning: String::new(),
                    tool_calls: Vec::new(),
                },
                Some(u),
            )
        }));
    };
    let content = delta.content.unwrap_or_default();
    let reasoning = delta.reasoning.unwrap_or_default();
    let tool_calls: Vec<ToolCallDelta> = delta
        .tool_calls
        .into_iter()
        .map(|tc| {
            let function = tc.function;
            ToolCallDelta {
                index: tc.index,
                id: tc.id,
                name: function.as_ref().and_then(|f| f.name.clone()),
                arguments: function.and_then(|f| f.arguments).unwrap_or_default(),
            }
        })
        .collect();
    if content.is_empty() && reasoning.is_empty() && tool_calls.is_empty() && usage.is_none() {
        return Ok(None);
    }
    Ok(Some((
        ChatChunk {
            content,
            reasoning,
            tool_calls,
        },
        usage,
    )))
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
    fn parses_stream_chunk_with_content() {
        let json = r#"{ "choices": [ {"delta": {"role": "assistant", "content": "Hel"}} ] }"#;
        let (chunk, usage) = parse_stream_chunk(json).unwrap().unwrap();
        assert_eq!(chunk.content, "Hel");
        assert_eq!(chunk.reasoning, "");
        assert!(usage.is_none());
    }

    #[test]
    fn parses_stream_chunk_with_reasoning() {
        let json = r#"{ "choices": [ {"delta": {"reasoning": "Let me think."}} ] }"#;
        let (chunk, _) = parse_stream_chunk(json).unwrap().unwrap();
        assert_eq!(chunk.content, "");
        assert_eq!(chunk.reasoning, "Let me think.");
    }

    #[test]
    fn empty_stream_chunk_is_none() {
        // A role-only delta (no content or reasoning) yields no chunk.
        let json = r#"{ "choices": [ {"delta": {"role": "assistant"}} ] }"#;
        assert_eq!(parse_stream_chunk(json).unwrap(), None);
        // A payload with no choices also yields no chunk.
        assert_eq!(parse_stream_chunk(r#"{"choices": []}"#).unwrap(), None);
    }

    #[test]
    fn parses_stream_chunk_with_tool_call() {
        let json = r#"{ "choices": [ {"delta": {"tool_calls": [
            {"index": 0, "id": "call_1", "function": {"name": "get_local_time", "arguments": ""}}
        ]}} ] }"#;
        let (chunk, _) = parse_stream_chunk(json).unwrap().unwrap();
        assert_eq!(chunk.content, "");
        assert_eq!(chunk.tool_calls.len(), 1);
        assert_eq!(chunk.tool_calls[0].index, 0);
        assert_eq!(chunk.tool_calls[0].id.as_deref(), Some("call_1"));
        assert_eq!(chunk.tool_calls[0].name.as_deref(), Some("get_local_time"));
        assert_eq!(chunk.tool_calls[0].arguments, "");
    }

    #[test]
    fn tool_call_deltas_merge_into_complete_calls() {
        // First fragment carries the id and name; later ones stream arguments.
        let mut calls: Vec<ToolCall> = Vec::new();
        merge_tool_call_delta(
            &mut calls,
            &ToolCallDelta {
                index: 0,
                id: Some("call_1".to_string()),
                name: Some("get_local_time".to_string()),
                arguments: String::new(),
            },
        );
        merge_tool_call_delta(
            &mut calls,
            &ToolCallDelta {
                index: 0,
                id: None,
                name: None,
                arguments: "{}".to_string(),
            },
        );
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].name, "get_local_time");
        assert_eq!(calls[0].arguments, "{}");
    }
}
