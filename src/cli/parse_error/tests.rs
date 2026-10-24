use std::path::Path;

use effigy_cli::usage_hint::{usage_hint_for, UsageHint};
use effigy_ui::PlainRenderer;

use super::{parse_error_json_details, render_parse_error};

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn parse_error_json_details_contains_the_targeted_answer() {
    let hint = usage_hint_for(&argv(&["container", "exec", "app", "--", "true"]));
    let details = parse_error_json_details(&hint);
    assert_eq!(
        details["hint"],
        "Run `effigy container --help` for every form"
    );
    assert!(
        details["did_you_mean"]
            .as_str()
            .is_some_and(|text| text.contains("effigy exec")),
        "{details}"
    );
    assert_eq!(details["usage"], serde_json::json!([]));
}

#[test]
fn parse_error_json_details_without_a_command_points_at_the_general_help() {
    let details = parse_error_json_details(&UsageHint::default());
    assert_eq!(
        details["hint"],
        "Run `effigy --help` to see supported command forms"
    );
    assert_eq!(details["did_you_mean"], serde_json::Value::Null);
}

#[test]
fn render_parse_error_shows_the_nearest_usage_and_not_the_whole_help() {
    // Before (effigy#318): `unknown argument: app` followed by ~95 lines of
    // the general help panel.
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    let hint = usage_hint_for(&argv(&["tasks", "--repo"]));
    render_parse_error(
        &mut renderer,
        Path::new("/tmp/repo"),
        "--repo requires a value",
        &hint,
    )
    .expect("parse error render should succeed");
    let rendered =
        String::from_utf8(renderer.into_inner()).expect("rendered parse error should be utf8");
    assert!(rendered.contains("Invalid command arguments"));
    assert!(rendered.contains("--repo requires a value"));
    assert!(rendered.contains("effigy tasks"));
    assert!(
        rendered.contains("--help"),
        "the answer points at the command's own help: {rendered}"
    );
    assert!(
        !rendered.contains("Commands\n"),
        "an argument error never prints the whole help panel: {rendered}"
    );
    // One or two usage lines, not the topic's full list.
    let usage_lines = rendered
        .lines()
        .filter(|line| line.trim_start().starts_with("effigy "))
        .count();
    assert!(
        (1..=2).contains(&usage_lines),
        "expected one or two usage lines, saw {usage_lines}: {rendered}"
    );
}

#[test]
fn render_parse_error_names_the_supported_command_for_a_mistyped_one() {
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    let hint = usage_hint_for(&argv(&["container", "exec", "app"]));
    render_parse_error(
        &mut renderer,
        Path::new("/tmp/repo"),
        "unknown argument: app",
        &hint,
    )
    .expect("parse error render should succeed");
    let rendered =
        String::from_utf8(renderer.into_inner()).expect("rendered parse error should be utf8");
    assert!(rendered.contains("effigy container exec` does not exist"));
    assert!(rendered.contains("effigy exec"));
    assert!(!rendered.contains("Commands\n"));
}
