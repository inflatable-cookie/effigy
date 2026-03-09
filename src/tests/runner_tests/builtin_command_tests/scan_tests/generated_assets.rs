use super::*;

#[test]
fn run_manifest_task_builtin_scan_generated_assets_text_reports_bulky_generated_paths() {
    assert_generated_text_report_case(
        "builtin-scan-generated-assets-text",
        &["src", "dist", "vendor"],
        |root| {
            write_large_code_file(&root.join("src/app.ts"), 40);
            write_asset_file(&root.join("dist/app.min.js"), 180);
            write_asset_file(&root.join("vendor/runtime.wasm"), 320);
        },
        &[
            "generated-assets",
            "--warn",
            "100",
            "--high",
            "250",
            "--critical",
            "500",
            "--show-warnings",
        ],
        &[
            "Generated Assets",
            "scanned-files: 5  candidate-files: 2  findings: 2",
            "findings: 2",
            "severity-counts: critical=0 high=1 warning=1",
            "dist/app.min.js",
            "vendor/runtime.wasm",
            "[vendor-or-build-path]",
        ],
        &["src/app.ts"],
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_assets_json_emits_machine_payload() {
    assert_generated_json_case(
        "builtin-scan-generated-assets-json",
        "generated-assets",
        "dist",
        "dist/app.min.js",
        &[
            "generated-assets",
            "--warn",
            "100",
            "--high",
            "250",
            "--critical",
            "500",
            "--json",
        ],
        None,
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_assets_markdown_out_writes_report() {
    assert_generated_assets_markdown_out_report("builtin-scan-generated-assets-markdown-out");
}

#[test]
fn run_manifest_task_builtin_scan_generated_assets_uses_manifest_defaults() {
    assert_generated_assets_manifest_defaults_report(
        "builtin-scan-generated-assets-manifest-defaults",
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_assets_ignores_parent_gitignore_above_scan_root() {
    assert_generated_scan_ignores_parent_gitignore(
        "builtin-scan-generated-assets-parent-ignore",
        "generated-assets",
        "dist",
        "dist/app.min.js",
        "dist/app.min.js",
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_assets_root_fans_out_across_child_catalogs() {
    assert_generated_scan_root_fans_out(
        "builtin-scan-generated-assets-root-fanout",
        "generated-assets",
        "dist",
        "dist/app.min.js",
        "catalog_a/dist/app.min.js",
    );
}
