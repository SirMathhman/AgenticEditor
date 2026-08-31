mod core;

use core::errors::AppError;
use core::tree::{FileData, TreeNode};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

/// The directory that all relative file paths are resolved against, plus a
/// generation counter that increments on every root change. Starts with no
/// folder open; can be set at runtime via `set_root` or cleared via
/// `close_root`.
type RootState = Mutex<(Option<PathBuf>, u64)>;

fn initial_root() -> (Option<PathBuf>, u64) {
    (None, 0)
}

/// Clones the current root and its generation, erroring if no folder is open.
fn current_root(state: &tauri::State<'_, RootState>) -> Result<(PathBuf, u64), AppError> {
    let guard = state.lock().map_err(|_| AppError::Poisoned)?;
    let root = guard.0.clone().ok_or(AppError::NoRoot)?;
    Ok((root, guard.1))
}

/// Returns the directory tree of the current root.
/// Runs on a blocking thread so large directories don't freeze the UI.
#[tauri::command]
async fn list_tree(state: tauri::State<'_, RootState>) -> Result<Vec<TreeNode>, AppError> {
    let (root, gen) = current_root(&state)?;
    let result = tauri::async_runtime::spawn_blocking({
        let root = root.clone();
        move || core::tree::read_dir_tree(&root, 0)
    })
    .await
    .map_err(|e| AppError::Io(std::io::Error::other(e), root))??;
    if state.lock().map_err(|_| AppError::Poisoned)?.1 != gen {
        return Err(AppError::StaleRoot);
    }
    Ok(result)
}

/// Reads a file's contents, given its path relative to the current root.
#[tauri::command]
async fn read_file(state: tauri::State<'_, RootState>, path: String) -> Result<String, AppError> {
    let (root, gen) = current_root(&state)?;
    let result = tauri::async_runtime::spawn_blocking({
        let root = root.clone();
        move || core::tree::read_file_at(&root, &path)
    })
    .await
    .map_err(|e| AppError::Io(std::io::Error::other(e), root))??;
    if state.lock().map_err(|_| AppError::Poisoned)?.1 != gen {
        return Err(AppError::StaleRoot);
    }
    Ok(result)
}

/// Reads an image file's raw bytes (base64-encoded), given its path relative
/// to the current root.
#[tauri::command]
async fn read_file_data(
    state: tauri::State<'_, RootState>,
    path: String,
) -> Result<FileData, AppError> {
    let (root, gen) = current_root(&state)?;
    let result = tauri::async_runtime::spawn_blocking({
        let root = root.clone();
        move || core::tree::read_file_data(&root, &path)
    })
    .await
    .map_err(|e| AppError::Io(std::io::Error::other(e), root))??;
    if state.lock().map_err(|_| AppError::Poisoned)?.1 != gen {
        return Err(AppError::StaleRoot);
    }
    Ok(result)
}

/// Writes `contents` to a file, given its path relative to the current root.
/// Creates the file if it does not exist.
#[tauri::command]
async fn write_file(
    state: tauri::State<'_, RootState>,
    path: String,
    contents: String,
) -> Result<(), AppError> {
    let (root, gen) = current_root(&state)?;
    tauri::async_runtime::spawn_blocking({
        let root = root.clone();
        move || core::tree::write_file_at(&root, &path, &contents)
    })
    .await
    .map_err(|e| AppError::Io(std::io::Error::other(e), root))??;
    if state.lock().map_err(|_| AppError::Poisoned)?.1 != gen {
        return Err(AppError::StaleRoot);
    }
    Ok(())
}

/// Sets the root directory that all relative file paths resolve against.
/// Returns the canonicalized root path.
#[tauri::command]
async fn set_root(state: tauri::State<'_, RootState>, path: String) -> Result<PathBuf, AppError> {
    let new_root = PathBuf::from(path);
    if !new_root.is_dir() {
        return Err(AppError::NotADirectory(new_root));
    }
    let canonical = new_root
        .canonicalize()
        .map_err(|e| AppError::Io(e, new_root))?;
    let mut guard = state.lock().map_err(|_| AppError::Poisoned)?;
    guard.0 = Some(canonical.clone());
    guard.1 += 1;
    Ok(canonical)
}

/// Clears the root directory. The app then has no current folder until a new
/// one is opened.
#[tauri::command]
async fn close_root(state: tauri::State<'_, RootState>) -> Result<(), AppError> {
    let mut guard = state.lock().map_err(|_| AppError::Poisoned)?;
    guard.0 = None;
    guard.1 += 1;
    Ok(())
}

/// Returns the current root directory, or `None` if no folder is open.
#[tauri::command]
async fn get_root(state: tauri::State<'_, RootState>) -> Result<Option<PathBuf>, AppError> {
    Ok(state.lock().map_err(|_| AppError::Poisoned)?.0.clone())
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
            close_root,
            get_root
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
