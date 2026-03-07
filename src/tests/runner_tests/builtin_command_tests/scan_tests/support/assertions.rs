use super::super::super::prelude::{
    assert_file_text_contains_all, assert_output_contains_all, assert_output_excludes_all, Path,
    RunnerError,
};

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_markdown_report_written(
    out: &str,
    report_path: &Path,
    scan: &str,
    report_rel: &str,
    expected_report_lines: &[&str],
) {
    let confirmation = format!("Wrote markdown {scan} report to {report_rel} (findings: 1).");
    assert!(
        out.contains(&confirmation),
        "missing markdown confirmation `{confirmation}` in output:\n{out}"
    );
    assert_file_text_contains_all(report_path, expected_report_lines);
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_threshold_option_rejected(
    err: RunnerError,
    scan: &str,
) {
    match err {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(
                message,
                format!("`scan {scan}` does not accept threshold options")
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_text_scan_is_clean(
    out: &str,
    title: &str,
    summary: &str,
    unexpected_snippets: &[&str],
) {
    assert_output_contains_all(out, &[title, summary]);
    if !unexpected_snippets.is_empty() {
        assert_output_excludes_all(out, unexpected_snippets);
    }
}
