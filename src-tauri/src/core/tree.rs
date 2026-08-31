use std::fs;
use std::path::Path;

/// A node in the directory tree.
#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub name: String,
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
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut nodes = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        if file_type.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            let children = if depth < MAX_DEPTH {
                Some(read_dir_tree(&entry.path(), depth + 1)?)
            } else {
                None
            };
            nodes.push(TreeNode { name, is_dir: true, children });
        } else {
            nodes.push(TreeNode { name, is_dir: false, children: None });
        }
    }
    nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn node(name: &str, is_dir: bool) -> TreeNode {
        TreeNode { name: name.into(), is_dir, children: None }
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
}
