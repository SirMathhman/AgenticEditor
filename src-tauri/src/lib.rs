mod core;

use core::errors::AppError;
use core::openrouter::{ChatMessage, ChatReply, Model};
use core::tree::{FileData, TreeNode};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

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

/// The file where recently opened roots are persisted.
fn recent_file(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| AppError::Io(std::io::Error::other(e), PathBuf::from("app config dir")))?;
    Ok(dir.join("recent.json"))
}

/// The file where user settings are persisted.
fn settings_file(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| AppError::Io(std::io::Error::other(e), PathBuf::from("app config dir")))?;
    Ok(dir.join("settings.json"))
}

/// The file where a project's chat sessions are persisted. It lives under the
/// app config dir (never inside the project itself), keyed by a hash of the
/// root path so each project has its own file.
fn project_sessions_file(
    app: &tauri::AppHandle,
    root: &std::path::Path,
) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| AppError::Io(std::io::Error::other(e), PathBuf::from("app config dir")))?;
    Ok(dir.join(core::settings::project_sessions_file_name(root)))
}

/// The credential-manager service and user used to store the OpenRouter API
/// key. The key lives in the OS credential store (Windows Credential Manager,
/// macOS Keychain, Linux Secret Service) rather than a plaintext file.
const KEY_SERVICE: &str = "com.mathm.tauri-app";
const KEY_USER: &str = "openrouter_key";

/// The keyring entry for the OpenRouter API key.
fn key_entry() -> Result<keyring::Entry, AppError> {
    keyring::Entry::new(KEY_SERVICE, KEY_USER).map_err(|e| AppError::Keyring(e.to_string()))
}

/// Reads the stored OpenRouter API key, if any.
fn load_key() -> Result<Option<String>, AppError> {
    match key_entry()?.get_password() {
        Ok(key) if !key.trim().is_empty() => Ok(Some(key.trim().to_string())),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Keyring(e.to_string())),
    }
}

/// Stores the OpenRouter API key in the OS credential manager. An empty key
/// clears the stored value.
fn save_key(key: &str) -> Result<(), AppError> {
    let entry = key_entry()?;
    if key.trim().is_empty() {
        // Deleting a missing entry is not an error.
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Keyring(e.to_string())),
        }
    } else {
        entry
            .set_password(key.trim())
            .map_err(|e| AppError::Keyring(e.to_string()))
    }
}

/// Stores the user's OpenRouter API key (or clears it when empty). Returns the
/// masked key so the UI can confirm what was saved without exposing the secret.
#[tauri::command]
async fn set_key(key: String) -> Result<String, AppError> {
    // The keyring call blocks, so run it off the async runtime.
    let stored = key.clone();
    tauri::async_runtime::spawn_blocking(move || save_key(&stored))
        .await
        .map_err(|e| AppError::Keyring(e.to_string()))??;
    Ok(if key.trim().is_empty() {
        String::new()
    } else {
        core::openrouter::mask_key(&key)
    })
}

/// Returns the masked OpenRouter API key, or `None` if no key is stored.
#[tauri::command]
async fn get_key() -> Result<Option<String>, AppError> {
    let key = tauri::async_runtime::spawn_blocking(load_key)
        .await
        .map_err(|e| AppError::Keyring(e.to_string()))??;
    Ok(key.map(|k| core::openrouter::mask_key(&k)))
}

/// Fetches the models available on the user's OpenRouter account. Requires a
/// stored API key. Runs the keyring read and the network call on blocking
/// threads.
#[tauri::command]
async fn list_models() -> Result<Vec<Model>, AppError> {
    let key = tauri::async_runtime::spawn_blocking(load_key)
        .await
        .map_err(|e| AppError::Keyring(e.to_string()))??
        .ok_or_else(|| {
            AppError::Http("no OpenRouter API key set — add one in the chat panel".to_string())
        })?;
    let result = tauri::async_runtime::spawn_blocking(move || core::openrouter::fetch_models(&key))
        .await
        .map_err(|e| AppError::Http(e.to_string()))?;
    result
}

