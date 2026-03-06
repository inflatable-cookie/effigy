use super::prelude::{
    assert_builtin_argument_contract_case_table, assert_file_text_contains_all,
    assert_file_text_equals, assert_output_contains_all, assert_path_missing, fs, run_builtin_ok,
    temp_workspace, write_package_json_scripts, write_root_manifest, BuiltinArgumentContractCase,
};

#[test]
fn run_manifest_task_builtin_migrate_preview_reports_candidates_without_writing() {
    let root = temp_workspace("builtin-migrate-preview");
    write_package_json_scripts(
        &root,
        &[("build", "npm run compile"), ("test", "vitest run")],
    );

    let out = run_builtin_ok(root.to_path_buf(), "migrate", &[]);
    assert_output_contains_all(
        &out,
        &[
            "Migrate Preview",
            "candidate scripts: 2",
            "+ tasks.build = \"npm run compile\"",
            "+ tasks.test = \"vitest run\"",
            "No files were modified.",
        ],
    );
    assert_path_missing(&root.join("effigy.toml"), "migrate preview manifest");
}

#[test]
fn run_manifest_task_builtin_migrate_apply_writes_ready_imports() {
    let root = temp_workspace("builtin-migrate-apply");
    write_package_json_scripts(
        &root,
        &[("build", "npm run compile"), ("test", "vitest run")],
    );

    let out = run_builtin_ok(root.to_path_buf(), "migrate", &["--apply"]);
    assert_output_contains_all(&out, &["mode: apply", "Applied: wrote"]);
    assert_file_text_contains_all(
        &root.join("effigy.toml"),
        &[
            "[tasks]",
            "build = \"npm run compile\"",
            "test = \"vitest run\"",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_migrate_preserves_package_source_file() {
    let root = temp_workspace("builtin-migrate-preserves-source");
    let source = "{\n  \"scripts\": {\n    \"build\": \"npm run compile\"\n  }\n}\n";
    fs::write(root.join("package.json"), source).expect("write package scripts");

    let _ = run_builtin_ok(root.to_path_buf(), "migrate", &["--apply"]);

    assert_file_text_equals(&root.join("package.json"), source);
}

#[test]
fn run_manifest_task_builtin_migrate_conflicts_require_manual_remediation() {
    let root = temp_workspace("builtin-migrate-conflicts");
    write_root_manifest(&root, "[tasks]\nbuild = \"printf old\"\n");
    write_package_json_scripts(&root, &[("build", "npm run compile"), ("lint", "eslint .")]);

    let out = run_builtin_ok(root.to_path_buf(), "migrate", &["--apply"]);
    assert_output_contains_all(
        &out,
        &[
            "Manual Remediation",
            "skip `build` (already defined in `[tasks]`)",
            "+ tasks.lint = \"eslint .\"",
        ],
    );
    assert_file_text_contains_all(
        &root.join("effigy.toml"),
        &["build = \"printf old\"", "lint = \"eslint .\""],
    );
}

#[test]
fn run_manifest_task_builtin_migrate_json_reports_schema_and_conflicts() {
    let root = temp_workspace("builtin-migrate-json");
    write_root_manifest(&root, "[tasks]\nbuild = \"printf old\"\n");
    write_package_json_scripts(
        &root,
        &[("build", "npm run compile"), ("test", "vitest run")],
    );

    let out = run_builtin_ok(root, "migrate", &["--json"]);
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.migrate.v1\"",
            "\"apply\": false",
            "\"name\": \"test\"",
            "\"name\": \"build\"",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_migrate_argument_contract_table() {
    let cases = [
        BuiltinArgumentContractCase {
            workspace: "builtin-migrate-missing-from-value",
            args: &["--from"],
            expect_error: true,
            expected: &["`--from` requires a file path"],
        },
        BuiltinArgumentContractCase {
            workspace: "builtin-migrate-missing-script-value",
            args: &["--script"],
            expect_error: true,
            expected: &["`--script` requires a script name"],
        },
        BuiltinArgumentContractCase {
            workspace: "builtin-migrate-unknown-arg",
            args: &["--wat"],
            expect_error: true,
            expected: &["unknown argument(s) for built-in `migrate`: --wat"],
        },
        BuiltinArgumentContractCase {
            workspace: "builtin-migrate-help-precedence",
            args: &["--help", "--wat"],
            expect_error: false,
            expected: &["migrate Help", "Usage"],
        },
    ];

    assert_builtin_argument_contract_case_table("migrate", &cases);
}
