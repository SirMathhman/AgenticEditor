import { invoke } from "@tauri-apps/api/core";
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

/// A single chat message sent to or received from the model.
export interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

/// Sends a chat completion to OpenRouter and returns the assistant's reply.
/// Requires a stored API key.
export function chat(model: string, messages: ChatMessage[]): Promise<string> {
  return invoke<string>("chat", { model, messages });
}

/// A single message within a persisted chat session.
export interface SessionMessage {
  role: "user" | "agent";
  text: string;
}

/// A persisted chat session.
export interface ChatSession {
  id: string;
  title: string;
  messages: SessionMessage[];
}

/// Persisted user settings.
export interface Settings {
  /// The id of the selected chat model, or `null` when none is chosen.
  model_id: string | null;
  /// The persisted chat sessions.
  sessions: ChatSession[];
}

/// Returns the persisted user settings.
export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

/// Persists the full settings (model id and chat sessions) in a single write.
/// The caller owns the complete settings state and sends it whole.
export function saveSettings(settings: Settings): Promise<void> {
  return invoke<void>("save_settings", { settings });
}
