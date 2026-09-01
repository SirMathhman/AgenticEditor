//! OpenRouter integration: fetch the list of models available to the user's
//! account. The API key is supplied by the user at runtime (see the `set_key`
//! command) and is never hardcoded.

use std::io::Read;
use std::path::Path;

use super::errors::AppError;
use super::settings::{CustomAgent, Provider};

/// The default base URL for a local llama.cpp server.
pub const DEFAULT_LLAMA_BASE_URL: &str = "http://localhost:8080";

/// The connection details for a model provider: the base URL to call and the
/// optional API key (OpenRouter requires one; a local llama.cpp server does
/// not). Both providers speak the OpenAI-compatible API, so the request and
/// streaming code is shared — only these differ.
#[derive(Clone)]
pub struct ProviderConfig {
    /// Which provider this config targets. Drives the error label and (via
    /// `new`) the validation rules.
    provider: Provider,
    /// The base URL without a trailing path (e.g. `https://openrouter.ai/api/v1`
    /// or `http://localhost:8080`).
    base_url: String,
    /// The API key, if the provider needs one.
    api_key: Option<String>,
}

impl ProviderConfig {
    /// Builds the config for a provider, validating that the provider's
    /// requirements are met (OpenRouter needs an API key). For llama.cpp, an
    /// empty `base_url` falls back to the default local server address.
    pub fn new(
        provider: Provider,
        base_url: &str,
        api_key: Option<&str>,
    ) -> Result<Self, AppError> {
        match provider {
            Provider::OpenRouter => {
                let Some(key) = api_key.filter(|k| !k.trim().is_empty()) else {
                    return Err(AppError::Http(
                        "no OpenRouter API key set — add one in the chat panel".to_string(),
                    ));
                };
                Ok(ProviderConfig {
                    provider,
                    base_url: "https://openrouter.ai/api/v1".to_string(),
                    api_key: Some(key.to_string()),
                })
            }
            Provider::LlamaCpp => Ok(ProviderConfig {
                provider,
                base_url: if base_url.trim().is_empty() {
                    DEFAULT_LLAMA_BASE_URL.to_string()
                } else {
                    base_url.trim_end_matches('/').to_string()
                },
                api_key: None,
            }),
        }
    }

    /// The URL for the provider's model list.
    fn models_url(&self) -> String {
        format!("{}/models", self.base_url.trim_end_matches('/'))
    }

    /// The URL for the provider's chat-completions endpoint.
    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// Adds the provider's auth header to a request builder, if it needs one.
    fn apply_auth(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        match &self.api_key {
            Some(key) => request.header("Authorization", format!("Bearer {key}")),
            None => request,
        }
    }

    /// A short label for error messages (e.g. `OpenRouter`, `the llama.cpp server`).
    fn label(&self) -> &'static str {
        match self.provider {
            Provider::OpenRouter => "OpenRouter",
            Provider::LlamaCpp => "the llama.cpp server",
        }
    }

    /// The provider this config targets.
    pub fn provider(&self) -> Provider {
        self.provider
    }
}

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

