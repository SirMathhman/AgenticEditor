use std::path::PathBuf;

/// Structured errors for the tauri-free core.
///
/// Tauri serializes the `Display` impl to the frontend, so commands can
/// return `Result<T, AppError>` directly without any extra mapping.
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    /// An I/O operation failed.
    #[error("I/O error at {1}: {0}")]
    Io(#[source] std::io::Error, PathBuf),

    /// The resolved path escapes the root directory.
    #[error("path escapes the root directory: {0}")]
    PathEscapesRoot(String),

    /// The relative path was empty.
    #[error("empty path")]
    EmptyPath,

    /// The relative path is structurally invalid.
    #[error("invalid path: {0}")]
    InvalidPath(String),

    /// The resolved path is a directory, not a file.
    #[error("path is a directory: {0}")]
    IsDirectory(PathBuf),

    /// The path is not a directory (used when setting the root).
    #[error("not a directory: {0}")]
    NotADirectory(PathBuf),

    /// The file is not a supported image type.
    #[error("not a supported image type: {0}")]
    NotAnImage(PathBuf),

    /// The root state was poisoned (a panic occurred while holding the lock).
    #[error("root state is poisoned")]
    Poisoned,
}

/// Tauri command errors must be serializable. We serialize to the `Display`
/// string so the frontend receives a human-readable message (and can later be
/// extended to a structured payload without breaking the wire format).
impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
