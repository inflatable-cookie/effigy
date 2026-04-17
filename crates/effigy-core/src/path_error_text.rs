use std::path::Path;

pub fn failed_to_read_path(path: &Path, error: impl std::fmt::Display) -> String {
    format!("failed to read {}: {error}", path.display())
}

pub fn failed_to_parse_path(path: &Path, error: impl std::fmt::Display) -> String {
    format!("failed to parse {}: {error}", path.display())
}

pub fn failed_to_write_path(path: &Path, error: impl std::fmt::Display) -> String {
    format!("failed to write {}: {error}", path.display())
}

pub fn failed_to_render_path(path: &Path, error: impl std::fmt::Display) -> String {
    format!("failed to render {}: {error}", path.display())
}

pub fn failed_to_parse_toml_syntax_in_path(path: &Path, error: impl std::fmt::Display) -> String {
    format!("failed to parse TOML syntax in {}: {error}", path.display())
}

pub fn strict_manifest_parse_failed_in_path(path: &Path, error: impl std::fmt::Display) -> String {
    format!(
        "strict manifest parse failed in {}: {error}",
        path.display()
    )
}

#[cfg(test)]
#[path = "path_error_text/tests.rs"]
mod tests;
