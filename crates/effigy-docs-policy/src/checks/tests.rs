use super::*;

#[test]
fn validate_json_examples_passes_with_enough_blocks() {
    let content = "## Examples\n```json\n{\"ok\":true,\"schema\":\"v1\"}\n```\n```json\n{\"ok\":false,\"schema\":\"v1\"}\n```\n";
    let result = validate_json_examples(
        content,
        "test.md",
        "Examples",
        2,
        &["\"schema\":\"v1\"".to_string()],
        &[],
    );
    assert!(result.ok);
    assert_eq!(result.block_count, 2);
}

#[test]
fn validate_json_examples_fails_with_too_few_blocks() {
    let content = "## Examples\n```json\n{}\n```\n";
    let result = validate_json_examples(content, "test.md", "Examples", 3, &[], &[]);
    assert!(!result.ok);
    assert!(result.failures[0].contains("expected at least 3"));
}

#[test]
fn validate_json_examples_fails_missing_section() {
    let content = "## Other\nstuff\n";
    let result = validate_json_examples(content, "test.md", "Missing", 1, &[], &[]);
    assert!(!result.ok);
    assert!(result.failures[0].contains("not found"));
}

#[test]
fn validate_json_examples_checks_required_blocks() {
    let content = "## Ex\n```json\n{\"hit\":true}\n```\n```json\n{\"miss\":true}\n```\n";
    let result = validate_json_examples(
        content,
        "test.md",
        "Ex",
        1,
        &[],
        &[
            (1, "\"hit\":true".to_string()),
            (2, "\"hit\":true".to_string()),
        ],
    );
    assert!(!result.ok);
    assert!(result.failures.iter().any(|f| f.contains("block #2")));
}
