use super::prelude::{
    assert_file_text_contains_all, assert_json_array_field_non_empty, assert_json_string_field_eq,
    assert_output_contains_all, assert_output_excludes_all, fs, parse_json_output_with_schema,
    run_builtin_err, run_builtin_ok, temp_workspace, write_manifest, write_root_manifest, Path,
    RunnerError,
};

fn write_large_code_file(path: &Path, line_count: usize) {
    let body = (0..line_count)
        .map(|idx| format!("const line_{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, format!("{body}\n")).expect("write large code file");
}

fn write_large_rust_file(path: &Path, line_count: usize) {
    let body = (0..line_count)
        .map(|idx| format!("pub fn line_{idx}() -> usize {{ {idx} }}"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, format!("{body}\n")).expect("write large rust file");
}

fn write_asset_file(path: &Path, size: usize) {
    fs::write(path, vec![b'a'; size]).expect("write asset file");
}

fn write_attention_file(path: &Path, lines: &[&str]) {
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write attention file");
}

fn write_duplicate_block_file(path: &Path, block_prefix: &str) {
    let mut lines = vec![format!("pub fn {block_prefix}_alpha() -> usize {{")];
    lines.push("    let seed = 1;".to_owned());
    for idx in 0..18 {
        lines.push(format!("    let acc_{idx} = seed + {idx};"));
    }
    lines.push("    acc_17".to_owned());
    lines.push("}".to_owned());
    let block = format!("{}\n", lines.join("\n"));
    fs::write(path, format!("// header comment\n{block}\n")).expect("write duplicate block file");
}

fn write_comment_ratio_file(path: &Path, comment_lines: usize, code_lines: usize) {
    let mut lines = (0..comment_lines)
        .map(|idx| format!("// commentary line {idx}"))
        .collect::<Vec<String>>();
    lines.extend((0..code_lines).map(|idx| format!("const line_{idx} = {idx};")));
    fs::write(path, format!("{}\n", lines.join("\n"))).expect("write comment ratio file");
}

#[test]
fn run_manifest_task_builtin_scan_god_files_text_ignores_docs_generated_and_gitignored_paths() {
    let root = temp_workspace("builtin-scan-god-files-text");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::create_dir_all(root.join("ignored")).expect("mkdir ignored");
    fs::write(root.join(".gitignore"), "ignored/\n").expect("write gitignore");
    fs::write(
        root.join("README.md"),
        (0..40)
            .map(|_| "documentation line")
            .collect::<Vec<&str>>()
            .join("\n"),
    )
    .expect("write docs");
    fs::write(
        root.join("src/generated.ts"),
        format!(
            "/* @generated */\n{}\n",
            (0..40)
                .map(|idx| format!("const generated_{idx} = {idx};"))
                .collect::<Vec<String>>()
                .join("\n")
        ),
    )
    .expect("write generated");
    write_large_code_file(&root.join("ignored/hidden.ts"), 18);
    write_large_code_file(&root.join("src/app.ts"), 12);

    let out = run_builtin_ok(
        root,
        "scan",
        &["god-files", "--threshold", "10", "--show-warnings"],
    );

    assert_output_contains_all(
        &out,
        &["God Files", "findings: 1", "src/app.ts", "12 code lines"],
    );
    assert_output_excludes_all(
        &out,
        &["README.md", "src/generated.ts", "ignored/hidden.ts"],
    );
}

#[test]
fn run_manifest_task_builtin_scan_god_files_text_hides_warning_rows_by_default() {
    let root = temp_workspace("builtin-scan-god-files-text-hide-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_large_code_file(&root.join("src/warn.ts"), 12);
    write_large_code_file(&root.join("src/high.ts"), 22);

    let out = run_builtin_ok(
        root,
        "scan",
        &[
            "god-files",
            "--warn",
            "10",
            "--high",
            "20",
            "--critical",
            "30",
        ],
    );

    assert_output_contains_all(
        &out,
        &[
            "findings: 2",
            "severity-counts: critical=0 high=1 warning=1",
            "warning-rows-hidden: 1  use --show-warnings to list them",
            "src/high.ts",
        ],
    );
    assert_output_excludes_all(&out, &["src/warn.ts"]);
}

#[test]
fn run_manifest_task_builtin_scan_god_files_show_warnings_lists_warning_rows() {
    let root = temp_workspace("builtin-scan-god-files-text-show-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_large_code_file(&root.join("src/warn.ts"), 12);
    write_large_code_file(&root.join("src/high.ts"), 22);

    let out = run_builtin_ok(
        root,
        "scan",
        &[
            "god-files",
            "--warn",
            "10",
            "--high",
            "20",
            "--critical",
            "30",
            "--show-warnings",
        ],
    );

    assert_output_contains_all(
        &out,
        &[
            "findings: 2",
            "severity-counts: critical=0 high=1 warning=1",
            "src/high.ts",
            "src/warn.ts",
        ],
    );
    assert_output_excludes_all(&out, &["warning-rows-hidden:"]);
}

#[test]
fn run_manifest_task_builtin_scan_god_files_skips_docs_examples_and_lockfiles_by_default() {
    let root = temp_workspace("builtin-scan-god-files-docs-and-lockfiles");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::create_dir_all(root.join("docs/guides/code")).expect("mkdir docs code");
    write_large_code_file(&root.join("src/app.ts"), 12);
    write_large_rust_file(&root.join("docs/guides/code/example.rs"), 30);
    fs::write(
        root.join("pnpm-lock.yaml"),
        (0..40)
            .map(|idx| format!("lock_{idx}: value_{idx}"))
            .collect::<Vec<String>>()
            .join("\n"),
    )
    .expect("write lockfile");

    let out = run_builtin_ok(
        root,
        "scan",
        &["god-files", "--threshold", "10", "--show-warnings"],
    );

    assert_output_contains_all(&out, &["findings: 1", "src/app.ts"]);
    assert_output_excludes_all(&out, &["docs/guides/code/example.rs", "pnpm-lock.yaml"]);
}

#[test]
fn run_manifest_task_builtin_scan_god_files_keeps_tests_but_skips_migrations_by_default() {
    let root = temp_workspace("builtin-scan-god-files-tests-not-migrations");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("tests")).expect("mkdir tests");
    fs::create_dir_all(root.join("migrations")).expect("mkdir migrations");
    write_large_rust_file(&root.join("tests/large_spec.rs"), 30);
    fs::write(
        root.join("migrations/202603051200__baseline.sql"),
        (0..40)
            .map(|idx| format!("insert into demo values ({idx});"))
            .collect::<Vec<String>>()
            .join("\n"),
    )
    .expect("write migration");

    let out = run_builtin_ok(
        root,
        "scan",
        &["god-files", "--threshold", "10", "--show-warnings"],
    );

    assert_output_contains_all(&out, &["findings: 1", "tests/large_spec.rs"]);
    assert_output_excludes_all(&out, &["migrations/202603051200__baseline.sql"]);
}

#[test]
fn run_manifest_task_builtin_scan_god_files_skips_examples_fixtures_and_benchmarks_by_default() {
    let root = temp_workspace("builtin-scan-god-files-non-prod-paths");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::create_dir_all(root.join("examples")).expect("mkdir examples");
    fs::create_dir_all(root.join("fixtures")).expect("mkdir fixtures");
    fs::create_dir_all(root.join("benchmarks")).expect("mkdir benchmarks");
    write_large_code_file(&root.join("src/app.ts"), 12);
    write_large_rust_file(&root.join("examples/demo.rs"), 30);
    write_large_rust_file(&root.join("fixtures/payload.rs"), 30);
    write_large_rust_file(&root.join("benchmarks/parser.rs"), 30);

    let out = run_builtin_ok(
        root,
        "scan",
        &["god-files", "--threshold", "10", "--show-warnings"],
    );

    assert_output_contains_all(&out, &["findings: 1", "src/app.ts"]);
    assert_output_excludes_all(
        &out,
        &[
            "examples/demo.rs",
            "fixtures/payload.rs",
            "benchmarks/parser.rs",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_god_files_json_emits_machine_payload() {
    let root = temp_workspace("builtin-scan-god-files-json");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_large_code_file(&root.join("src/app.ts"), 12);

    let out = run_builtin_ok(root, "scan", &["god-files", "--threshold", "10", "--json"]);

    let parsed = parse_json_output_with_schema(&out, "effigy.scan.god-files.v1");
    assert_json_string_field_eq(&parsed, "scan", "god-files");
    assert_json_string_field_eq(&parsed, "format", "text");
    assert_eq!(parsed["finding_count"].as_u64(), Some(1));
    assert_json_array_field_non_empty(&parsed, "findings");
    assert_json_string_field_eq(&parsed["findings"][0], "path", "src/app.ts");
    assert_json_string_field_eq(&parsed["findings"][0], "severity", "warning");
}

#[test]
fn run_manifest_task_builtin_scan_god_files_markdown_out_writes_report() {
    let root = temp_workspace("builtin-scan-god-files-markdown-out");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_large_code_file(&root.join("src/app.ts"), 12);
    let report_path = root.join("reports/god-files.md");

    let out = run_builtin_ok(
        root.clone(),
        "scan",
        &[
            "god-files",
            "--threshold",
            "10",
            "--markdown",
            "--out",
            "reports/god-files.md",
        ],
    );

    let expected = "Wrote markdown god-files report to reports/god-files.md (findings: 1).";
    assert_output_contains_all(&out, &[expected]);
    assert_file_text_contains_all(
        &report_path,
        &[
            "# God Files",
            "| Severity | Code Lines | Total Lines | Path |",
            "| warning | 12 | 12 | `src/app.ts` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_god_files_fail_on_findings_returns_non_zero() {
    let root = temp_workspace("builtin-scan-god-files-fail");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_large_code_file(&root.join("src/app.ts"), 12);

    let err = run_builtin_err(
        root,
        "scan",
        &[
            "god-files",
            "--threshold",
            "10",
            "--fail-on-findings",
            "--show-warnings",
        ],
    );

    match err {
        RunnerError::BuiltinScanNonZero {
            finding_count,
            rendered,
        } => {
            assert_eq!(finding_count, 1);
            assert_output_contains_all(&rendered, &["God Files", "src/app.ts"]);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_scan_god_files_uses_manifest_defaults_for_threshold_format_and_out() {
    let root = temp_workspace("builtin-scan-god-files-manifest-defaults");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.god_files]
warn = 10
high = 20
critical = 30
format = "markdown"
out = "reports/god-files.md"
"#,
    );
    write_large_code_file(&root.join("src/app.ts"), 12);
    let report_path = root.join("reports/god-files.md");

    let out = run_builtin_ok(root, "scan", &["god-files"]);

    assert_output_contains_all(
        &out,
        &["Wrote markdown god-files report to reports/god-files.md (findings: 1)."],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# God Files",
            "- Thresholds: warn=`10` high=`20` critical=`30`",
            "| warning | 12 | 12 | `src/app.ts` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_god_files_no_gitignore_flag_includes_ignored_paths() {
    let root = temp_workspace("builtin-scan-god-files-no-gitignore");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("ignored")).expect("mkdir ignored");
    fs::write(root.join(".gitignore"), "ignored/\n").expect("write gitignore");
    write_large_code_file(&root.join("ignored/hidden.ts"), 12);

    let out = run_builtin_ok(
        root,
        "scan",
        &[
            "god-files",
            "--threshold",
            "10",
            "--no-gitignore",
            "--show-warnings",
        ],
    );

    assert_output_contains_all(&out, &["findings: 1", "ignored/hidden.ts"]);
}

#[test]
fn run_manifest_task_builtin_scan_god_files_include_and_exclude_flags_override_traversal_scope() {
    let root = temp_workspace("builtin-scan-god-files-include-exclude");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");
    write_large_code_file(&root.join("src/app.ts"), 12);
    write_large_code_file(&root.join("scripts/dev.ts"), 14);

    let include_only = run_builtin_ok(
        root.clone(),
        "scan",
        &[
            "god-files",
            "--threshold",
            "10",
            "--include",
            "scripts/**",
            "--show-warnings",
        ],
    );
    assert_output_contains_all(&include_only, &["findings: 1", "scripts/dev.ts"]);
    assert_output_excludes_all(&include_only, &["src/app.ts"]);

    let exclude_scripts = run_builtin_ok(
        root,
        "scan",
        &[
            "god-files",
            "--threshold",
            "10",
            "--include",
            "scripts/**",
            "--include",
            "src/**",
            "--exclude",
            "scripts/**",
            "--show-warnings",
        ],
    );
    assert_output_contains_all(&exclude_scripts, &["findings: 1", "src/app.ts"]);
    assert_output_excludes_all(&exclude_scripts, &["scripts/dev.ts"]);
}

#[test]
fn run_manifest_task_builtin_scan_god_files_ignores_parent_gitignore_above_scan_root() {
    let root = temp_workspace("builtin-scan-god-files-parent-ignore");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(&root.join("effigy.toml"), "");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_large_rust_file(&farmyard.join("src/lib.rs"), 12);

    let out = run_builtin_ok(
        farmyard,
        "scan",
        &["god-files", "--threshold", "10", "--show-warnings"],
    );

    assert_output_contains_all(&out, &["findings: 1", "src/lib.rs", "12 code lines"]);
}

#[test]
fn run_manifest_task_builtin_scan_god_files_root_fans_out_across_child_catalogs() {
    let root = temp_workspace("builtin-scan-god-files-root-fanout");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(&root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_large_rust_file(&farmyard.join("src/lib.rs"), 12);

    let out = run_builtin_ok(
        root,
        "scan",
        &["god-files", "--threshold", "10", "--show-warnings"],
    );

    assert_output_contains_all(
        &out,
        &["findings: 1", "farmyard/src/lib.rs", "12 code lines"],
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_assets_text_reports_bulky_generated_paths() {
    let root = temp_workspace("builtin-scan-generated-assets-text");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::create_dir_all(root.join("dist")).expect("mkdir dist");
    fs::create_dir_all(root.join("vendor")).expect("mkdir vendor");
    write_large_code_file(&root.join("src/app.ts"), 40);
    write_asset_file(&root.join("dist/app.min.js"), 180);
    write_asset_file(&root.join("vendor/runtime.wasm"), 320);

    let out = run_builtin_ok(
        root,
        "scan",
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
    );

    assert_output_contains_all(
        &out,
        &[
            "Generated Assets",
            "scanned-files: 5  candidate-files: 2  findings: 2",
            "findings: 2",
            "severity-counts: critical=0 high=1 warning=1",
            "dist/app.min.js",
            "vendor/runtime.wasm",
            "[vendor-or-build-path]",
        ],
    );
    assert_output_excludes_all(&out, &["src/app.ts"]);
}

#[test]
fn run_manifest_task_builtin_scan_generated_assets_json_emits_machine_payload() {
    let root = temp_workspace("builtin-scan-generated-assets-json");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("dist")).expect("mkdir dist");
    write_asset_file(&root.join("dist/app.min.js"), 180);

    let out = run_builtin_ok(
        root,
        "scan",
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
    );

    let parsed = parse_json_output_with_schema(&out, "effigy.scan.generated-assets.v1");
    assert_json_string_field_eq(&parsed, "scan", "generated-assets");
    assert_json_string_field_eq(&parsed, "format", "text");
    assert_eq!(parsed["candidate_files"].as_u64(), Some(1));
    assert_eq!(parsed["finding_count"].as_u64(), Some(1));
    assert_json_array_field_non_empty(&parsed, "findings");
    assert_json_string_field_eq(&parsed["findings"][0], "path", "dist/app.min.js");
    assert_json_string_field_eq(&parsed["findings"][0], "severity", "warning");
}

#[test]
fn run_manifest_task_builtin_scan_generated_assets_markdown_out_writes_report() {
    let root = temp_workspace("builtin-scan-generated-assets-markdown-out");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("dist")).expect("mkdir dist");
    write_asset_file(&root.join("dist/app.min.js"), 180);
    let report_path = root.join("reports/generated-assets.md");

    let out = run_builtin_ok(
        root.clone(),
        "scan",
        &[
            "generated-assets",
            "--warn",
            "100",
            "--markdown",
            "--out",
            "reports/generated-assets.md",
        ],
    );

    assert_output_contains_all(
        &out,
        &["Wrote markdown generated-assets report to reports/generated-assets.md (findings: 1)."],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# Generated Assets",
            "| Severity | Size | Path | Reason |",
            "| warning | 180 B | `dist/app.min.js` | `vendor-or-build-path` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_assets_uses_manifest_defaults() {
    let root = temp_workspace("builtin-scan-generated-assets-manifest-defaults");
    fs::create_dir_all(root.join("dist")).expect("mkdir dist");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.generated_assets]
warn = 100
high = 250
critical = 500
format = "markdown"
out = "reports/generated-assets.md"
"#,
    );
    write_asset_file(&root.join("dist/app.min.js"), 180);
    let report_path = root.join("reports/generated-assets.md");

    let out = run_builtin_ok(root, "scan", &["generated-assets"]);

    assert_output_contains_all(
        &out,
        &["Wrote markdown generated-assets report to reports/generated-assets.md (findings: 1)."],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# Generated Assets",
            "- Thresholds (bytes): warn=`100` high=`250` critical=`500`",
            "| warning | 180 B | `dist/app.min.js` | `vendor-or-build-path` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_generated_assets_ignores_parent_gitignore_above_scan_root() {
    let root = temp_workspace("builtin-scan-generated-assets-parent-ignore");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("dist")).expect("mkdir farmyard dist");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(&root.join("effigy.toml"), "");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_asset_file(&farmyard.join("dist/app.min.js"), 180);

    let out = run_builtin_ok(
        farmyard,
        "scan",
        &["generated-assets", "--warn", "100", "--show-warnings"],
    );

    assert_output_contains_all(&out, &["findings: 1", "dist/app.min.js"]);
}

#[test]
fn run_manifest_task_builtin_scan_generated_assets_root_fans_out_across_child_catalogs() {
    let root = temp_workspace("builtin-scan-generated-assets-root-fanout");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("dist")).expect("mkdir farmyard dist");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(&root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_asset_file(&farmyard.join("dist/app.min.js"), 180);

    let out = run_builtin_ok(
        root,
        "scan",
        &["generated-assets", "--warn", "100", "--show-warnings"],
    );

    assert_output_contains_all(&out, &["findings: 1", "farmyard/dist/app.min.js"]);
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_hides_warning_rows_by_default() {
    let root = temp_workspace("builtin-scan-attention-markers-hide-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_attention_file(
        &root.join("src/app.ts"),
        &[
            "// TODO: tidy before refactor",
            "const live = 1;",
            "// FIXME: handle retries cleanly",
        ],
    );
    write_attention_file(
        &root.join("src/lib.rs"),
        &[
            "#[deprecated(note = \"use new_api\")]",
            "pub fn old_api() {}",
        ],
    );

    let out = run_builtin_ok(root, "scan", &["attention-markers"]);

    assert_output_contains_all(
        &out,
        &[
            "Attention Markers",
            "matched-lines: 3  findings: 3",
            "severity-counts: critical=0 high=2 warning=1",
            "warning-rows-hidden: 1  use --show-warnings to list them",
            "src/app.ts:3",
            "[FIXME]",
            "src/lib.rs:1",
            "deprecation",
        ],
    );
    assert_output_excludes_all(&out, &["[TODO]"]);
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_show_warnings_lists_warning_rows() {
    let root = temp_workspace("builtin-scan-attention-markers-show-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_attention_file(
        &root.join("src/app.ts"),
        &[
            "// TODO: tidy before refactor",
            "// FIXME: handle retries cleanly",
        ],
    );

    let out = run_builtin_ok(root, "scan", &["attention-markers", "--show-warnings"]);

    assert_output_contains_all(
        &out,
        &[
            "matched-lines: 2  findings: 2",
            "severity-counts: critical=0 high=1 warning=1",
            "[TODO]",
            "[FIXME]",
        ],
    );
    assert_output_excludes_all(&out, &["warning-rows-hidden:"]);
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_json_emits_machine_payload() {
    let root = temp_workspace("builtin-scan-attention-markers-json");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_attention_file(
        &root.join("src/app.ts"),
        &["// TODO: tidy before refactor", "const live = 1;"],
    );

    let out = run_builtin_ok(root, "scan", &["attention-markers", "--json"]);

    let parsed = parse_json_output_with_schema(&out, "effigy.scan.attention-markers.v1");
    assert_json_string_field_eq(&parsed, "scan", "attention-markers");
    assert_json_string_field_eq(&parsed, "format", "text");
    assert_eq!(parsed["matched_lines"].as_u64(), Some(1));
    assert_eq!(parsed["finding_count"].as_u64(), Some(1));
    assert_json_array_field_non_empty(&parsed, "findings");
    assert_json_string_field_eq(&parsed["findings"][0], "path", "src/app.ts");
    assert_json_string_field_eq(&parsed["findings"][0], "severity", "warning");
    assert_json_string_field_eq(&parsed["findings"][0], "category", "deferred-work");
    assert_json_string_field_eq(&parsed["findings"][0], "marker", "TODO");
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_markdown_out_writes_report() {
    let root = temp_workspace("builtin-scan-attention-markers-markdown-out");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_attention_file(
        &root.join("src/app.ts"),
        &["// FIXME: handle retries cleanly", "const live = 1;"],
    );
    let report_path = root.join("reports/attention-markers.md");

    let out = run_builtin_ok(
        root.clone(),
        "scan",
        &[
            "attention-markers",
            "--markdown",
            "--out",
            "reports/attention-markers.md",
        ],
    );

    assert_output_contains_all(
        &out,
        &["Wrote markdown attention-markers report to reports/attention-markers.md (findings: 1)."],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# Attention Markers",
            "| Severity | Category | Marker | Path | Line | Snippet |",
            "| high | deferred-work | `FIXME` | `src/app.ts` | 1 | `// FIXME: handle retries cleanly` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_uses_manifest_defaults() {
    let root = temp_workspace("builtin-scan-attention-markers-manifest-defaults");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.attention_markers]
warning = ["LATER"]
high = ["SHIPME"]
critical = ["BLOCKER"]
format = "markdown"
out = "reports/attention-markers.md"
"#,
    );
    write_attention_file(
        &root.join("src/app.ts"),
        &["// SHIPME: split this module before release"],
    );
    let report_path = root.join("reports/attention-markers.md");

    let out = run_builtin_ok(root, "scan", &["attention-markers"]);

    assert_output_contains_all(
        &out,
        &[
            "Wrote markdown attention-markers report to reports/attention-markers.md (findings: 1).",
        ],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# Attention Markers",
            "- Markers: warning=`1` high=`1` critical=`1`",
            "| high | deferred-work | `SHIPME` | `src/app.ts` | 1 | `// SHIPME: split this module before release` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_root_fans_out_across_child_catalogs() {
    let root = temp_workspace("builtin-scan-attention-markers-root-fanout");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n")
        .expect("write root gitignore");
    write_manifest(&root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_attention_file(
        &farmyard.join("src/lib.rs"),
        &["// TODO: revisit bootstrap ordering"],
    );

    let out = run_builtin_ok(root, "scan", &["attention-markers", "--show-warnings"]);

    assert_output_contains_all(&out, &["findings: 1", "farmyard/src/lib.rs:1", "[TODO]"]);
}

#[test]
fn run_manifest_task_builtin_scan_attention_markers_rejects_threshold_flags() {
    let root = temp_workspace("builtin-scan-attention-markers-threshold-reject");
    write_root_manifest(&root, "");

    let err = run_builtin_err(root, "scan", &["attention-markers", "--warn", "10"]);

    match err {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(
                message,
                "`scan attention-markers` does not accept threshold options"
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_scan_comment_ratio_hides_warning_rows_by_default() {
    let root = temp_workspace("builtin-scan-comment-ratio-hide-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_comment_ratio_file(&root.join("src/warn.ts"), 30, 20);
    write_comment_ratio_file(&root.join("src/high.ts"), 50, 20);

    let out = run_builtin_ok(
        root,
        "scan",
        &[
            "comment-ratio",
            "--warn",
            "1.0",
            "--high",
            "2.0",
            "--critical",
            "3.0",
            "--min-code-lines",
            "20",
        ],
    );

    assert_output_contains_all(
        &out,
        &[
            "Comment Ratio",
            "candidate-files: 2",
            "findings: 2",
            "severity-counts: critical=0 high=1 warning=1",
            "warning-rows-hidden: 1  use --show-warnings to list them",
            "src/high.ts",
        ],
    );
    assert_output_excludes_all(&out, &["src/warn.ts"]);
}

#[test]
fn run_manifest_task_builtin_scan_comment_ratio_show_warnings_lists_rows() {
    let root = temp_workspace("builtin-scan-comment-ratio-show-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_comment_ratio_file(&root.join("src/warn.ts"), 30, 20);

    let out = run_builtin_ok(
        root,
        "scan",
        &[
            "comment-ratio",
            "--warn",
            "1.0",
            "--high",
            "2.0",
            "--critical",
            "3.0",
            "--min-code-lines",
            "20",
            "--show-warnings",
        ],
    );

    assert_output_contains_all(
        &out,
        &[
            "findings: 1",
            "severity-counts: critical=0 high=0 warning=1",
            "warning  ratio=1.50  30 comment / 20 code  src/warn.ts",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_comment_ratio_json_emits_machine_payload() {
    let root = temp_workspace("builtin-scan-comment-ratio-json");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_comment_ratio_file(&root.join("src/warn.ts"), 30, 20);

    let out = run_builtin_ok(
        root,
        "scan",
        &[
            "comment-ratio",
            "--warn",
            "1.0",
            "--min-code-lines",
            "20",
            "--json",
        ],
    );

    let parsed = parse_json_output_with_schema(&out, "effigy.scan.comment-ratio.v1");
    assert_json_string_field_eq(&parsed, "scan", "comment-ratio");
    assert_json_string_field_eq(&parsed, "format", "text");
    assert_eq!(parsed["candidate_files"].as_u64(), Some(1));
    assert_eq!(parsed["finding_count"].as_u64(), Some(1));
    assert_json_array_field_non_empty(&parsed, "findings");
    assert_json_string_field_eq(&parsed["findings"][0], "severity", "warning");
}

#[test]
fn run_manifest_task_builtin_scan_comment_ratio_uses_manifest_defaults_and_root_fans_out() {
    let root = temp_workspace("builtin-scan-comment-ratio-manifest-fanout");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n").expect("write gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "root"

[scan.comment_ratio]
warn = 1.0
high = 2.0
critical = 3.0
min_code_lines = 20
format = "markdown"
out = "reports/comment-ratio.md"
"#,
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_comment_ratio_file(&farmyard.join("src/lib.ts"), 30, 20);
    let report_path = root.join("reports/comment-ratio.md");

    let out = run_builtin_ok(root, "scan", &["comment-ratio"]);

    assert_output_contains_all(
        &out,
        &["Wrote markdown comment-ratio report to reports/comment-ratio.md (findings: 1)."],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# Comment Ratio",
            "- Findings: `1`",
            "| warning | 1.50 | 30 | 20 | `farmyard/src/lib.ts` |",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_duplicate_blocks_hides_warning_rows_by_default() {
    let root = temp_workspace("builtin-scan-duplicate-blocks-hide-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");

    let out = run_builtin_ok(root, "scan", &["duplicate-blocks"]);

    assert_output_contains_all(
        &out,
        &[
            "Duplicate Blocks",
            "candidate-blocks:",
            "findings: 1",
            "severity-counts: critical=0 high=0 warning=1",
            "warning-rows-hidden: 1  use --show-warnings to list them",
            "No high or critical duplicate blocks found.",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_duplicate_blocks_show_warnings_lists_locations() {
    let root = temp_workspace("builtin-scan-duplicate-blocks-show-warnings");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");

    let out = run_builtin_ok(root, "scan", &["duplicate-blocks", "--show-warnings"]);

    assert_output_contains_all(
        &out,
        &[
            "findings: 1",
            "severity-counts: critical=0 high=0 warning=1",
            "warning  22 lines  2 occurrences",
            "[src/alpha.rs:2-23, src/beta.rs:2-23]",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_duplicate_blocks_json_emits_machine_payload() {
    let root = temp_workspace("builtin-scan-duplicate-blocks-json");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");

    let out = run_builtin_ok(root, "scan", &["duplicate-blocks", "--json"]);

    let parsed = parse_json_output_with_schema(&out, "effigy.scan.duplicate-blocks.v1");
    assert_json_string_field_eq(&parsed, "scan", "duplicate-blocks");
    assert_json_string_field_eq(&parsed, "format", "text");
    assert_eq!(parsed["candidate_blocks"].as_u64(), Some(6));
    assert_eq!(parsed["finding_count"].as_u64(), Some(1));
    assert_json_array_field_non_empty(&parsed, "findings");
    assert_json_string_field_eq(&parsed["findings"][0], "severity", "warning");
    assert_eq!(
        parsed["findings"][0]["locations"]
            .as_array()
            .map(|v| v.len()),
        Some(2)
    );
}

#[test]
fn run_manifest_task_builtin_scan_duplicate_blocks_markdown_out_writes_report() {
    let root = temp_workspace("builtin-scan-duplicate-blocks-markdown-out");
    write_root_manifest(&root, "");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_duplicate_block_file(&root.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&root.join("src/beta.rs"), "shared");
    let report_path = root.join("reports/duplicate-blocks.md");

    let out = run_builtin_ok(
        root.clone(),
        "scan",
        &[
            "duplicate-blocks",
            "--markdown",
            "--out",
            "reports/duplicate-blocks.md",
        ],
    );

    assert_output_contains_all(
        &out,
        &["Wrote markdown duplicate-blocks report to reports/duplicate-blocks.md (findings: 1)."],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# Duplicate Blocks",
            "| Severity | Lines | Occurrences | Fingerprint | Snippet | Locations |",
            "| warning | 22 | 2 | `",
            "src/alpha.rs:2-23<br>src/beta.rs:2-23",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_scan_duplicate_blocks_uses_manifest_defaults_and_root_fans_out() {
    let root = temp_workspace("builtin-scan-duplicate-blocks-manifest-fanout");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(farmyard.join("src")).expect("mkdir farmyard src");
    fs::write(root.join(".gitignore"), "*\n!.gitignore\n!effigy.toml\n").expect("write gitignore");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "root"

[scan.duplicate_blocks]
format = "markdown"
out = "reports/duplicate-blocks.md"
"#,
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n",
    );
    write_duplicate_block_file(&farmyard.join("src/alpha.rs"), "shared");
    write_duplicate_block_file(&farmyard.join("src/beta.rs"), "shared");
    let report_path = root.join("reports/duplicate-blocks.md");

    let out = run_builtin_ok(root, "scan", &["duplicate-blocks"]);

    assert_output_contains_all(
        &out,
        &["Wrote markdown duplicate-blocks report to reports/duplicate-blocks.md (findings: 1)."],
    );
    assert_file_text_contains_all(
        &report_path,
        &[
            "# Duplicate Blocks",
            "- Findings: `1`",
            "farmyard/src/alpha.rs:2-23<br>farmyard/src/beta.rs:2-23",
        ],
    );
}
