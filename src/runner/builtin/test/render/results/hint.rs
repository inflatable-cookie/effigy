use serde_json::json;

use crate::runner::builtin::test::BuiltinTestExecResult;

pub(super) fn append_builtin_test_filter_hint(
    mut rendered: String,
    results: &[BuiltinTestExecResult],
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> String {
    if requested_suite.is_none() || passthrough.is_empty() {
        return rendered;
    }

    let failed = failed_commands(results);
    if failed.is_empty() {
        return rendered;
    }

    rendered.push_str("\nHint\n────\n");
    rendered.push_str(
        "Selected suite failed while using a test filter. This often means no tests matched.\n",
    );
    rendered.push_str("failed command(s):\n");
    for command in failed {
        rendered.push_str("- ");
        rendered.push_str(&command);
        rendered.push('\n');
    }
    rendered.push_str("Try again without the filter to verify suite execution.\n");
    rendered
}

pub(super) fn build_builtin_test_filter_hint_payload(
    results: &[BuiltinTestExecResult],
    requested_suite: Option<&str>,
    passthrough: &[String],
) -> Option<serde_json::Value> {
    if requested_suite.is_none() || passthrough.is_empty() {
        return None;
    }
    let failed = failed_commands(results);
    if failed.is_empty() {
        return None;
    }
    Some(json!({
        "kind": "selected-suite-filter-no-match",
        "message": "Selected suite failed while using a test filter. This often means no tests matched.",
        "failed_commands": failed,
        "suggestion": "Try again without the filter to verify suite execution.",
    }))
}

fn failed_commands(results: &[BuiltinTestExecResult]) -> Vec<String> {
    results
        .iter()
        .filter(|result| !result.success)
        .map(|result| result.command.clone())
        .collect::<Vec<String>>()
}