/// A tool call the agent made, emitted to the frontend as a `chat:tool`
/// event so the UI can show what the agent did and with what result.
#[derive(serde::Serialize, Clone)]
struct ToolCallEvent {
    /// The tool's name (e.g. `get_local_time`).
    name: String,
    /// The tool's result (or error text when the call failed).
    result: String,
}

/// Sends a streaming chat completion to OpenRouter with the agent's tools
/// available. As each chunk arrives it is emitted to the frontend as a
/// `chat:chunk` event (so the reply can be rendered token by token); each
/// tool call the agent makes is emitted as a `chat:tool` event; when the
/// final stream ends the accumulated reply is returned. Requires a stored
/// API key. Runs the keyring read and the network call on blocking threads.
#[tauri::command]
async fn chat(
    app: tauri::AppHandle,
    state: tauri::State<'_, RootState>,
    model: String,
    messages: Vec<ChatMessage>,
    agent_prompt: Option<String>,
) -> Result<ChatReply, AppError> {
    let key = tauri::async_runtime::spawn_blocking(load_key)
        .await
        .map_err(|e| AppError::Keyring(e.to_string()))??
        .ok_or_else(|| {
            AppError::Http("no OpenRouter API key set — add one in the chat panel".to_string())
        })?;
    // The file and directory tools operate on the open project folder. The
    // chat UI is disabled when no folder is open, so a missing root here is a
    // programming error rather than a user-facing state.
    let root = state
        .lock()
        .map_err(|_| AppError::Poisoned)?
        .0
        .clone()
        .ok_or(AppError::NoRoot)?;
    tauri::async_runtime::spawn_blocking(move || {
        core::openrouter::chat_with_tools(
            &key,
            &model,
            &root,
            &messages,
            agent_prompt.as_deref(),
            &mut |chunk| {
                let _ = app.emit("chat:chunk", chunk);
            },
            &mut |name, result| {
                let _ = app.emit(
                    "chat:tool",
                    ToolCallEvent {
                        name: name.to_string(),
                        result: result.to_string(),
                    },
                );
            },
        )
    })
    .await
    .map_err(|e| AppError::Http(e.to_string()))?
}

/// Summarizes a conversation into a concise summary, to be used as a
/// replacement for the full history when the context window is nearly full.
/// Requires a stored API key. Runs the network call on a blocking thread.
#[tauri::command]
async fn compact_history(model: String, messages: Vec<ChatMessage>) -> Result<String, AppError> {
    let key = tauri::async_runtime::spawn_blocking(load_key)
        .await
        .map_err(|e| AppError::Keyring(e.to_string()))??
        .ok_or_else(|| {
            AppError::Http("no OpenRouter API key set — add one in the chat panel".to_string())
        })?;
    tauri::async_runtime::spawn_blocking(move || {
        core::openrouter::summarize_conversation(&key, &model, &messages)
    })
    .await
    .map_err(|e| AppError::Http(e.to_string()))?
}

/// Returns the names and descriptions of the tools the agent can call. Pure
/// in-memory (no I/O or network), so it runs inline rather than on a blocking
/// thread.
#[tauri::command]
fn list_tools() -> Vec<core::tools::ToolInfo> {
    core::tools::tool_info()
}

/// Returns the persisted user settings.
#[tauri::command]
async fn get_settings(app: tauri::AppHandle) -> Result<core::settings::Settings, AppError> {
    let file = settings_file(&app)?;
    let file_for_task = file.clone();
    tauri::async_runtime::spawn_blocking(move || {
        core::settings::load_settings::<core::settings::Settings>(&file_for_task)
    })
    .await
    .map_err(|e| AppError::Io(std::io::Error::other(e), file))?
}

