import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export interface TreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  /// Whether the backend classifies this file as a renderable image. The
  /// backend is the single source of truth for image detection.
  is_image: boolean;
  /// Whether this directory is excluded (large/generated/VCS). Shown greyed
  /// out and not expandable.
  is_excluded: boolean;
  children?: TreeNode[];
}

export function listTree(): Promise<TreeNode[]> {
  return invoke<TreeNode[]>("list_tree");
}

/// Returns the current root directory, or `null` if no folder is open.
export function getRoot(): Promise<string | null> {
  return invoke<string | null>("get_root");
}

export interface RecentRoot {
  path: string;
}

/// Returns the list of recently opened roots, most recent first.
export function recentRoots(): Promise<RecentRoot[]> {
  return invoke<RecentRoot[]>("recent_roots");
}

export function setRoot(path: string): Promise<string> {
  return invoke<string>("set_root", { path });
}

/// Clears the current root directory.
export function closeRoot(): Promise<void> {
  return invoke<void>("close_root");
}

/// Opens the native folder picker and, if a folder is chosen, sets it as the
/// new root. Resolves to the new root path, or `null` if the user cancelled.
/// `defaultPath` is the directory the picker opens in (typically the current
/// root).
export async function pickRootFolder(
  defaultPath?: string,
): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    defaultPath,
  });
  if (typeof selected !== "string") {
    return null;
  }
  return setRoot(selected);
}

export function readFile(path: string): Promise<string> {
  return invoke<string>("read_file", { path });
}

export function writeFile(path: string, contents: string): Promise<void> {
  return invoke<void>("write_file", { path, contents });
}

export interface FileData {
  data: string;
  mime_type: string;
}

export function readFileData(path: string): Promise<FileData> {
  return invoke<FileData>("read_file_data", { path });
}

/// A model available on the user's OpenRouter account.
export interface Model {
  id: string;
  name: string;
  /// The provider, derived from the id prefix by the backend (e.g. `openai`).
  provider: string;
  context_length?: number;
}

/// Stores the user's OpenRouter API key (empty string clears it). Returns the
/// masked key, or an empty string when cleared.
export function setKey(key: string): Promise<string> {
  return invoke<string>("set_key", { key });
}

/// Returns the masked OpenRouter API key, or `null` if none is stored.
export function getKey(): Promise<string | null> {
  return invoke<string | null>("get_key");
}

/// Fetches the models available on the user's OpenRouter account.
export function listModels(): Promise<Model[]> {
  return invoke<Model[]>("list_models");
}

/// A tool the agent can call: its name and a human-facing description.
export interface ToolInfo {
  name: string;
  description: string;
}

/// Returns the tools the agent can call, in registry order.
export function listTools(): Promise<ToolInfo[]> {
  return invoke<ToolInfo[]>("list_tools");
}

/// A single chat message sent to or received from the model.
export interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

/// Token usage for a chat completion, as reported by the API.
export interface TokenUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

/// The assistant's reply to a chat completion.
export interface ChatReply {
  /// The visible reply text.
  content: string;
  /// The model's chain-of-thought, if it produced any (reasoning models).
  reasoning: string | null;
  /// Token usage for the final round, if the API reported it.
  usage?: TokenUsage | null;
}

/// A single incremental chunk from a streaming chat completion.
export interface ChatChunk {
  /// The incremental visible reply text (may be empty).
  content: string;
  /// The incremental chain-of-thought text (may be empty).
  reasoning: string;
}

/// Sends a streaming chat completion to OpenRouter. Chunks are emitted to the
/// frontend as `chat:chunk` events (see `listenChatChunk`) as they arrive; the
/// promise resolves with the accumulated reply when the stream ends. Requires
/// a stored API key. `agentPrompt` is an optional custom system prompt layered
/// on top of the base agent prompt.
export function chat(
  model: string,
  messages: ChatMessage[],
  agentPrompt?: string | null,
): Promise<ChatReply> {
  return invoke<ChatReply>("chat", {
    model,
    messages,
    agentPrompt: agentPrompt ?? null,
  });
}

/// Subscribes to streaming chat chunks. Returns an unlisten function.
export function listenChatChunk(
  handler: (chunk: ChatChunk) => void,
): Promise<() => void> {
  return listen<ChatChunk>("chat:chunk", (event) => handler(event.payload));
}

/// A tool call the agent made during a chat turn, emitted as a `chat:tool`
/// event when the call is executed.
export interface ToolCallEvent {
  /// The tool's name (e.g. `get_local_time`).
  name: string;
  /// The tool's result (or error text when the call failed).
  result: string;
}

/// Subscribes to the agent's tool calls. Returns an unlisten function.
export function listenChatTool(
  handler: (tool: ToolCallEvent) => void,
): Promise<() => void> {
  return listen<ToolCallEvent>("chat:tool", (event) => handler(event.payload));
}

/// A single message within a persisted chat session.
export interface SessionMessage {
  role: "user" | "agent";
  text: string;
  /// The model's chain-of-thought, present only on agent messages from
  /// reasoning models.
  thinking?: string | null;
}

/// A persisted chat session.
export interface ChatSession {
  id: string;
  title: string;
  messages: SessionMessage[];
}

/// A user-defined custom agent: a named system prompt.
export interface CustomAgent {
  id: string;
  name: string;
  prompt: string;
}

/// Persisted user settings (global, not tied to a project).
export interface Settings {
  /// The id of the selected chat model, or `null` when none is chosen.
  model_id: string | null;
  /// The user's custom agents.
  agents: CustomAgent[];
  /// The id of the currently active custom agent, or `null` for default.
  active_agent_id: string | null;
}

/// Returns the persisted user settings.
export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

/// Persists the user settings (the selected model id) in a single write.
export function saveSettings(settings: Settings): Promise<void> {
  return invoke<void>("save_settings", { settings });
}

/// Returns the chat sessions for a project, given its root path.
export function getProjectSessions(root: string): Promise<ChatSession[]> {
  return invoke<ChatSession[]>("get_project_sessions", { root });
}

/// Persists the chat sessions for a project, given its root path. The caller
/// owns the complete session list and sends it whole.
export function saveProjectSessions(
  root: string,
  sessions: ChatSession[],
): Promise<void> {
  return invoke<void>("save_project_sessions", { root, sessions });
}
