use super::prelude::*;

#[test]
fn run_manifest_task_builtin_init_creates_scaffold_when_missing() {
    let root = temp_workspace("builtin-init-create");

    let out = run_builtin_ok(root.clone(), "init", &[]);
    assert_contains_all(&out, &["Created effigy.toml"]);

    let manifest = fs::read_to_string(root.join("effigy.toml")).expect("read created manifest");
    assert!(manifest.contains("[tasks]"));
    assert!(manifest.contains("ping = \"printf ok\""));
    assert!(manifest.contains("# [tasks.dev]"));
    assert!(manifest.contains("# [tasks.validate]"));

    let listed = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: Some("ping".to_owned()),
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect("generated scaffold should parse and list tasks");
    assert!(listed.contains("ping"));
}

#[test]
fn run_manifest_task_builtin_init_refuses_overwrite_without_force() {
    let root = temp_workspace("builtin-init-refuse-overwrite");
    write_root_manifest(&root, "[tasks]\nold = \"printf old\"\n");

    let err = run_builtin_err(root.clone(), "init", &[]);
    assert_task_invocation_error_contains(err, &["already exists", "`effigy init --force`"]);

    let existing = fs::read_to_string(root.join("effigy.toml")).expect("read existing");
    assert!(existing.contains("old = \"printf old\""));
}

#[test]
fn run_manifest_task_builtin_init_force_overwrites_existing_manifest() {
    let root = temp_workspace("builtin-init-force-overwrite");
    write_root_manifest(&root, "[tasks]\nold = \"printf old\"\n");

    let out = run_builtin_ok(root.clone(), "init", &["--force"]);
    assert_contains_all(&out, &["Overwrote effigy.toml"]);

    let manifest = fs::read_to_string(root.join("effigy.toml")).expect("read overwritten");
    assert!(manifest.contains("ping = \"printf ok\""));
    assert!(!manifest.contains("old = \"printf old\""));
}

#[test]
fn run_manifest_task_builtin_init_dry_run_prints_scaffold_without_writing() {
    let root = temp_workspace("builtin-init-dry-run");

    let out = run_builtin_ok(root.clone(), "init", &["--dry-run"]);
    assert_contains_all(&out, &["[tasks]", "# [tasks.dev]"]);
    assert!(
        !root.join("effigy.toml").exists(),
        "dry-run should not write manifest"
    );
}

#[test]
fn run_manifest_task_builtin_init_json_reports_write_status() {
    let root = temp_workspace("builtin-init-json");

    let out = run_builtin_ok(root.clone(), "init", &["--json"]);
    assert_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.v1\"",
            "\"written\": true",
            "\"dry_run\": false",
            "\"content\":",
        ],
    );
    assert!(root.join("effigy.toml").exists());
}

#[test]
fn run_manifest_task_builtin_migrate_preview_reports_candidates_without_writing() {
    let root = temp_workspace("builtin-migrate-preview");
    write_package_json_scripts(
        &root,
        &[("build", "npm run compile"), ("test", "vitest run")],
    );

    let out = run_builtin_ok(root.clone(), "migrate", &[]);
    assert_contains_all(
        &out,
        &[
            "Migrate Preview",
            "candidate scripts: 2",
            "+ tasks.build = \"npm run compile\"",
            "+ tasks.test = \"vitest run\"",
            "No files were modified.",
        ],
    );
    assert!(
        !root.join("effigy.toml").exists(),
        "preview mode should not write manifest"
    );
}

#[test]
fn run_manifest_task_builtin_migrate_apply_writes_ready_imports() {
    let root = temp_workspace("builtin-migrate-apply");
    write_package_json_scripts(
        &root,
        &[("build", "npm run compile"), ("test", "vitest run")],
    );

    let out = run_builtin_ok(root.clone(), "migrate", &["--apply"]);
    assert_contains_all(&out, &["mode: apply", "Applied: wrote"]);
    let manifest = fs::read_to_string(root.join("effigy.toml")).expect("read migrated manifest");
    assert!(manifest.contains("[tasks]"));
    assert!(manifest.contains("build = \"npm run compile\""));
    assert!(manifest.contains("test = \"vitest run\""));
}

#[test]
fn run_manifest_task_builtin_migrate_preserves_package_source_file() {
    let root = temp_workspace("builtin-migrate-preserves-source");
    let source = "{\n  \"scripts\": {\n    \"build\": \"npm run compile\"\n  }\n}\n";
    fs::write(root.join("package.json"), source).expect("write package scripts");

    let _ = run_builtin_ok(root.clone(), "migrate", &["--apply"]);

    let package_after = fs::read_to_string(root.join("package.json")).expect("read package");
    assert_eq!(package_after, source, "migration must be non-destructive");
}

#[test]
fn run_manifest_task_builtin_migrate_conflicts_require_manual_remediation() {
    let root = temp_workspace("builtin-migrate-conflicts");
    write_root_manifest(&root, "[tasks]\nbuild = \"printf old\"\n");
    write_package_json_scripts(&root, &[("build", "npm run compile"), ("lint", "eslint .")]);

    let out = run_builtin_ok(root.clone(), "migrate", &["--apply"]);
    assert_contains_all(
        &out,
        &[
            "Manual Remediation",
            "skip `build` (already defined in `[tasks]`)",
            "+ tasks.lint = \"eslint .\"",
        ],
    );
    let manifest = fs::read_to_string(root.join("effigy.toml")).expect("read migrated manifest");
    assert!(manifest.contains("build = \"printf old\""));
    assert!(manifest.contains("lint = \"eslint .\""));
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
    assert_contains_all(
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
fn run_manifest_task_builtin_migrate_validates_arguments() {
    let cases = [
        BuiltinErrorCase {
            workspace: "builtin-migrate-missing-from-value",
            command: "migrate",
            args: &["--from"],
            manifest: "",
            expected: &["`--from` requires a file path"],
        },
        BuiltinErrorCase {
            workspace: "builtin-migrate-missing-script-value",
            command: "migrate",
            args: &["--script"],
            manifest: "",
            expected: &["`--script` requires a script name"],
        },
        BuiltinErrorCase {
            workspace: "builtin-migrate-unknown-arg",
            command: "migrate",
            args: &["--wat"],
            manifest: "",
            expected: &["unknown argument(s) for built-in `migrate`: --wat"],
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_root_manifest(&root, case.manifest);
        let err = run_builtin_err(root, case.command, case.args);
        assert_task_invocation_error_contains(err, case.expected);
    }
}
