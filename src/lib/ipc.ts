import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export interface TreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  /// Whether the backend classifies this file as a renderable image. The
  /// backend is the single source of truth for image detection.
  is_image: boolean;
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
