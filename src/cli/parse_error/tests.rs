use std::path::Path;

use super::{parse_error_json_details, render_parse_error};
use effigy_ui::PlainRenderer;

#[test]
fn parse_error_json_details_contains_hint() {
    let details = parse_error_json_details();
    assert_eq!(
        details["hint"],
        "Run `effigy --help` to see supported command forms"
    );
}

#[test]
fn render_parse_error_includes_error_and_help_content() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    render_parse_error(
        &mut renderer,
        Path::new("/tmp/repo"),
        "--repo requires a value",
    )
    .expect("parse error render should succeed");
    let rendered =
        String::from_utf8(renderer.into_inner()).expect("rendered parse error should be utf8");
    assert!(rendered.contains("Invalid command arguments"));
    assert!(rendered.contains("--repo requires a value"));
    assert!(rendered.contains("Commands"));
    assert!(rendered.contains("--help"));
}
