mod core;

use core::tree::{FileData, TreeNode};
use tauri::Manager;

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

/// Reads a file's contents, given its path relative to the current working directory.
#[tauri::command]
async fn read_file(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = std::env::current_dir().map_err(|e| e.to_string())?;
        core::tree::read_file_at(&dir, &path)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reads an image file's raw bytes (base64-encoded), given its path relative
/// to the current working directory.
#[tauri::command]
async fn read_file_data(path: String) -> Result<FileData, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = std::env::current_dir().map_err(|e| e.to_string())?;
        core::tree::read_file_data(&dir, &path)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Writes `contents` to a file, given its path relative to the current working
/// directory. Creates the file if it does not exist.
#[tauri::command]
async fn write_file(path: String, contents: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = std::env::current_dir().map_err(|e| e.to_string())?;
        core::tree::write_file_at(&dir, &path, &contents)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens the main window filling the leftmost monitor.
fn setup_leftmost_monitor(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let window = app.get_webview_window("main").expect("main window");
    if let Ok(monitors) = app.available_monitors() {
        if let Some(leftmost) = monitors.iter().min_by_key(|m| m.position().x) {
            let pos = leftmost.position();
            let size = leftmost.size();
            window
                .set_position(tauri::PhysicalPosition::new(pos.x, pos.y))
                .ok();
            window
                .set_size(tauri::PhysicalSize::new(size.width, size.height))
                .ok();
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_mcp_bridge::init());
    }

    builder
        .setup(|app| setup_leftmost_monitor(app))
        .invoke_handler(tauri::generate_handler![
            list_tree,
            read_file,
            read_file_data,
            write_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
