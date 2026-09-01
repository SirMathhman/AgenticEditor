//! Pure persistence for the agent's project-scoped memory: a small set of
//! markdown/text files the agent can read and edit across chat sessions.
//!
//! Memory lives in a per-project directory (under the app config dir, keyed by
//! the same project hash as chat sessions — see `settings.rs`), never inside
//! the project folder itself. All paths are relative to the memory root and
//! are protected against traversal the same way `tree.rs` protects project
//! paths. This module is tauri-free and unit-tested.

use super::errors::AppError;
use std::fs;
use std::path::{Path, PathBuf};

/// Ensures the memory root directory exists, creating it (and any missing
/// parents) on first use. Cheap and idempotent, so every operation calls it
/// before resolving paths — this also lets `canonicalize` succeed on a fresh
/// root.
fn ensure_root(root: &Path) -> Result<(), AppError> {
    fs::create_dir_all(root).map_err(|e| AppError::Io(e, root.to_path_buf()))
}

/// Resolves `rel_path` against the memory root for an operation on an existing
/// file or directory. An empty `rel_path` resolves to the root itself. Rejects
/// paths that would escape the root.
fn resolve_existing(root: &Path, rel_path: &str) -> Result<PathBuf, AppError> {
    ensure_root(root)?;
    let path = if rel_path.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel_path)
    };
    let canonical_root = root
        .canonicalize()
        .map_err(|e| AppError::Io(e, root.to_path_buf()))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|e| AppError::Io(e, path.clone()))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(AppError::PathEscapesRoot(rel_path.to_string()));
    }
    Ok(canonical_path)
}

/// Resolves `rel_path` against the memory root for an operation whose target
/// (and possibly its parent) may not exist yet (create, rename destination).
/// The target cannot be canonicalized up front, so escaping paths are rejected
/// lexically; the boundary is enforced by [`reject_escaping_rel_path`].
fn resolve_for_write(root: &Path, rel_path: &str) -> Result<PathBuf, AppError> {
    ensure_root(root)?;
    if rel_path.is_empty() {
        return Err(AppError::EmptyPath);
    }
    reject_escaping_rel_path(rel_path)?;
    Ok(root.join(rel_path))
}

/// Lexically rejects a relative path that would escape the root: an absolute
/// path (or one with a leading separator, which `is_absolute` misses on
/// Windows), or a `..` component that climbs above the root.
fn reject_escaping_rel_path(rel_path: &str) -> Result<(), AppError> {
    if Path::new(rel_path).is_absolute() || rel_path.starts_with(['/', '\\']) {
        return Err(AppError::PathEscapesRoot(rel_path.to_string()));
    }
    let mut depth = 0usize;
    for component in rel_path.split(['/', '\\']) {
        match component {
            "" | "." => {}
            ".." => {
                if depth == 0 {
                    return Err(AppError::PathEscapesRoot(rel_path.to_string()));
                }
                depth -= 1;
            }
            _ => depth += 1,
        }
    }
    Ok(())
}

/// Reads a memory file's contents, or lists a memory directory. An empty path
/// lists the memory root. Directories are listed with a trailing `/`.
pub fn view(root: &Path, rel_path: &str) -> Result<String, AppError> {
    let path = resolve_existing(root, rel_path)?;
    if path.is_dir() {
        return list_dir(&path);
    }
    fs::read_to_string(&path).map_err(|e| AppError::Io(e, path))
}