/// Fetches the models available from a provider (OpenRouter or a local
/// llama.cpp server). Both expose an OpenAI-compatible `/models` endpoint.
///
/// This is a network call, so it must be run on a blocking thread (see the
/// `list_models` command). For OpenRouter the key is sent in the
/// `Authorization` header and is never logged or persisted by this function.
pub fn fetch_models(config: &ProviderConfig) -> Result<Vec<Model>, AppError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Http(e.to_string()))?;

    let response = config
        .apply_auth(client.get(config.models_url()))
        .header("Accept", "application/json")
        .send()
        .map_err(|e| AppError::Http(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http(format!(
            "{} returned {status}",
            config.label()
        )));
    }

    let body: ModelsResponse = response.json().map_err(|e| AppError::Http(e.to_string()))?;

    // OpenRouter ids carry a `provider/` prefix; llama.cpp ids don't, so label
    // those models with the provider name directly.
    let provider_label = match config.provider() {
        Provider::OpenRouter => None,
        Provider::LlamaCpp => Some("llama.cpp".to_string()),
    };

    Ok(body
        .data
        .into_iter()
        .map(|m| {
            let name = m.name.unwrap_or_else(|| m.id.clone());
            Model {
                provider: provider_label.clone().unwrap_or_else(|| provider_of(&m.id)),
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

/// Sends a streaming chat completion to the configured provider (OpenRouter or
/// a local llama.cpp server), invoking `on_chunk` for each incremental chunk as
/// it arrives. Returns the accumulated reply.
///
/// This is a network call, so it must be run on a blocking thread (see the
/// `chat` command). For OpenRouter the key is sent in the `Authorization`
/// header and is never logged or persisted by this function.
fn chat_completion_stream(
    config: &ProviderConfig,
    model: &str,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    on_chunk: &mut dyn FnMut(&ChatChunk),
) -> Result<StreamReply, AppError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::Http(e.to_string()))?;

    let mut response = config
        .apply_auth(client.post(config.chat_url()))
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
        return Err(AppError::Http(format!(
            "{} returned {status}",
            config.label()
        )));
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
/// Shared context an agent (main or subagent) needs to run its tool loop.
///
/// `is_main_agent` is the single source of truth for whether this agent may
/// spawn subagents: it controls both the tool-spec filtering (subagents are not
/// shown `spawn_subagent`) and the recursion guard in `execute_agent_tool`.
/// `on_subagent` carries the callback that streams a spawned subagent's progress
/// to the UI; it is set only for the main agent.
pub struct AgentContext<'a> {
    /// The provider connection details (base URL + optional key).
    pub config: &'a ProviderConfig,
    pub model: &'a str,
    pub root: &'a Path,
    pub memory_root: &'a Path,
    /// The user's predefined custom agents, so `spawn_subagent` can resolve a
    /// `role` that matches one of them by name.
    pub agents: &'a [CustomAgent],
    /// Whether this is the top-level agent (may spawn subagents) or a subagent
    /// (may not). Drives tool-spec filtering and the recursion guard.
    pub is_main_agent: bool,
    pub on_subagent: Option<&'a mut dyn FnMut(SubagentEvent)>,
}

/// A single streamed event from a subagent's run, surfaced to the UI so its
/// work is visible as a nested tool call.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SubagentEvent {
    /// The subagent started; `role` is the resolved role/instructions.
    Start { role: String },
    /// A streamed content or reasoning delta from the subagent.
    Chunk { content: String, reasoning: String },
    /// A tool call the subagent completed, with its result.
    Tool { name: String, result: String },
    /// The subagent finished; `result` is its final answer (or an error string).
    End { result: String },
}

/// A single streamed event from any agent's tool loop. The main agent maps
/// these to its own UI events; a subagent re-emits them as nested
/// [`SubagentEvent`]s so its work is visible inside the `spawn_subagent` call.
enum AgentEvent {
    /// A streamed content or reasoning delta.
    Chunk(ChatChunk),
    /// A tool call that completed, with its result.
    Tool { name: String, result: String },
}

/// Cap for a custom agent/subagent prompt so a very long prompt cannot blow up
/// the context window. Generous (4000 chars ≈ 1000 tokens) but prevents
/// accidental or malicious prompt bloat.
const MAX_AGENT_PROMPT_CHARS: usize = 4000;

/// Build the main agent's system prompt: working-directory context plus, when
/// present, the user's custom agent prompt layered on top.
fn main_system_prompt(root: &Path, agent_prompt: Option<&str>) -> String {
    let base_prompt = format!(
        "You are an agent working inside the project folder `{}`. \
         File and directory tool paths are relative to that folder. \
         You can also run PowerShell commands in that folder with the \
         run_command tool. Use the tools to inspect and modify the \
         project rather than guessing at its contents. \
         You have a persistent memory (the `memory` tool) that survives \
         across chat sessions: use it to record decisions, conventions, and \
         context worth remembering, and consult it at the start of a task \
         before assuming you have no prior context. \
         You can delegate self-contained subtasks to a subagent with the \
         spawn_subagent tool; it works in the same folder with the same tools \
         and returns a single final answer. Give it a complete, self-contained \
         task, since it does not see this conversation.",
        root.display()
    );
    layer_custom_prompt(&base_prompt, agent_prompt)
}

/// Build a subagent's system prompt: working-directory context plus the
/// resolved role/instructions for its delegated task.
fn subagent_system_prompt(root: &Path, role: Option<&str>) -> String {
    let base_prompt = format!(
        "You are a focused subagent working inside the project folder `{}`. \
         File and directory tool paths are relative to that folder. \
         You can also run PowerShell commands in that folder with the \
         run_command tool. You have a persistent memory (the `memory` tool). \
         You were delegated a single, self-contained task. Complete it using \
         the tools, then finish with a concise final answer summarizing what \
         you did and any results. Do not spawn further subagents.",
        root.display()
    );
    layer_custom_prompt(&base_prompt, role)
}

/// Append a custom prompt to a base system prompt, trimming and capping it so
/// it cannot blow up the context window. Returns the base unchanged when the
/// custom prompt is empty.
fn layer_custom_prompt(base: &str, custom: Option<&str>) -> String {
    match custom {
        Some(prompt) if !prompt.trim().is_empty() => {
            let trimmed = prompt.trim();
            let capped: String = trimmed.chars().take(MAX_AGENT_PROMPT_CHARS).collect();
            format!("{base}\n\n{capped}")
        }
        _ => base.to_string(),
    }
}

/// Run a chat completion with tool calling. The model may call tools in a
/// bounded loop (up to `MAX_TOOL_ROUNDS` rounds); each tool result is fed back
/// and the model continues until it produces a final text answer or the round
/// limit is hit.
///
/// `on_chunk` is invoked for every streamed content/reasoning delta so the UI
/// can render tokens live. `on_tool` is invoked after each tool call completes,
/// with the tool name and its result string, so the UI can show what the agent
/// did. `on_subagent` receives the subagent's progress when the main agent
/// calls `spawn_subagent`.
#[allow(clippy::too_many_arguments)]
pub fn chat_with_tools(
    config: &ProviderConfig,
    model: &str,
    root: &Path,
    memory_root: &Path,
    agents: &[CustomAgent],
    messages: &[ChatMessage],
    agent_prompt: Option<&str>,
    on_chunk: &mut dyn FnMut(&ChatChunk),
    on_tool: &mut dyn FnMut(&str, &str),
    on_subagent: &mut dyn FnMut(SubagentEvent),
) -> Result<ChatReply, AppError> {
    let system_content = main_system_prompt(root, agent_prompt);
    let mut ctx = AgentContext {
        config,
        model,
        root,
        memory_root,
        agents,
        is_main_agent: true,
        on_subagent: Some(on_subagent),
    };
    // A single sink for the main agent's loop: chunks and tool calls go to the
    // UI directly. Subagent progress is routed through `ctx.on_subagent` by
    // `execute_agent_tool`.
    let mut on_event = |ev: AgentEvent| match ev {
        AgentEvent::Chunk(c) => on_chunk(&c),
        AgentEvent::Tool { name, result } => on_tool(&name, &result),
    };
    run_tool_loop(&mut ctx, messages, &system_content, &mut on_event)
}

/// The shared bounded tool loop used by both the main agent and subagents. All
/// streamed output (content deltas and completed tool calls) is reported
/// through the single `on_event` sink, which keeps the callback surface to one
/// mutable borrow — important because a subagent's own loop reuses this same
/// function while its parent still holds a borrow of the subagent sink.
fn run_tool_loop(
    ctx: &mut AgentContext<'_>,
    messages: &[ChatMessage],
    system_content: &str,
    on_event: &mut dyn FnMut(AgentEvent),
) -> Result<ChatReply, AppError> {
    // Cap the number of tool rounds so a model that keeps calling tools
    // cannot loop forever (each round is a full network round trip).
    const MAX_TOOL_ROUNDS: usize = 5;
    // Only the main agent may spawn subagents; hide the tool from subagents so
    // they don't call something that would fail.
    let tools = if ctx.is_main_agent {
        tool_specs()
    } else {
        tool_specs()
            .into_iter()
            .filter(|t| t.function.name != "spawn_subagent")
            .collect()
    };

    let mut conversation: Vec<ChatMessage> = vec![ChatMessage {
        role: "system".to_string(),
        content: system_content.to_string(),
        tool_call_id: None,
        tool_calls: None,
    }];
    conversation.extend_from_slice(messages);

    for _ in 0..MAX_TOOL_ROUNDS {
        // Stream this round's chunks straight into the event sink.
        let mut on_chunk = |c: &ChatChunk| on_event(AgentEvent::Chunk(c.clone()));
        let reply =
            chat_completion_stream(ctx.config, ctx.model, &conversation, &tools, &mut on_chunk)?;

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
            let result = execute_agent_tool(ctx, &call.name, &call.arguments)
                .unwrap_or_else(|e| e.to_string());
            on_event(AgentEvent::Tool {
                name: call.name.clone(),
                result: result.clone(),
            });
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

/// Dispatch a tool call for an agent. `spawn_subagent` is only available to the
/// main agent (it needs the subagent callback); every other tool goes through
/// the shared registry.
fn execute_agent_tool(
    ctx: &mut AgentContext<'_>,
    name: &str,
    arguments: &str,
) -> Result<String, AppError> {
    if name == "spawn_subagent" {
        // Only the main agent may spawn subagents (no recursion). The tool is
        // hidden from subagents' specs, so this guard is a backstop against a
        // model that calls it anyway.
        if !ctx.is_main_agent {
            return Err(AppError::Tool(
                "Subagents cannot spawn further subagents".into(),
            ));
        }
        // Copy out the connection details (all `Copy` references) before taking
        // a mutable borrow of the subagent sink, so the two borrows don't
        // overlap.
        let config = ctx.config;
        let model = ctx.model;
        let root = ctx.root;
        let memory_root = ctx.memory_root;
        let agents = ctx.agents;
        let sink = ctx
            .on_subagent
            .as_deref_mut()
            .ok_or_else(|| AppError::Tool("Subagents cannot spawn further subagents".into()))?;
        return run_subagent(config, model, root, memory_root, agents, arguments, sink);
    }
    super::tools::execute_tool(ctx.root, ctx.memory_root, name, arguments)
}

/// Run a subagent to completion and stream its progress through `on_event`.
/// The subagent uses the same tools as the main agent (minus `spawn_subagent`)
/// and returns its final answer as a single string. It takes the parent's
/// connection details by value (all `Copy` references) so the caller can keep
/// a mutable borrow of the parent's subagent sink while the subagent runs.
fn run_subagent(
    config: &ProviderConfig,
    model: &str,
    root: &Path,
    memory_root: &Path,
    agents: &[CustomAgent],
    arguments: &str,
    on_event: &mut dyn FnMut(SubagentEvent),
) -> Result<String, AppError> {
    let task = super::tools::arg_str(arguments, "task")?;
    let role = super::tools::arg_str_opt(arguments, "role")?;

    // Resolve the subagent's instructions: if `role` matches a predefined
    // custom agent by name, use that agent's prompt; otherwise treat `role` as
    // free-form instructions.
    let resolved_role = match &role {
        Some(r) if !r.trim().is_empty() => match agents.iter().find(|a| a.name == *r) {
            Some(a) => a.prompt.clone(),
            None => format!("You are a focused subagent. {r}"),
        },
        _ => String::new(),
    };

    let display_role = role.clone().unwrap_or_else(|| "subagent".into());
    on_event(SubagentEvent::Start {
        role: display_role.clone(),
    });

    // Build the subagent's context: same root/memory/tools, but no ability to
    // spawn further subagents.
    let mut sub_ctx = AgentContext {
        config,
        model,
        root,
        memory_root,
        agents,
        is_main_agent: false,
        on_subagent: None,
    };

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: task.to_string(),
        tool_call_id: None,
        tool_calls: None,
    }];
    let system_content = subagent_system_prompt(root, Some(&resolved_role));

    // Re-emit the subagent's loop events as nested `SubagentEvent`s so its work
    // is visible inside the `spawn_subagent` call.
    let mut on_event2 = |ev: AgentEvent| match ev {
        AgentEvent::Chunk(c) => {
            if !c.content.is_empty() || !c.reasoning.is_empty() {
                on_event(SubagentEvent::Chunk {
                    content: c.content,
                    reasoning: c.reasoning,
                });
            }
        }
        AgentEvent::Tool { name, result } => {
            on_event(SubagentEvent::Tool { name, result });
        }
    };

    let result = run_tool_loop(&mut sub_ctx, &messages, &system_content, &mut on_event2);

    match result {
        Ok(reply) => {
            let final_text = if reply.content.trim().is_empty() {
                "(subagent finished with no final answer)".to_string()
            } else {
                reply.content
            };
            on_event(SubagentEvent::End {
                result: final_text.clone(),
            });
            Ok(final_text)
        }
        Err(e) => {
            let msg = e.to_string();
            on_event(SubagentEvent::End {
                result: msg.clone(),
            });
            Err(e)
        }
    }
}

/// Summarizes a conversation into a concise summary, to be used as a
/// replacement for the full history when the context window is nearly full.
/// This is a non-streaming call (the summary is short and the user is not
/// watching it arrive token by token).
pub fn summarize_conversation(
    config: &ProviderConfig,
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

    let response = config
        .apply_auth(client.post(config.chat_url()))
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
            "{} returned {status} during summarization",
            config.label()
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
