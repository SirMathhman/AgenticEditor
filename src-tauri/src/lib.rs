// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// A node in the directory tree.
#[derive(serde::Serialize)]
struct TreeNode {
    name: String,
    is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<TreeNode>>,
}

/// Directories that are never useful to list (large, generated, or VCS internals).
const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", "dist"];
/// Maximum recursion depth for the tree.
const MAX_DEPTH: usize = 4;

fn read_dir_tree(dir: &std::path::Path, depth: usize) -> Result<Vec<TreeNode>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut nodes = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        if file_type.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            let children = if depth < MAX_DEPTH {
                Some(read_dir_tree(&entry.path(), depth + 1)?)
            } else {
                None
            };
            nodes.push(TreeNode { name, is_dir: true, children });
        } else {
            nodes.push(TreeNode { name, is_dir: false, children: None });
        }
    }
    nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(nodes)
}

/// Returns the directory tree of the current working directory.
#[tauri::command]
fn list_tree() -> Result<Vec<TreeNode>, String> {
    let dir = std::env::current_dir().map_err(|e| e.to_string())?;
    read_dir_tree(&dir, 0)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_mcp_bridge::init());
    }

    builder
        .invoke_handler(tauri::generate_handler![greet, list_tree])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
