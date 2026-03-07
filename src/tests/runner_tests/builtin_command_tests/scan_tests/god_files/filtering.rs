use super::*;

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
