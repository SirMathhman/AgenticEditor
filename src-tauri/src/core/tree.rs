use super::errors::AppError;
use std::fs;
use std::path::Path;

/// A node in the directory tree.
#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub name: String,
    /// Path relative to the tree root, using `/` separators.
    pub path: String,
    pub is_dir: bool,
    /// Whether this file is an image the UI can render. The backend is the
    /// single source of truth for image detection (via `image_mime_type`), so
    /// the frontend never sniffs extensions itself.
    #[serde(default)]
    pub is_image: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TreeNode>>,
}

/// Directories that are never useful to list (large, generated, or VCS internals).
pub const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", "dist"];
/// Maximum recursion depth for the tree.
pub const MAX_DEPTH: usize = 4;

/// Builds a directory tree rooted at `dir`, recursing up to `MAX_DEPTH`.
pub fn read_dir_tree(dir: &Path, depth: usize) -> Result<Vec<TreeNode>, AppError> {
    read_dir_tree_at(dir, depth, "")
}

fn read_dir_tree_at(dir: &Path, depth: usize, rel: &str) -> Result<Vec<TreeNode>, AppError> {
    let entries = fs::read_dir(dir).map_err(|e| AppError::Io(e, dir.to_path_buf()))?;
    let mut nodes = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| AppError::Io(e, dir.to_path_buf()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let file_type = entry
            .file_type()
            .map_err(|e| AppError::Io(e, dir.to_path_buf()))?;
        let child_rel = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if file_type.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            let children = if depth < MAX_DEPTH {
                Some(read_dir_tree_at(&entry.path(), depth + 1, &child_rel)?)
            } else {
                None
            };
            nodes.push(TreeNode {
                name,
                path: child_rel,
                is_dir: true,
                is_image: false,
                children,
            });
        } else {
            let is_image = image_mime_type(&child_rel).is_some();
            nodes.push(TreeNode {
                name,
                path: child_rel,
                is_dir: false,
                is_image,
                children: None,
            });
        }
    }
    nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(nodes)
}

/// Reads a file's contents, given its path relative to `root`.
/// Rejects paths that would escape `root` (e.g. `..` components).
pub fn read_file_at(root: &Path, rel_path: &str) -> Result<String, AppError> {
    let path = resolve_in_root(root, rel_path)?;
    fs::read_to_string(&path).map_err(|e| AppError::Io(e, path))
}

/// Resolves `rel_path` against `root`, rejecting paths that escape `root`.
fn resolve_in_root(root: &Path, rel_path: &str) -> Result<std::path::PathBuf, AppError> {
    if rel_path.is_empty() {
        return Err(AppError::EmptyPath);
    }
    let path = root.join(rel_path);
    let canonical_root = root
        .canonicalize()
        .map_err(|e| AppError::Io(e, root.to_path_buf()))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|e| AppError::Io(e, path.clone()))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(AppError::PathEscapesRoot(rel_path.to_string()));
    }
    if canonical_path.is_dir() {
        return Err(AppError::IsDirectory(canonical_path));
    }
    Ok(canonical_path)
}

/// Resolves `rel_path` against `root` for writing. Unlike `resolve_in_root`,
/// the target file need not exist yet, so only the parent directory is
/// canonicalized to enforce the root boundary.
fn resolve_in_root_for_write(root: &Path, rel_path: &str) -> Result<std::path::PathBuf, AppError> {
    if rel_path.is_empty() {
        return Err(AppError::EmptyPath);
    }
    let path = root.join(rel_path);
    let canonical_root = root
        .canonicalize()
        .map_err(|e| AppError::Io(e, root.to_path_buf()))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or(AppError::InvalidPath(rel_path.to_string()))?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|e| AppError::Io(e, parent.to_path_buf()))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(AppError::PathEscapesRoot(rel_path.to_string()));
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::InvalidPath(rel_path.to_string()))?;
    let target = canonical_parent.join(file_name);
    if target.is_dir() {
        return Err(AppError::IsDirectory(target));
    }
    Ok(target)
}

/// Writes `contents` to a file, given its path relative to `root`.
/// Creates the file if it does not exist. Rejects paths that would escape `root`.
pub fn write_file_at(root: &Path, rel_path: &str, contents: &str) -> Result<(), AppError> {
    let path = resolve_in_root_for_write(root, rel_path)?;
    fs::write(&path, contents).map_err(|e| AppError::Io(e, path))
}

