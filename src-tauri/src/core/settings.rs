//! Pure persistence for user settings (the selected chat model and chat
//! sessions). Stored as JSON in `app_config_dir/settings.json`.

use std::fs;
use std::path::Path;

use super::errors::AppError;

/// A single chat message within a session.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct ChatMessage {
    /// The author: `user` or `agent`.
    pub role: String,
    /// The message text.
    pub text: String,
    /// The model's chain-of-thought, present only on agent messages from
    /// reasoning models. Defaults to `None` so older session files still load.
    #[serde(default)]
    pub thinking: Option<String>,
}

/// A chat session: a titled conversation with the agent.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct ChatSession {
    /// A unique id for the session (e.g. a timestamp or uuid).
    pub id: String,
    /// A short display title (derived from the first user message).
    pub title: String,
    /// The conversation, in order.
    pub messages: Vec<ChatMessage>,
    /// The model id last used in this chat, so reopening it restores the same
    /// model into the picker. `None` for chats persisted before this field.
    #[serde(default)]
    pub model_id: Option<String>,
    /// The provider last used in this chat. Defaults to OpenRouter for
    /// sessions persisted before this field existed.
    #[serde(default)]
    pub provider: Provider,
}

/// The model provider the chat panel talks to. Both expose an
/// OpenAI-compatible API, so the same request/streaming code serves both —
/// only the base URL and (for OpenRouter) the auth differ.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone, Copy, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    /// The OpenRouter hosted API (requires a stored API key).
    #[default]
    OpenRouter,
    /// A local llama.cpp server (no auth; configured by base URL).
    LlamaCpp,
}

/// A user-defined custom agent: a named system prompt that layers on top of
/// the base agent prompt to give the model a specific role or behavior.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct CustomAgent {
    /// A unique id for the agent (e.g. a uuid).
    pub id: String,
    /// A short display name (e.g. "Code Reviewer").
    pub name: String,
    /// The system prompt text. Layered on top of the base agent prompt so the
    /// model always retains tool and working-directory context.
    pub prompt: String,
}

/// User settings persisted across app restarts.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone, Default)]
pub struct Settings {
    /// The id of the selected chat model (e.g. `openai/gpt-4o`), or `None`
    /// when the user has not chosen one yet.
    pub model_id: Option<String>,
    /// The user's custom agents. Empty by default.
    #[serde(default)]
    pub agents: Vec<CustomAgent>,
    /// The id of the currently active custom agent, or `None` for the default
    /// (no custom prompt).
    #[serde(default)]
    pub active_agent_id: Option<String>,
    /// The model provider. Defaults to OpenRouter so older settings files
    /// (without this field) keep working.
    #[serde(default)]
    pub provider: Provider,
    /// The base URL of the llama.cpp server, used only when the provider is
    /// `LlamaCpp`. Empty by default; the frontend supplies a sensible default.
    #[serde(default)]
    pub base_url: String,
}

/// The chat sessions belonging to a single project (root folder). Stored in a
/// per-project file under the app config dir, keyed by a hash of the root
/// path, so sessions never land inside the project directory itself.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone, Default)]
pub struct ProjectSessions {
    /// The chat sessions, most recent first.
    pub sessions: Vec<ChatSession>,
}

/// Derives a stable, filesystem-safe file name for a project's session file
/// from its root path. The path is normalized (see [`normalize_root`]) and
/// then hashed with a 64-bit FNV-1a, which keeps the name short, avoids any
/// characters that are illegal in file names, and is deterministic across
/// runs (unlike `DefaultHasher`, which is randomized per process). The same
/// project — even when its path is written with different separators, case,
/// or a trailing slash — always maps to the same file.
pub fn project_sessions_file_name(root: &Path) -> String {
    let normalized = normalize_root(root);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:016x}", hash)
}

/// Normalizes a root path into a canonical string form for hashing: forward
/// slashes, lowercase (so case-insensitive filesystems like Windows treat
/// `C:\X` and `c:/x` as the same project), no duplicate separators, and no
/// trailing separator (a lone leading `/` for absolute paths is kept).
fn normalize_root(root: &Path) -> String {
    let s: String = root.to_string_lossy().replace('\\', "/").to_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut prev_slash = false;
    for c in s.chars() {
        if c == '/' {
            if !prev_slash {
                out.push(c);
            }
            prev_slash = true;
        } else {
            out.push(c);
            prev_slash = false;
        }
    }
    if out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

/// Loads a settings value from `file`. A missing file yields the type's
/// defaults; a corrupt file is an error.
pub fn load_settings<T: serde::de::DeserializeOwned + Default>(file: &Path) -> Result<T, AppError> {
    match fs::read_to_string(file) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|e| AppError::Io(std::io::Error::other(e), file.to_path_buf())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(AppError::Io(e, file.to_path_buf())),
    }
}

