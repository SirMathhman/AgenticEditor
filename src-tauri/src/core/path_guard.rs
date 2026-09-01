//! Shared path-traversal protection for the tauri-free core.
//!
//! Both `tree.rs` (project file operations) and `memory.rs` (agent memory
//! files) resolve relative paths against a root directory and must reject any
//! path that would escape that root. The lexical rejection rule lives here so
//! the two modules cannot drift apart. Canonicalization-based resolvers stay
//! in their respective modules because they have different requirements
//! (project writes require the parent to exist; memory writes create nested
//! directories).

use super::errors::AppError;
use std::path::Path;

/// Lexically rejects a relative path that would escape its root: an absolute
/// path (or one with a leading separator, which `is_absolute` misses on
/// Windows), or a `..` component that climbs above the root.
///
/// This is a pure lexical check — it does not touch the filesystem — so it can
/// be applied to paths that do not exist yet (and therefore cannot be
/// canonicalized).
pub fn reject_escaping_rel_path(rel_path: &str) -> Result<(), AppError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_relative_paths() {
        assert!(reject_escaping_rel_path("a.txt").is_ok());
        assert!(reject_escaping_rel_path("sub/inner.txt").is_ok());
        assert!(reject_escaping_rel_path("a\\b.txt").is_ok());
        assert!(reject_escaping_rel_path("./a.txt").is_ok());
    }

    #[test]
    fn rejects_dotdot_that_escapes() {
        assert!(reject_escaping_rel_path("../a.txt").is_err());
        assert!(reject_escaping_rel_path("a/../../b.txt").is_err());
        assert!(reject_escaping_rel_path("..").is_err());
    }

    #[test]
    fn accepts_dotdot_that_stays_within() {
        // `a/../b` resolves to `b`, which is still inside the root.
        assert!(reject_escaping_rel_path("a/../b.txt").is_ok());
    }

    #[test]
    fn rejects_absolute_and_leading_separator() {
        assert!(reject_escaping_rel_path("/etc/passwd").is_err());
        assert!(reject_escaping_rel_path("\\etc\\passwd").is_err());
        // On Windows a bare `/path` is not `is_absolute`, so the leading-
        // separator check is what catches it.
        assert!(reject_escaping_rel_path("/a.txt").is_err());
    }
}
