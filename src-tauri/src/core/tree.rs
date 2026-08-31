use std::fs;
use std::path::Path;

/// A node in the directory tree.
#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub name: String,
    /// Path relative to the tree root, using `/` separators.
    pub path: String,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TreeNode>>,
}

/// Directories that are never useful to list (large, generated, or VCS internals).
pub const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", "dist"];
/// Maximum recursion depth for the tree.
pub const MAX_DEPTH: usize = 4;

/// Builds a directory tree rooted at `dir`, recursing up to `MAX_DEPTH`.
pub fn read_dir_tree(dir: &Path, depth: usize) -> Result<Vec<TreeNode>, String> {
    read_dir_tree_at(dir, depth, "")
}

fn read_dir_tree_at(dir: &Path, depth: usize, rel: &str) -> Result<Vec<TreeNode>, String> {
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut nodes = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
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
                children,
            });
        } else {
            nodes.push(TreeNode {
                name,
                path: child_rel,
                is_dir: false,
                children: None,
            });
        }
    }
    nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(nodes)
}

/// Reads a file's contents, given its path relative to `root`.
/// Rejects paths that would escape `root` (e.g. `..` components).
pub fn read_file_at(root: &Path, rel_path: &str) -> Result<String, String> {
    if rel_path.is_empty() {
        return Err("empty path".into());
    }
    let path = root.join(rel_path);
    // Ensure the resolved path is still inside the root.
    let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
    let canonical_path = path.canonicalize().map_err(|e| e.to_string())?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err("path escapes the root directory".into());
    }
    if canonical_path.is_dir() {
        return Err("path is a directory".into());
    }
    fs::read_to_string(&canonical_path).map_err(|e| e.to_string())
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
}
