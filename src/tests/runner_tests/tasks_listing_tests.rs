use super::*;

#[test]
fn run_tasks_lists_catalogs_and_tasks() {
    let root = temp_workspace("list-tasks");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[tasks.reset-db]\nrun = \"printf farmyard\"\n",
    );

    let out = run_tasks_from_repo(&root, None, None, false);
    assert_contains_all(&out, &["root", "farmyard", "reset-db"]);
}

#[test]
fn run_tasks_supports_compact_task_definitions() {
    let root = temp_workspace("compact-tasks");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks]
api = "printf api"
jobs = "printf jobs"
"#,
    );

    let out = run_tasks_from_repo(&root, None, None, false);
    assert_contains_all(&out, &["api", "jobs", "printf api"]);
}

#[test]
fn run_tasks_supports_mixed_compact_and_table_task_definitions() {
    let root = temp_workspace("mixed-compact-and-table");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks]
api = "printf api"

[tasks.dev]
run = "printf dev"
"#,
    );

    let out = run_builtin_ok(root.clone(), "api", &[]);
    assert_eq!(out, "");

    let tasks = run_tasks_from_repo(&root, None, None, false);
    assert_contains_all(&tasks, &["api", "dev"]);
}

#[test]
fn run_tasks_supports_compact_sequence_task_definitions() {
    let root = temp_workspace("compact-sequence-tasks");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks]
drop-db = "printf drop-db"
migrate-db = "printf migrate-db"
reset-db = [{ task = "drop-db" }, { task = "migrate-db" }]
"#,
    );

    let out = run_builtin_ok(root.clone(), "reset-db", &[]);
    assert_eq!(out, "");

    let tasks = run_tasks_from_repo(&root, Some("reset-db"), None, false);
    assert_contains_all(&tasks, &["reset-db", "<sequence:2>"]);
}

#[test]
fn run_tasks_with_task_filter_reports_only_matches() {
    let root = temp_workspace("task-filter");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[tasks.reset-db]\nrun = \"printf farmyard\"\n",
    );

    let out = run_tasks_from_repo(&root, Some("reset-db"), None, false);
    assert_contains_all(&out, &["Task Matches: reset-db", "farmyard", "reset-db"]);
    assert!(!out.contains("root      │ reset-db"));
}

#[test]
fn run_tasks_with_test_filter_shows_catalog_fallback_note() {
    let root = temp_workspace("task-filter-test-note");

    let out = run_tasks_from_repo(&root, Some("test"), None, false);
    assert_contains_all(
        &out,
        &[
            "Task Matches: test",
            "Built-in Task Matches",
            "built-in fallback supports `<catalog>/test`",
        ],
    );
}

#[test]
fn run_tasks_without_catalogs_still_lists_builtin_tasks() {
    let root = temp_workspace("builtins-only");

    let out = run_tasks_from_repo(&root, None, None, false);
    assert_contains_all(
        &out,
        &[
            "Tasks",
            "help",
            "doctor",
            "test",
            "<catalog>/test fallback",
            "tasks",
        ],
    );
}

#[test]
fn run_tasks_json_renders_machine_readable_payload() {
    let root = temp_workspace("tasks-json");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[tasks.reset-db]\nrun = \"printf farmyard\"\n",
    );

    let out = run_tasks_from_repo(&root, None, None, true);

    let parsed = parse_json_output(&out);
    assert_eq!(parsed["catalog_count"], 2);
    assert!(parsed["catalog_tasks"].is_array());
    assert!(parsed["managed_profiles"].is_array());
    assert!(parsed["builtin_tasks"].is_array());
}

#[test]
fn run_tasks_json_filter_includes_builtin_matches_and_notes() {
    let root = temp_workspace("tasks-json-filter");
    let out = run_tasks_from_repo(&root, Some("test"), None, true);

    let parsed = parse_json_output(&out);
    assert_eq!(parsed["filter"], "test");
    assert!(parsed["builtin_matches"].is_array());
    assert!(parsed["managed_profile_matches"].is_array());
    assert!(parsed["notes"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));
}

