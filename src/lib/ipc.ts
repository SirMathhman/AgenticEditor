import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export interface TreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  children?: TreeNode[];
}

export function listTree(): Promise<TreeNode[]> {
  return invoke<TreeNode[]>("list_tree");
}

export function getRoot(): Promise<string> {
  return invoke<string>("get_root");
}

export function setRoot(path: string): Promise<string> {
  return invoke<string>("set_root", { path });
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

const IMAGE_EXTENSIONS = [
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "bmp",
  "ico",
  "svg",
];

export function isImagePath(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return IMAGE_EXTENSIONS.includes(ext);
}