/// MIME types for image extensions we can render in the UI.
pub fn image_mime_type(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "ico" => Some("image/x-icon"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

/// A file's raw contents, base64-encoded, with its MIME type.
#[derive(serde::Serialize, Debug)]
pub struct FileData {
    pub data: String,
    pub mime_type: String,
}

/// Reads a file's raw bytes (for binary files like images), given its path
/// relative to `root`. Rejects paths that would escape `root`.
pub fn read_file_data(root: &Path, rel_path: &str) -> Result<FileData, AppError> {
    let path = resolve_in_root(root, rel_path)?;
    let mime_type = image_mime_type(&path.to_string_lossy())
        .ok_or_else(|| AppError::NotAnImage(path.clone()))?;
    let bytes = fs::read(&path).map_err(|e| AppError::Io(e, path))?;
    Ok(FileData {
        data: base64_encode(&bytes),
        mime_type: mime_type.to_string(),
    })
}

/// Minimal base64 encoder (avoids a dependency for a small, stable alphabet).
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn node(name: &str, is_dir: bool) -> TreeNode {
        TreeNode {
            name: name.into(),
            path: name.into(),
            is_dir,
            is_image: false,
            children: None,
        }
    }

    #[test]
    fn skips_excluded_directories() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::create_dir(dir.path().join("dist")).unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("main.rs"), "").unwrap();

        let tree = read_dir_tree(dir.path(), 0).unwrap();
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["src", "main.rs"]);
    }

    #[test]
    fn sorts_directories_before_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("zeta.txt"), "").unwrap();
        fs::write(dir.path().join("alpha.txt"), "").unwrap();
        fs::create_dir(dir.path().join("mid")).unwrap();

        let tree = read_dir_tree(dir.path(), 0).unwrap();
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["mid", "alpha.txt", "zeta.txt"]);
    }

    #[test]
    fn marks_image_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("photo.png"), "").unwrap();
        fs::write(dir.path().join("notes.txt"), "").unwrap();

        let tree = read_dir_tree(dir.path(), 0).unwrap();
        let by_name: std::collections::HashMap<&str, bool> =
            tree.iter().map(|n| (n.name.as_str(), n.is_image)).collect();
        assert_eq!(by_name.get("photo.png"), Some(&true));
        assert_eq!(by_name.get("notes.txt"), Some(&false));
    }

    #[test]
    fn caps_recursion_at_max_depth() {
        let dir = tempfile::tempdir().unwrap();
        // Build a chain deeper than MAX_DEPTH: a/b/c/d/e/f
        let mut path = dir.path().to_path_buf();
        for name in ["a", "b", "c", "d", "e", "f"] {
            path = path.join(name);
            fs::create_dir(&path).unwrap();
        }

        let tree = read_dir_tree(dir.path(), 0).unwrap();
        // Walk down the chain, counting how many levels have children.
        let mut current = &tree[0];
        let mut depth_with_children = 0;
        while let Some(children) = &current.children {
            depth_with_children += 1;
            current = &children[0];
        }
        assert_eq!(depth_with_children, MAX_DEPTH);
    }

    #[test]
    fn returns_error_for_missing_dir() {
        let result = read_dir_tree(Path::new("/nonexistent/path/xyz"), 0);
        assert!(result.is_err());
    }

    #[test]
    fn read_file_at_returns_contents() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hello.txt"), "hello world").unwrap();

        let contents = read_file_at(dir.path(), "hello.txt").unwrap();
        assert_eq!(contents, "hello world");
    }

    #[test]
    fn read_file_at_reads_nested_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("inner.txt"), "nested").unwrap();

        let contents = read_file_at(dir.path(), "sub/inner.txt").unwrap();
        assert_eq!(contents, "nested");
    }

    #[test]
    fn read_file_at_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("secret.txt"), "top secret").unwrap();

        // Attempt to escape the root via `..`.
        let result = read_file_at(dir.path(), "../secret.txt");
        assert!(result.is_err());
    }

    #[test]
    fn read_file_at_rejects_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("somedir")).unwrap();

        let result = read_file_at(dir.path(), "somedir");
        assert!(result.is_err());
    }

    #[test]
    fn write_file_at_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();

        write_file_at(dir.path(), "new.txt", "hello").unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("new.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn write_file_at_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "old").unwrap();

        write_file_at(dir.path(), "a.txt", "new").unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "new");
    }

    #[test]
    fn write_file_at_writes_nested_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();

        write_file_at(dir.path(), "sub/inner.txt", "nested").unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("sub").join("inner.txt")).unwrap(),
            "nested"
        );
    }

    #[test]
    fn write_file_at_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();

        let result = write_file_at(dir.path(), "../escape.txt", "x");
        assert!(result.is_err());
    }

    #[test]
    fn write_file_at_rejects_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("somedir")).unwrap();

        let result = write_file_at(dir.path(), "somedir", "x");
        assert!(result.is_err());
    }

    #[test]
    fn tree_nodes_carry_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src").join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("root.txt"), "r").unwrap();

        let tree = read_dir_tree(dir.path(), 0).unwrap();
        // Directories sort first: src, then root.txt.
        assert_eq!(tree[0].path, "src");
        assert_eq!(tree[1].path, "root.txt");
        let src_children = tree[0].children.as_ref().unwrap();
        assert_eq!(src_children[0].path, "src/main.rs");
    }

    #[test]
    fn base64_encode_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn image_mime_type_maps_extensions() {
        assert_eq!(image_mime_type("a/b.png"), Some("image/png"));
        assert_eq!(image_mime_type("a/b.JPG"), Some("image/jpeg"));
        assert_eq!(image_mime_type("a/b.svg"), Some("image/svg+xml"));
        assert_eq!(image_mime_type("a/b.txt"), None);
        assert_eq!(image_mime_type("noext"), None);
    }

    #[test]
    fn read_file_data_returns_base64_and_mime() {
        let dir = tempfile::tempdir().unwrap();
        let bytes: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        fs::write(dir.path().join("img.png"), bytes).unwrap();

        let data = read_file_data(dir.path(), "img.png").unwrap();
        assert_eq!(data.mime_type, "image/png");
        assert_eq!(data.data, base64_encode(bytes));
    }

    #[test]
    fn read_file_data_rejects_non_images() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("notes.txt"), "hi").unwrap();

        let result = read_file_data(dir.path(), "notes.txt");
        assert!(result.is_err());
    }

    #[test]
    fn read_file_data_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("secret.png"), &[0, 1, 2]).unwrap();

        let result = read_file_data(dir.path(), "../secret.png");
        assert!(result.is_err());
    }
}
