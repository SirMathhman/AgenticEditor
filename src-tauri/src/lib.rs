// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Returns the names of the entries in the current working directory.
#[tauri::command]
fn list_files() -> Result<Vec<String>, String> {
    let dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    Ok(names)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_mcp_bridge::init());
    }

    builder
        .invoke_handler(tauri::generate_handler![greet, list_files])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
