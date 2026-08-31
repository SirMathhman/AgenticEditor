mod core;

use core::tree::TreeNode;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Returns the directory tree of the current working directory.
/// Runs on a blocking thread so large directories don't freeze the UI.
#[tauri::command]
async fn list_tree() -> Result<Vec<TreeNode>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let dir = std::env::current_dir().map_err(|e| e.to_string())?;
        core::tree::read_dir_tree(&dir, 0)
    })
    .await
    .map_err(|e| e.to_string())?
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
