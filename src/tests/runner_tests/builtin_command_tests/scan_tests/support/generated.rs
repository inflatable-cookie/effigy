use super::assert_markdown_report_written;
use super::{setup_fanout_scan_workspace, setup_scan_workspace, write_asset_file};
use crate::runner::tests::prelude::{
    assert_json_array_field_non_empty, assert_json_string_field_eq, assert_output_contains_all, fs,
    parse_json_output_with_schema, run_builtin_ok, temp_workspace, write_manifest,
    write_root_manifest, Path,
};

fn run_generated_scan_json_case(
    name: &str,
    scan: &str,
    dir: &str,
    asset_rel: &str,
    args: &[&str],
) -> serde_json::Value {
    let root = temp_workspace(name);
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join(dir)).expect("mkdir generated scan dir");
    write_asset_file(&root.join(asset_rel), 180);
    let out = run_builtin_ok(root, "scan", args);
    parse_json_output_with_schema(
        &out,
        if scan == "generated-assets" {
            "effigy.scan.generated-assets.v1"
        } else {
            "effigy.scan.generated-in-src.v1"
        },
    )
}

fn run_generated_scan_markdown_out_case(
    name: &str,
    dir: &str,
    asset_rel: &str,
    report_rel: &str,
    args: &[&str],
) -> (String, std::path::PathBuf) {
    let root = setup_scan_workspace(name, None, &[dir]);
    write_asset_file(&root.join(asset_rel), 180);
    let report_path = root.join(report_rel);
    let out = run_builtin_ok(root.clone(), "scan", args);
    (out, report_path)
}

fn run_generated_scan_manifest_defaults_case(
    name: &str,
    manifest_text: &str,
    scan: &str,
    dir: &str,
    asset_rel: &str,
    report_rel: &str,
) -> (String, std::path::PathBuf) {
    let root = setup_scan_workspace(name, Some(manifest_text), &[dir]);
    write_asset_file(&root.join(asset_rel), 180);
    let report_path = root.join(report_rel);
    let out = run_builtin_ok(root, "scan", &[scan]);
    (out, report_path)
}

