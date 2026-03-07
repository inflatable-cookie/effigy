use super::super::super::prelude::run_builtin_ok;
use super::assert_markdown_report_written;
use super::{setup_scan_workspace, write_attention_file};

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn run_marker_markdown_out_case(
    name: &str,
    scan: &str,
    report_rel: &str,
    source_lines: &[&str],
) -> (String, std::path::PathBuf) {
    let root = setup_scan_workspace(name, None, &["src"]);
    write_attention_file(&root.join("src/app.ts"), source_lines);
    let report_path = root.join(report_rel);
    let out = run_builtin_ok(
        root.clone(),
        "scan",
        &[scan, "--markdown", "--out", report_rel],
    );
    (out, report_path)
}

fn run_marker_manifest_defaults_case(
    name: &str,
    manifest_text: &str,
    scan: &str,
    report_rel: &str,
    source_lines: &[&str],
) -> (String, std::path::PathBuf) {
    let root = setup_scan_workspace(name, Some(manifest_text), &["src"]);
    write_attention_file(&root.join("src/app.ts"), source_lines);
    let report_path = root.join(report_rel);
    let out = run_builtin_ok(root, "scan", &[scan]);
    (out, report_path)
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_marker_manifest_defaults_report(
    name: &str,
    manifest_text: &str,
    scan: &str,
    report_rel: &str,
    source_lines: &[&str],
    expected_lines: &[&str],
) {
    let (out, report_path) =
        run_marker_manifest_defaults_case(name, manifest_text, scan, report_rel, source_lines);
    assert_markdown_report_written(&out, &report_path, scan, report_rel, expected_lines);
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn run_clean_scan_case(
    name: &str,
    dir: &str,
    path: &str,
    lines: &[&str],
    scan: &str,
) -> String {
    let root = setup_scan_workspace(name, None, &[dir]);
    write_attention_file(&root.join(path), lines);
    run_builtin_ok(root, "scan", &[scan, "--show-warnings"])
}