/// Persists the full settings (model id and chat sessions) in a single write.
/// The frontend owns the complete settings state and sends it whole, so this
/// is a plain write — no read-modify-write, which avoids lost updates between
/// concurrent saves.
#[tauri::command]
async fn save_settings(
    app: tauri::AppHandle,
    settings: core::settings::Settings,
) -> Result<(), AppError> {
    let file = settings_file(&app)?;
    let file_for_task = file.clone();
    tauri::async_runtime::spawn_blocking(move || {
        core::settings::save_settings(&file_for_task, &settings)
    })
    .await
    .map_err(|e| AppError::Io(std::io::Error::other(e), file.clone()))??;
    Ok(())
}

/// Returns the chat sessions for a project, given its root path. A missing
/// file yields an empty list.
#[tauri::command]
async fn get_project_sessions(
    app: tauri::AppHandle,
    root: String,
) -> Result<Vec<core::settings::ChatSession>, AppError> {
    let root_path = PathBuf::from(&root);
    let file = project_sessions_file(&app, &root_path)?;
    let file_for_task = file.clone();
    let sessions = tauri::async_runtime::spawn_blocking(move || {
        core::settings::load_settings::<core::settings::ProjectSessions>(&file_for_task)
    })
    .await
    .map_err(|e| AppError::Io(std::io::Error::other(e), file.clone()))??
    .sessions;
    Ok(sessions)
}

/// Persists the chat sessions for a project, given its root path. The frontend
/// owns the complete session list and sends it whole, so this is a plain
/// write — no read-modify-write.
#[tauri::command]
async fn save_project_sessions(
    app: tauri::AppHandle,
    root: String,
    sessions: Vec<core::settings::ChatSession>,
) -> Result<(), AppError> {
    let root_path = PathBuf::from(&root);
    let file = project_sessions_file(&app, &root_path)?;
    let file_for_task = file.clone();
    let payload = core::settings::ProjectSessions { sessions };
    tauri::async_runtime::spawn_blocking(move || {
        core::settings::save_settings(&file_for_task, &payload)
    })
    .await
    .map_err(|e| AppError::Io(std::io::Error::other(e), file.clone()))??;
    Ok(())
}

/// Sets the root directory that all relative file paths resolve against.
/// Returns the canonicalized root path. Also records the root in the recent
/// list.
#[tauri::command]
async fn set_root(
    app: tauri::AppHandle,
    state: tauri::State<'_, RootState>,
    path: String,
) -> Result<PathBuf, AppError> {
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
    // Record in the recent list; a failure here is non-fatal, but logged so
    // a corrupt or unwritable recent.json is diagnosable.
    match recent_file(&app) {
        Ok(file) => {
            let recent = core::recent::load_recent(&file).unwrap_or_default();
            if let Err(e) = core::recent::save_recent(
                &file,
                &core::recent::add_recent(&recent, &canonical.to_string_lossy()),
            ) {
                log::warn!("failed to persist recent roots: {e}");
            }
        }
        Err(e) => log::warn!("failed to resolve recent roots file: {e}"),
    }
    Ok(canonical)
}

/// Returns the list of recently opened roots, most recent first. Paths are
/// rendered in a display-friendly form (the Windows `\\?\` prefix is stripped).
#[tauri::command]
async fn recent_roots(app: tauri::AppHandle) -> Result<Vec<core::recent::RecentRoot>, AppError> {
    let file = recent_file(&app)?;
    let roots = core::recent::load_recent(&file)?;
    Ok(roots
        .into_iter()
        .map(|r| core::recent::RecentRoot {
            path: core::paths::display_path(std::path::Path::new(&r.path)),
        })
        .collect())
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

/// Returns the current root directory, or `None` if no folder is open. The
/// path is rendered in a display-friendly form (the Windows `\\?\` prefix is
/// stripped).
#[tauri::command]
async fn get_root(state: tauri::State<'_, RootState>) -> Result<Option<PathBuf>, AppError> {
    let root = state.lock().map_err(|_| AppError::Poisoned)?.0.clone();
    Ok(root.map(|r| PathBuf::from(core::paths::display_path(&r))))
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
            get_root,
            recent_roots,
            set_key,
            get_key,
            list_models,
            list_tools,
            chat,
            compact_history,
            get_settings,
            save_settings,
            get_project_sessions,
            save_project_sessions
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