fn assert_generated_markdown_case(
    out: &str,
    report_path: &Path,
    scan: &str,
    report_rel: &str,
    expected_lines: &[&str],
) {
    assert_markdown_report_written(out, report_path, scan, report_rel, expected_lines);
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_generated_json_case(
    name: &str,
    scan: &str,
    dir: &str,
    asset_rel: &str,
    args: &[&str],
    category: Option<&str>,
) {
    let parsed = run_generated_scan_json_case(name, scan, dir, asset_rel, args);
    assert_json_string_field_eq(&parsed, "scan", scan);
    assert_json_string_field_eq(&parsed, "format", "text");
    assert_eq!(parsed["candidate_files"].as_u64(), Some(1));
    assert_eq!(parsed["finding_count"].as_u64(), Some(1));
    assert_json_array_field_non_empty(&parsed, "findings");
    assert_json_string_field_eq(&parsed["findings"][0], "path", asset_rel);
    assert_json_string_field_eq(&parsed["findings"][0], "severity", "warning");
    if let Some(category) = category {
        assert_json_string_field_eq(&parsed["findings"][0], "category", category);
    }
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_generated_text_report_case<
    F,
>(
    name: &str,
    dirs: &[&str],
    setup: F,
    args: &[&str],
    expected_output_lines: &[&str],
    unexpected_output_lines: &[&str],
) where
    F: FnOnce(&Path),
{
    let root = setup_scan_workspace(name, None, dirs);
    setup(&root);

    let out = run_builtin_ok(root, "scan", args);
    assert_output_contains_all(&out, expected_output_lines);
    crate::runner::tests::prelude::assert_output_excludes_all(&out, unexpected_output_lines);
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_generated_markdown_report(
    name: &str,
    scan: &str,
    dir: &str,
    asset_rel: &str,
    report_rel: &str,
    args: &[&str],
    expected_lines: &[&str],
) {
    let (out, report_path) =
        run_generated_scan_markdown_out_case(name, dir, asset_rel, report_rel, args);
    assert_generated_markdown_case(&out, &report_path, scan, report_rel, expected_lines);
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_generated_assets_markdown_out_report(
    name: &str,
) {
    assert_generated_markdown_report(
        name,
        "generated-assets",
        "dist",
        "dist/app.min.js",
        "reports/generated-assets.md",
        &[
            "generated-assets",
            "--warn",
            "100",
            "--markdown",
            "--out",
            "reports/generated-assets.md",
        ],
        &[
            "# Generated Assets",
            "| Severity | Size | Path | Reason |",
            "| warning | 180 B | `dist/app.min.js` | `vendor-or-build-path` |",
        ],
    );
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_generated_manifest_defaults_report(
    name: &str,
    manifest_text: &str,
    scan: &str,
    dir: &str,
    asset_rel: &str,
    report_rel: &str,
    expected_lines: &[&str],
) {
    let (out, report_path) = run_generated_scan_manifest_defaults_case(
        name,
        manifest_text,
        scan,
        dir,
        asset_rel,
        report_rel,
    );
    assert_generated_markdown_case(&out, &report_path, scan, report_rel, expected_lines);
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_generated_assets_manifest_defaults_report(
    name: &str,
) {
    assert_generated_manifest_defaults_report(
        name,
        r#"[scan.generated_assets]
warn = 100
high = 250
critical = 500
format = "markdown"
out = "reports/generated-assets.md"
"#,
        "generated-assets",
        "dist",
        "dist/app.min.js",
        "reports/generated-assets.md",
        &[
            "# Generated Assets",
            "- Thresholds (bytes): warn=`100` high=`250` critical=`500`",
            "| warning | 180 B | `dist/app.min.js` | `vendor-or-build-path` |",
        ],
    );
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_generated_in_src_markdown_out_report(
    name: &str,
) {
    assert_generated_markdown_report(
        name,
        "generated-in-src",
        "src",
        "src/client.generated.ts",
        "reports/generated-in-src.md",
        &[
            "generated-in-src",
            "--warn",
            "100",
            "--markdown",
            "--out",
            "reports/generated-in-src.md",
        ],
        &[
            "# Generated In Src",
            "| Severity | Size | Category | Reason | Path |",
            "| warning | 180 B | generated-filename | `filename-marker` | `src/client.generated.ts` |",
        ],
    );
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_generated_in_src_manifest_defaults_report(
    name: &str,
) {
    assert_generated_manifest_defaults_report(
        name,
        r#"[scan.generated_in_src]
warn = 100
high = 250
critical = 500
source_roots = ["src/**"]
format = "markdown"
out = "reports/generated-in-src.md"
"#,
        "generated-in-src",
        "src",
        "src/client.generated.ts",
        "reports/generated-in-src.md",
        &[
            "# Generated In Src",
            "- Thresholds (bytes): warn=`100` high=`250` critical=`500`",
            "- Source roots: `src/**`",
            "| warning | 180 B | generated-filename | `filename-marker` | `src/client.generated.ts` |",
        ],
    );
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_generated_scan_ignores_parent_gitignore(
    name: &str,
    scan: &str,
    child_dir: &str,
    asset_rel: &str,
    expected_path: &str,
) {
    let (root, child) = setup_fanout_scan_workspace(name, "catalog_a", child_dir);
    write_manifest(&root.join("effigy.toml"), "");
    write_asset_file(&child.join(asset_rel), 180);

    let out = run_builtin_ok(child, "scan", &[scan, "--warn", "100", "--show-warnings"]);
    assert_output_contains_all(&out, &["findings: 1", expected_path]);
}

pub(in crate::runner::tests::builtin_command_tests::scan_tests) fn assert_generated_scan_root_fans_out(
    name: &str,
    scan: &str,
    child_dir: &str,
    asset_rel: &str,
    expected_path: &str,
) {
    let (root, child) = setup_fanout_scan_workspace(name, "catalog_a", child_dir);
    write_asset_file(&child.join(asset_rel), 180);

    let out = run_builtin_ok(root, "scan", &[scan, "--warn", "100", "--show-warnings"]);
    assert_output_contains_all(&out, &["findings: 1", expected_path]);
}
