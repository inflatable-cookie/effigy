use std::path::Path;

use super::*;

#[test]
fn read_parse_templates_are_stable() {
    let path = Path::new("/tmp/workspace/effigy.toml");
    assert_eq!(
        failed_to_read_path(path, "io-error"),
        "failed to read /tmp/workspace/effigy.toml: io-error"
    );
    assert_eq!(
        failed_to_parse_path(path, "parse-error"),
        "failed to parse /tmp/workspace/effigy.toml: parse-error"
    );
    assert_eq!(
        failed_to_write_path(path, "write-error"),
        "failed to write /tmp/workspace/effigy.toml: write-error"
    );
    assert_eq!(
        failed_to_render_path(path, "render-error"),
        "failed to render /tmp/workspace/effigy.toml: render-error"
    );
}

#[test]
fn doctor_toml_templates_are_stable() {
    let path = Path::new("/tmp/workspace/effigy.toml");
    assert_eq!(
        failed_to_parse_toml_syntax_in_path(path, "syntax-error"),
        "failed to parse TOML syntax in /tmp/workspace/effigy.toml: syntax-error"
    );
    assert_eq!(
        strict_manifest_parse_failed_in_path(path, "strict-error"),
        "strict manifest parse failed in /tmp/workspace/effigy.toml: strict-error"
    );
}