#[test]
fn run_tasks_lists_managed_profiles_for_tui_tasks() {
    let root = temp_workspace("tasks-managed-profiles-list");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf api" }]

[tasks.dev.profiles.admin]
concurrent = [{ run = "printf api" }]
"#,
    );

    let out = run_tasks_from_repo(&root, None, None, false);
    assert_contains_all(&out, &["Tasks", "dev admin", "<managed:tui profile:admin>"]);
    assert!(!out.contains("dev default"));
}

#[test]
fn run_tasks_filter_lists_managed_profiles_for_matching_task() {
    let root = temp_workspace("tasks-managed-profiles-filter");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf api" }]

[tasks.dev.profiles.front]
concurrent = [{ run = "printf api" }]
"#,
    );

    let out = run_tasks_from_repo(&root, Some("dev"), None, false);
    assert_contains_all(&out, &["Task Matches: dev", "dev front"]);
    assert!(!out.contains("dev default"));
}

#[test]
fn run_tasks_json_lists_managed_profiles_with_invocation_labels() {
    let root = temp_workspace("tasks-managed-profiles-json-list");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf api" }]

[tasks.dev.profiles.admin]
concurrent = [{ run = "printf api" }]
"#,
    );

    let out = run_tasks_from_repo(&root, None, None, true);

    let parsed = parse_json_output(&out);
    let tasks = parsed["managed_profiles"]
        .as_array()
        .expect("managed_profiles array")
        .iter()
        .filter_map(|row| row["task"].as_str())
        .collect::<Vec<&str>>();
    assert!(tasks.contains(&"dev admin"));
    assert!(!tasks.contains(&"dev default"));
}

#[test]
fn run_tasks_json_filter_lists_managed_profiles_with_invocation_labels() {
    let root = temp_workspace("tasks-managed-profiles-json-filter");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ run = "printf api" }]

[tasks.dev.profiles.front]
concurrent = [{ run = "printf api" }]
"#,
    );

    let out = run_tasks_from_repo(&root, Some("dev"), None, true);

    let parsed = parse_json_output(&out);
    let tasks = parsed["managed_profile_matches"]
        .as_array()
        .expect("managed_profile_matches array")
        .iter()
        .filter_map(|row| row["task"].as_str())
        .collect::<Vec<&str>>();
    assert!(tasks.contains(&"dev front"));
    assert!(!tasks.contains(&"dev default"));
}

#[test]
fn run_manifest_task_prefixed_builtin_tasks_targets_catalog_root_only() {
    let root = temp_workspace("builtin-tasks-prefixed-catalog");
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");

    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.root-only]
run = "printf root"
"#,
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        r#"[catalog]
alias = "farmyard"
[tasks.api]
run = "printf farmyard-api"
"#,
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        r#"[catalog]
alias = "dairy"
[tasks.admin]
run = "printf dairy-admin"
"#,
    );

    let out = run_builtin_ok(root, "farmyard/tasks", &[]);

    assert_contains_all(&out, &["Catalogs", "count: 1", "api"]);
    assert!(!out.contains("admin"));
    assert!(!out.contains("root-only"));
}

#[test]
fn run_manifest_task_relative_prefixed_builtin_tasks_target_catalog_root_only() {
    let root = temp_workspace("builtin-tasks-relative-prefixed-catalog");
    let dairy = root.join("dairy");
    let froyo = root.join("froyo");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    fs::create_dir_all(&froyo).expect("mkdir froyo");

    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.root-only]
run = "printf root"
"#,
    );
    write_manifest(
        &froyo.join("effigy.toml"),
        r#"[catalog]
alias = "froyo"
[tasks.validate]
run = "printf froyo-validate"
"#,
    );

    let out = run_builtin_ok(dairy, "../froyo/tasks", &[]);

    assert_contains_all(&out, &["Catalogs", "count: 1", "validate"]);
    assert!(!out.contains("root-only"));
}
