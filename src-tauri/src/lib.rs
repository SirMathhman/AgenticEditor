mod core;

use core::tree::{FileData, TreeNode};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

/// The directory that all relative file paths are resolved against.
/// Defaults to the process working directory; can be changed at runtime
/// via `set_root`.
type RootState = Mutex<PathBuf>;

fn initial_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Returns the directory tree of the current root.
/// Runs on a blocking thread so large directories don't freeze the UI.
#[tauri::command]
async fn list_tree(state: tauri::State<'_, RootState>) -> Result<Vec<TreeNode>, String> {
    let root = state.lock().map_err(|e| e.to_string())?.clone();
    tauri::async_runtime::spawn_blocking(move || core::tree::read_dir_tree(&root, 0))
        .await
        .map_err(|e| e.to_string())?
}

/// Reads a file's contents, given its path relative to the current root.
#[tauri::command]
async fn read_file(state: tauri::State<'_, RootState>, path: String) -> Result<String, String> {
    let root = state.lock().map_err(|e| e.to_string())?.clone();
    tauri::async_runtime::spawn_blocking(move || core::tree::read_file_at(&root, &path))
        .await
        .map_err(|e| e.to_string())?
}

/// Reads an image file's raw bytes (base64-encoded), given its path relative
/// to the current root.
#[tauri::command]
async fn read_file_data(
    state: tauri::State<'_, RootState>,
    path: String,
) -> Result<FileData, String> {
    let root = state.lock().map_err(|e| e.to_string())?.clone();
    tauri::async_runtime::spawn_blocking(move || core::tree::read_file_data(&root, &path))
        .await
        .map_err(|e| e.to_string())?
}

/// Writes `contents` to a file, given its path relative to the current root.
/// Creates the file if it does not exist.
#[tauri::command]
async fn write_file(
    state: tauri::State<'_, RootState>,
    path: String,
    contents: String,
) -> Result<(), String> {
    let root = state.lock().map_err(|e| e.to_string())?.clone();
    tauri::async_runtime::spawn_blocking(move || core::tree::write_file_at(&root, &path, &contents))
        .await
        .map_err(|e| e.to_string())?
}

/// Sets the root directory that all relative file paths resolve against.
/// Returns the canonicalized root path.
#[tauri::command]
async fn set_root(state: tauri::State<'_, RootState>, path: String) -> Result<PathBuf, String> {
    let new_root = PathBuf::from(path);
    if !new_root.is_dir() {
        return Err("not a directory".into());
    }
    let canonical = new_root.canonicalize().map_err(|e| e.to_string())?;
    *state.lock().map_err(|e| e.to_string())? = canonical.clone();
    Ok(canonical)
}

/// Returns the current root directory.
#[tauri::command]
async fn get_root(state: tauri::State<'_, RootState>) -> Result<PathBuf, String> {
    Ok(state.lock().map_err(|e| e.to_string())?.clone())
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
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(RootState::new(initial_root()));

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
            write_file,
            set_root,
            get_root
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
