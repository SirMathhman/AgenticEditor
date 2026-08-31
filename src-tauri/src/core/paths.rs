use std::path::Path;

/// Returns a display-friendly form of `path`, stripping the Windows
/// extended-length prefix (`\\?\`) that `canonicalize` adds. UNC paths are
/// rendered as `\\server\share`. Non-Windows paths are returned unchanged.
pub fn display_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if let Some(unc) = rest.strip_prefix("UNC\\") {
            return format!("\\\\{unc}");
        }
        return rest.to_string();
    }
    s.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_extended_prefix() {
        assert_eq!(
            display_path(Path::new(r"\\?\C:\Projects\app")),
            "C:\\Projects\\app"
        );
    }

    #[test]
    fn leaves_plain_paths_unchanged() {
        assert_eq!(display_path(Path::new("C:\\plain")), "C:\\plain");
        assert_eq!(display_path(Path::new("/home/user")), "/home/user");
    }

    #[test]
    fn renders_unc_paths() {
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share")),
            "\\\\server\\share"
        );
    }
}
