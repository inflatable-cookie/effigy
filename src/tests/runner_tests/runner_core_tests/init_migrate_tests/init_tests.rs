use super::prelude::*;

#[test]
fn run_manifest_task_builtin_init_creates_scaffold_when_missing() {
    let root = temp_workspace("builtin-init-create");

    let out = run_builtin_ok(root.to_path_buf(), "init", &[]);
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

    let err = run_builtin_err(root.to_path_buf(), "init", &[]);
    assert_task_invocation_error_contains(err, &["already exists", "`effigy init --force`"]);

    let existing = fs::read_to_string(root.join("effigy.toml")).expect("read existing");
    assert!(existing.contains("old = \"printf old\""));
}

#[test]
fn run_manifest_task_builtin_init_force_overwrites_existing_manifest() {
    let root = temp_workspace("builtin-init-force-overwrite");
    write_root_manifest(&root, "[tasks]\nold = \"printf old\"\n");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--force"]);
    assert_contains_all(&out, &["Overwrote effigy.toml"]);

    let manifest = fs::read_to_string(root.join("effigy.toml")).expect("read overwritten");
    assert!(manifest.contains("ping = \"printf ok\""));
    assert!(!manifest.contains("old = \"printf old\""));
}

#[test]
fn run_manifest_task_builtin_init_dry_run_prints_scaffold_without_writing() {
    let root = temp_workspace("builtin-init-dry-run");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--dry-run"]);
    assert_contains_all(&out, &["[tasks]", "# [tasks.dev]"]);
    assert!(
        !root.join("effigy.toml").exists(),
        "dry-run should not write manifest"
    );
}

#[test]
fn run_manifest_task_builtin_init_json_reports_write_status() {
    let root = temp_workspace("builtin-init-json");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--json"]);
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