/// Lists the immediate contents of a directory, marking subdirectories with a
/// trailing `/`. Directories sort before files.
fn list_dir(path: &Path) -> Result<String, AppError> {
    let mut entries: Vec<(String, bool)> = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| AppError::Io(e, path.to_path_buf()))? {
        let entry = entry.map_err(|e| AppError::Io(e, path.to_path_buf()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        entries.push((name, is_dir));
    }
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if entries.is_empty() {
        return Ok("(empty directory)".to_string());
    }
    Ok(entries
        .into_iter()
        .map(
            |(name, is_dir)| {
                if is_dir {
                    format!("{name}/")
                } else {
                    name
                }
            },
        )
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Creates a new memory file with the given contents. Fails if the file
/// already exists. Creates any missing parent directories.
pub fn create(root: &Path, rel_path: &str, contents: &str) -> Result<String, AppError> {
    let path = resolve_for_write(root, rel_path)?;
    if path.exists() {
        return Err(AppError::Tool(format!("already exists: {rel_path}")));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::Io(e, parent.to_path_buf()))?;
    }
    fs::write(&path, contents).map_err(|e| AppError::Io(e, path.clone()))?;
    Ok(format!("created {rel_path} ({} bytes)", contents.len()))
}

/// Replaces exactly one occurrence of `old` with `new` in a memory file.
/// Fails if `old` is not found or appears more than once (the caller must
/// supply enough context to make the match unique).
pub fn str_replace(root: &Path, rel_path: &str, old: &str, new: &str) -> Result<String, AppError> {
    let path = resolve_existing(root, rel_path)?;
    let contents = fs::read_to_string(&path).map_err(|e| AppError::Io(e, path.clone()))?;
    let count = contents.matches(old).count();
    match count {
        0 => Err(AppError::Tool(format!("old text not found in {rel_path}"))),
        1 => {
            let updated = contents.replacen(old, new, 1);
            fs::write(&path, updated).map_err(|e| AppError::Io(e, path))?;
            Ok(format!("replaced text in {rel_path}"))
        }
        n => Err(AppError::Tool(format!(
            "old text appears {n} times in {rel_path}; it must be unique"
        ))),
    }
}

/// Inserts `text` into a memory file at the given 0-based line number. Line 0
/// inserts before the first line; a line number at or beyond the end appends.
/// The inserted text becomes its own line(s).
pub fn insert(root: &Path, rel_path: &str, line: usize, text: &str) -> Result<String, AppError> {
    let path = resolve_existing(root, rel_path)?;
    let contents = fs::read_to_string(&path).map_err(|e| AppError::Io(e, path.clone()))?;
    let mut lines: Vec<&str> = contents.split('\n').collect();
    // Clamp the insertion point to the end of the file.
    let at = line.min(lines.len());
    lines.insert(at, text);
    let updated = lines.join("\n");
    fs::write(&path, updated).map_err(|e| AppError::Io(e, path))?;
    Ok(format!("inserted at line {line} in {rel_path}"))
}

/// Deletes a memory file or directory (recursively).
pub fn delete(root: &Path, rel_path: &str) -> Result<String, AppError> {
    if rel_path.is_empty() {
        return Err(AppError::Tool(
            "cannot delete the memory root itself".to_string(),
        ));
    }
    let path = resolve_existing(root, rel_path)?;
    if path.is_dir() {
        fs::remove_dir_all(&path).map_err(|e| AppError::Io(e, path.clone()))?;
        Ok(format!("deleted directory: {rel_path}"))
    } else {
        fs::remove_file(&path).map_err(|e| AppError::Io(e, path.clone()))?;
        Ok(format!("deleted file: {rel_path}"))
    }
}

/// Renames or moves a memory file or directory. Both paths are relative to the
/// memory root; the source must exist and the destination must not.
pub fn rename(root: &Path, old_path: &str, new_path: &str) -> Result<String, AppError> {
    if old_path.is_empty() || new_path.is_empty() {
        return Err(AppError::EmptyPath);
    }
    let from = resolve_existing(root, old_path)?;
    let to = resolve_for_write(root, new_path)?;
    if to.exists() {
        return Err(AppError::Tool(format!("already exists: {new_path}")));
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::Io(e, parent.to_path_buf()))?;
    }
    fs::rename(&from, &to).map_err(|e| AppError::Io(e, to.clone()))?;
    Ok(format!("renamed {old_path} to {new_path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn create_then_view_round_trips() {
        let dir = temp_root();
        let root = dir.path();
        create(root, "notes.md", "hello\nworld").unwrap();
        assert_eq!(view(root, "notes.md").unwrap(), "hello\nworld");
    }

    #[test]
    fn create_fails_when_file_exists() {
        let dir = temp_root();
        let root = dir.path();
        create(root, "a.md", "x").unwrap();
        assert!(create(root, "a.md", "y").is_err());
    }

    #[test]
    fn view_empty_path_lists_root() {
        let dir = temp_root();
        let root = dir.path();
        create(root, "a.md", "x").unwrap();
        create(root, "sub/b.md", "y").unwrap();
        let listing = view(root, "").unwrap();
        assert!(listing.contains("sub/"), "listing: {listing}");
        assert!(listing.contains("a.md"), "listing: {listing}");
    }

    #[test]
    fn str_replace_requires_unique_match() {
        let dir = temp_root();
        let root = dir.path();
        create(root, "a.md", "one two one").unwrap();
        // "one" appears twice -> error.
        assert!(str_replace(root, "a.md", "one", "ONE").is_err());
        // A unique substring succeeds.
        assert!(str_replace(root, "a.md", "two", "TWO").is_ok());
        assert_eq!(view(root, "a.md").unwrap(), "one TWO one");
    }

    #[test]
    fn str_replace_missing_text_is_an_error() {
        let dir = temp_root();
        let root = dir.path();
        create(root, "a.md", "abc").unwrap();
        assert!(str_replace(root, "a.md", "zzz", "q").is_err());
    }

    #[test]
    fn insert_at_line_zero_prepends() {
        let dir = temp_root();
        let root = dir.path();
        create(root, "a.md", "b\nc").unwrap();
        insert(root, "a.md", 0, "a").unwrap();
        assert_eq!(view(root, "a.md").unwrap(), "a\nb\nc");
    }

    #[test]
    fn insert_beyond_end_appends() {
        let dir = temp_root();
        let root = dir.path();
        create(root, "a.md", "a\nb").unwrap();
        insert(root, "a.md", 99, "c").unwrap();
        assert_eq!(view(root, "a.md").unwrap(), "a\nb\nc");
    }

    #[test]
    fn delete_removes_file_and_directory() {
        let dir = temp_root();
        let root = dir.path();
        create(root, "a.md", "x").unwrap();
        create(root, "sub/b.md", "y").unwrap();
        assert!(delete(root, "a.md").is_ok());
        assert!(view(root, "a.md").is_err());
        assert!(delete(root, "sub").is_ok());
        assert!(view(root, "sub/b.md").is_err());
    }

    #[test]
    fn delete_root_is_rejected() {
        let dir = temp_root();
        let root = dir.path();
        assert!(delete(root, "").is_err());
    }

    #[test]
    fn rename_moves_a_file() {
        let dir = temp_root();
        let root = dir.path();
        create(root, "a.md", "x").unwrap();
        assert!(rename(root, "a.md", "b.md").is_ok());
        assert!(view(root, "a.md").is_err());
        assert_eq!(view(root, "b.md").unwrap(), "x");
    }

    #[test]
    fn rename_fails_when_destination_exists() {
        let dir = temp_root();
        let root = dir.path();
        create(root, "a.md", "x").unwrap();
        create(root, "b.md", "y").unwrap();
        assert!(rename(root, "a.md", "b.md").is_err());
    }

    #[test]
    fn path_traversal_is_rejected() {
        let dir = temp_root();
        let root = dir.path();
        // `..` climbing above the root is rejected for reads.
        assert!(view(root, "../secret").is_err());
        // Absolute paths are rejected for writes.
        assert!(create(root, "/etc/passwd", "x").is_err());
        // A `..` that would escape is rejected for writes.
        assert!(create(root, "../escape.md", "x").is_err());
    }
}
