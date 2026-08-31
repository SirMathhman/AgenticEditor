use std::fs;
use std::path::Path;

use super::errors::AppError;

/// Maximum number of recent roots to remember.
pub const MAX_RECENT: usize = 10;

/// A recently opened root folder.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct RecentRoot {
    pub path: String,
}

/// Loads the list of recent roots from `file`. A missing file yields an empty
/// list; a corrupt file is an error.
pub fn load_recent(file: &Path) -> Result<Vec<RecentRoot>, AppError> {
    match fs::read_to_string(file) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|e| AppError::Io(std::io::Error::other(e), file.to_path_buf())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(AppError::Io(e, file.to_path_buf())),
    }
}

/// Saves the list of recent roots to `file`, keeping at most `MAX_RECENT`
/// entries. Parent directories are created if needed.
pub fn save_recent(file: &Path, roots: &[RecentRoot]) -> Result<(), AppError> {
    let trimmed: Vec<RecentRoot> = roots.iter().take(MAX_RECENT).cloned().collect();
    let contents = serde_json::to_string_pretty(&trimmed)
        .map_err(|e| AppError::Io(std::io::Error::other(e), file.to_path_buf()))?;
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::Io(e, parent.to_path_buf()))?;
    }
    fs::write(file, contents).map_err(|e| AppError::Io(e, file.to_path_buf()))
}

/// Returns a new recent list with `path` moved to the front, de-duplicated,
/// and capped at `MAX_RECENT` entries.
pub fn add_recent(roots: &[RecentRoot], path: &str) -> Vec<RecentRoot> {
    let mut out = Vec::with_capacity(MAX_RECENT);
    out.push(RecentRoot {
        path: path.to_string(),
    });
    for root in roots {
        if root.path != path && !out.iter().any(|r| r.path == root.path) {
            out.push(root.clone());
        }
        if out.len() == MAX_RECENT {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(paths: &[&str]) -> Vec<RecentRoot> {
        paths
            .iter()
            .map(|p| RecentRoot { path: (*p).into() })
            .collect()
    }

    #[test]
    fn add_recent_puts_new_path_first() {
        let list = add_recent(&roots(&["/a", "/b"]), "/c");
        assert_eq!(
            list.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
            vec!["/c", "/a", "/b"]
        );
    }

    #[test]
    fn add_recent_moves_existing_path_to_front() {
        let list = add_recent(&roots(&["/a", "/b", "/c"]), "/b");
        assert_eq!(
            list.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
            vec!["/b", "/a", "/c"]
        );
    }

    #[test]
    fn add_recent_caps_at_max() {
        let many: Vec<RecentRoot> = (0..MAX_RECENT)
            .map(|i| RecentRoot {
                path: format!("/p{i}"),
            })
            .collect();
        let list = add_recent(&many, "/new");
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0].path, "/new");
        assert!(!list.iter().any(|r| r.path == "/p9"));
    }

    #[test]
    fn load_recent_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let list = load_recent(&dir.path().join("recent.json")).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("recent.json");
        save_recent(&file, &roots(&["/a", "/b"])).unwrap();
        let list = load_recent(&file).unwrap();
        assert_eq!(list, roots(&["/a", "/b"]));
    }

    #[test]
    fn save_and_load_roundtrip_paths() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("recent.json");
        save_recent(&file, &roots(&["/a", "/b"])).unwrap();
        let list = load_recent(&file).unwrap();
        assert_eq!(
            list.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
            vec!["/a", "/b"]
        );
    }

    #[test]
    fn save_recent_trims_to_max() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("recent.json");
        let many: Vec<RecentRoot> = (0..MAX_RECENT + 5)
            .map(|i| RecentRoot {
                path: format!("/p{i}"),
            })
            .collect();
        save_recent(&file, &many).unwrap();
        let list = load_recent(&file).unwrap();
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0].path, "/p0");
    }

    #[test]
    fn load_recent_corrupt_file_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("recent.json");
        fs::write(&file, "not json").unwrap();
        assert!(load_recent(&file).is_err());
    }
}
