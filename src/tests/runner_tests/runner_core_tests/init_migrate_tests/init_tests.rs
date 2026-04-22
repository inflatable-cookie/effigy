use crate::runner::tests::prelude::{
    assert_file_text_contains_all, assert_file_text_excludes_all, assert_output_contains_all,
    assert_path_exists, assert_path_missing, assert_task_invocation_error_contains,
    run_builtin_err, run_builtin_ok, run_tasks, temp_workspace, write_root_manifest, TasksArgs,
};

#[test]
fn run_manifest_task_builtin_init_creates_scaffold_when_missing() {
    let root = temp_workspace("builtin-init-create");

    let out = run_builtin_ok(root.to_path_buf(), "init", &[]);
    assert_output_contains_all(&out, &["Created effigy.toml"]);
    assert_file_text_contains_all(
        &root.join("effigy.toml"),
        &[
            "[tasks]",
            "ping = \"printf ok\"",
            "# [tasks.dev]",
            "# [tasks.validate]",
        ],
    );

    let listed = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: Some("ping".to_owned()),
        resolve_selector: None,
        output_json: false,
        pretty_json: true,
    })
    .expect("generated scaffold should parse and list tasks");
    assert_output_contains_all(&listed, &["ping"]);
}

#[test]
fn run_manifest_task_builtin_init_refuses_overwrite_without_force() {
    let root = temp_workspace("builtin-init-refuse-overwrite");
    write_root_manifest(&root, "[tasks]\nold = \"printf old\"\n");

    let err = run_builtin_err(root.to_path_buf(), "init", &[]);
    assert_task_invocation_error_contains(err, &["already exists", "`effigy init --force`"]);
    assert_file_text_contains_all(&root.join("effigy.toml"), &["old = \"printf old\""]);
}

#[test]
fn run_manifest_task_builtin_init_force_overwrites_existing_manifest() {
    let root = temp_workspace("builtin-init-force-overwrite");
    write_root_manifest(&root, "[tasks]\nold = \"printf old\"\n");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--force"]);
    assert_output_contains_all(&out, &["Overwrote effigy.toml"]);
    assert_file_text_contains_all(&root.join("effigy.toml"), &["ping = \"printf ok\""]);
    assert_file_text_excludes_all(&root.join("effigy.toml"), &["old = \"printf old\""]);
}

#[test]
fn run_manifest_task_builtin_init_dry_run_prints_scaffold_without_writing() {
    let root = temp_workspace("builtin-init-dry-run");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--dry-run"]);
    assert_output_contains_all(&out, &["[tasks]", "# [tasks.dev]"]);
    assert_path_missing(&root.join("effigy.toml"), "dry-run manifest");
}

#[test]
fn run_manifest_task_builtin_init_json_reports_write_status() {
    let root = temp_workspace("builtin-init-json");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--json"]);
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.v1\"",
            "\"starter\": \"minimal\"",
            "\"written\": true",
            "\"dry_run\": false",
            "\"content\":",
        ],
    );
    assert_path_exists(&root.join("effigy.toml"), "init json manifest");
}

#[test]
fn run_manifest_task_builtin_init_positional_minimal_matches_default() {
    let root = temp_workspace("builtin-init-positional-minimal");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["minimal"]);
    assert_output_contains_all(&out, &["Created effigy.toml"]);
    assert_file_text_contains_all(
        &root.join("effigy.toml"),
        &["[tasks]", "ping = \"printf ok\""],
    );
}

#[test]
fn run_manifest_task_builtin_init_unknown_starter_errors_cleanly() {
    let root = temp_workspace("builtin-init-unknown-starter");

    let err = run_builtin_err(root.to_path_buf(), "init", &["does-not-exist"]);
    assert_task_invocation_error_contains(
        err,
        &[
            "failed to load starter",
            "does-not-exist",
            "starter not found",
        ],
    );
    assert_path_missing(
        &root.join("effigy.toml"),
        "unknown-starter run must not write a manifest",
    );
}

#[test]
fn run_manifest_task_builtin_init_extra_positional_errors() {
    let root = temp_workspace("builtin-init-extra-positional");

    let err = run_builtin_err(root.to_path_buf(), "init", &["minimal", "extra"]);
    assert_task_invocation_error_contains(
        err,
        &["accepts at most one starter name", "minimal", "extra"],
    );
}

#[test]
fn run_manifest_task_builtin_init_list_text_reports_registered_starters() {
    let root = temp_workspace("builtin-init-list-text");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--list"]);
    assert_output_contains_all(&out, &["Available starters:", "- minimal"]);
    assert_path_missing(
        &root.join("effigy.toml"),
        "`--list` must not emit a manifest",
    );
}

#[test]
fn run_manifest_task_builtin_init_list_json_reports_schema_and_entries() {
    let root = temp_workspace("builtin-init-list-json");

    let out = run_builtin_ok(root.to_path_buf(), "init", &["--list", "--json"]);
    assert_output_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.init.list.v1\"",
            "\"starters\":",
            "\"name\": \"minimal\"",
        ],
    );
    assert_path_missing(
        &root.join("effigy.toml"),
        "`--list --json` must not emit a manifest",
    );
}

#[test]
fn run_manifest_task_builtin_init_list_rejects_starter_name() {
    let root = temp_workspace("builtin-init-list-with-name");

    let err = run_builtin_err(root.to_path_buf(), "init", &["--list", "minimal"]);
    assert_task_invocation_error_contains(
        err,
        &["--list", "cannot be combined with a starter name"],
    );
}

#[test]
fn run_manifest_task_builtin_init_list_rejects_force_or_dry_run() {
    let root = temp_workspace("builtin-init-list-with-force");

    let err = run_builtin_err(root.to_path_buf(), "init", &["--list", "--force"]);
    assert_task_invocation_error_contains(
        err,
        &["--list", "cannot be combined with `--force` or `--dry-run`"],
    );
}
