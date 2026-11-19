//! Path and display helpers shared by the artifact substrate and the
//! deployment reports.

use std::path::Path;

/// Render `value` as one path component: anything outside `[A-Za-z0-9-_.]`
/// becomes `-`, so a value from a config file can never add a directory level.
pub fn safe_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

/// Show `path` relative to `repo_root` when it is inside the checkout, so a
/// report names a path a reader can open.
pub fn path_display(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{path_display, safe_path_component};
    use std::path::Path;

    #[test]
    fn safe_path_component_never_adds_a_directory_level() {
        assert_eq!(safe_path_component("uat/../etc"), "uat-..-etc");
        assert_eq!(safe_path_component("example-app"), "example-app");
    }

    #[test]
    fn path_display_strips_an_in_checkout_root() {
        let root = Path::new("/repo");
        assert_eq!(
            path_display(Path::new("/repo/.effigy/reports/x.json"), root),
            ".effigy/reports/x.json"
        );
        assert_eq!(path_display(Path::new("/other/x.json"), root), "/other/x.json");
    }
}
