use super::*;

#[test]
fn run_manifest_task_builtin_scan_generated_in_src_text_reports_generated_files_in_source_roots() {
    assert_generated_text_report_case(
        "builtin-scan-generated-in-src-text",
        &["src", "dist"],
        |root| {
            write_large_code_file(&root.join("src/app.ts"), 20);
            write_asset_file(&root.join("src/client.generated.ts"), 180);
            write_asset_file(&root.join("dist/client.generated.ts"), 220);
        },
        &[
            "generated-in-src",
            "--warn",
            "100",
            "--high",
            "250",
            "--critical",
            "500",
            "--show-warnings",
        ],
        &[
            "Generated In Src",
            "scanned-files: 2  candidate-files: 1  findings: 1",
            "severity-counts: critical=0 high=0 warning=1",
            "src/client.generated.ts",
            "[generated-filename] [filename-marker]",
        ],
        &["dist/client.generated.ts", "src/app.ts"],
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_in_src_json_emits_machine_payload() {
    assert_generated_json_case(
        "builtin-scan-generated-in-src-json",
        "generated-in-src",
        "src",
        "src/client.generated.ts",
        &[
            "generated-in-src",
            "--warn",
            "100",
            "--high",
            "250",
            "--critical",
            "500",
            "--json",
        ],
        Some("generated-filename"),
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_in_src_markdown_out_writes_report() {
    assert_generated_in_src_markdown_out_report("builtin-scan-generated-in-src-markdown-out");
}

#[test]
fn run_manifest_task_builtin_scan_generated_in_src_uses_manifest_defaults() {
    assert_generated_in_src_manifest_defaults_report(
        "builtin-scan-generated-in-src-manifest-defaults",
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_in_src_ignores_parent_gitignore_above_scan_root() {
    assert_generated_scan_ignores_parent_gitignore(
        "builtin-scan-generated-in-src-parent-ignore",
        "generated-in-src",
        "src",
        "src/client.generated.ts",
        "src/client.generated.ts",
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_in_src_root_fans_out_across_child_catalogs() {
    assert_generated_scan_root_fans_out(
        "builtin-scan-generated-in-src-root-fanout",
        "generated-in-src",
        "src",
        "src/client.generated.ts",
        "farmyard/src/client.generated.ts",
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_in_src_ignores_marker_literals_in_code() {
    let out = run_clean_scan_case(
        "builtin-scan-generated-in-src-ignore-marker-literals",
        "src",
        "src/app.rs",
        &[
            "const GENERATED: &str = \"@generated\";",
            "const DO_NOT_EDIT: &str = \"do not edit\";",
        ],
        "generated-in-src",
    );

    assert_text_scan_is_clean(
        &out,
        "Generated In Src",
        "candidate-files: 0  findings: 0",
        &[],
    );
}
