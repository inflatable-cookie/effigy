use super::harness::parse_json_output;

pub(in crate::runner::tests) fn parse_json_output_with_schema(
    rendered: &str,
    expected_schema: &str,
) -> serde_json::Value {
    let parsed = parse_json_output(rendered);
    assert_json_schema(&parsed, expected_schema);
    parsed
}

pub(in crate::runner::tests) fn parse_json_output_with_schema_version(
    rendered: &str,
    expected_schema: &str,
    expected_version: u64,
) -> serde_json::Value {
    let parsed = parse_json_output_with_schema(rendered, expected_schema);
    assert_json_schema_version(&parsed, expected_version);
    parsed
}

pub(in crate::runner::tests) fn assert_json_schema(parsed: &serde_json::Value, expected: &str) {
    assert_json_string_field_eq(parsed, "schema", expected);
}

pub(in crate::runner::tests) fn assert_json_schema_version(
    parsed: &serde_json::Value,
    expected: u64,
) {
    let actual = parsed["schema_version"].as_u64();
    assert_eq!(
        actual,
        Some(expected),
        "expected `schema_version`={expected}, got {actual:?}"
    );
}

pub(in crate::runner::tests) fn assert_json_string_field_eq(
    parsed: &serde_json::Value,
    field: &str,
    expected: &str,
) {
    let actual = parsed[field].as_str();
    assert_eq!(
        actual,
        Some(expected),
        "expected `{field}`={expected:?}, got {actual:?}"
    );
}

pub(in crate::runner::tests) fn assert_json_bool_field_eq(
    parsed: &serde_json::Value,
    field: &str,
    expected: bool,
) {
    let actual = parsed[field].as_bool();
    assert_eq!(
        actual,
        Some(expected),
        "expected `{field}`={expected}, got {actual:?}"
    );
}

pub(in crate::runner::tests) fn assert_json_array_field(parsed: &serde_json::Value, field: &str) {
    assert!(
        parsed[field].is_array(),
        "expected `{field}` to be an array, got {}",
        parsed[field]
    );
}

pub(in crate::runner::tests) fn assert_json_array_field_non_empty(
    parsed: &serde_json::Value,
    field: &str,
) {
    let items = parsed[field].as_array().expect("json array field");
    assert!(
        !items.is_empty(),
        "expected `{field}` array to be non-empty"
    );
}

pub(in crate::runner::tests) fn assert_json_object_field(parsed: &serde_json::Value, field: &str) {
    assert!(
        parsed[field].is_object(),
        "expected `{field}` to be an object, got {}",
        parsed[field]
    );
}