/// Saves a settings value to `file`. Parent directories are created if needed.
pub fn save_settings<T: serde::Serialize>(file: &Path, settings: &T) -> Result<(), AppError> {
    let contents = serde_json::to_string_pretty(settings)
        .map_err(|e| AppError::Io(std::io::Error::other(e), file.to_path_buf()))?;
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::Io(e, parent.to_path_buf()))?;
    }
    fs::write(file, contents).map_err(|e| AppError::Io(e, file.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_settings_missing_file_is_default() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let settings: Settings = load_settings(&file).unwrap();
        assert_eq!(settings, Settings::default());
        assert_eq!(settings.model_id, None);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let settings = Settings {
            model_id: Some("openai/gpt-4o".to_string()),
            agents: vec![CustomAgent {
                id: "a1".to_string(),
                name: "Reviewer".to_string(),
                prompt: "Be strict.".to_string(),
            }],
            active_agent_id: Some("a1".to_string()),
            // New fields (provider, base_url) take their defaults; the test
            // still round-trips and asserts equality on every field.
            ..Default::default()
        };
        save_settings(&file, &settings).unwrap();
        assert_eq!(load_settings::<Settings>(&file).unwrap(), settings);
    }

    #[test]
    fn project_sessions_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sessions.json");
        let sessions = ProjectSessions {
            sessions: vec![ChatSession {
                id: "s1".to_string(),
                title: "Hello".to_string(),
                messages: vec![
                    ChatMessage {
                        role: "user".to_string(),
                        text: "Hi".to_string(),
                        thinking: None,
                    },
                    ChatMessage {
                        role: "agent".to_string(),
                        text: "Hello!".to_string(),
                        thinking: Some("Let me think…".to_string()),
                    },
                ],
                model_id: Some("llama3".to_string()),
                provider: Provider::LlamaCpp,
            }],
        };
        save_settings(&file, &sessions).unwrap();
        assert_eq!(load_settings::<ProjectSessions>(&file).unwrap(), sessions);
    }

    #[test]
    fn load_session_without_thinking_field_is_none() {
        // A session file written before `thinking` existed has no such key on
        // its messages; it must still load, with `thinking` defaulting to None.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sessions.json");
        fs::write(
            &file,
            r#"{"sessions":[{"id":"s1","title":"Hi","messages":[{"role":"agent","text":"Hello"}]}]}"#,
        )
        .unwrap();
        let loaded: ProjectSessions = load_settings(&file).unwrap();
        assert_eq!(loaded.sessions[0].messages[0].thinking, None);
    }

    #[test]
    fn project_sessions_file_name_is_stable_and_safe() {
        let a = project_sessions_file_name(Path::new("/home/user/project"));
        let b = project_sessions_file_name(Path::new("/home/user/project"));
        // Deterministic: the same root always yields the same name.
        assert_eq!(a, b);
        // Different roots yield different names.
        assert_ne!(a, project_sessions_file_name(Path::new("/home/user/other")));
        // Filesystem-safe: only lowercase hex characters.
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn project_sessions_file_name_normalizes_equivalent_paths() {
        let base = project_sessions_file_name(Path::new("/home/user/project"));
        // A trailing slash and case differences refer to the same project and
        // must map to the same file.
        assert_eq!(
            base,
            project_sessions_file_name(Path::new("/home/user/project/"))
        );
        assert_eq!(
            base,
            project_sessions_file_name(Path::new("/HOME/user/project"))
        );
        // Backslashes normalize to forward slashes, so a Windows-style path
        // matches its POSIX equivalent.
        assert_eq!(
            project_sessions_file_name(Path::new("C:\\Users\\user\\project")),
            project_sessions_file_name(Path::new("c:/users/user/project"))
        );
        // A genuinely different project must not collide.
        assert_ne!(
            base,
            project_sessions_file_name(Path::new("/home/user/project2"))
        );
    }

    #[test]
    fn load_settings_corrupt_file_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        fs::write(&file, "not json").unwrap();
        assert!(load_settings::<Settings>(&file).is_err());
    }
}
