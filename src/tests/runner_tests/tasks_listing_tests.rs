use super::*;

fn create_workspace_dir(root: &PathBuf, name: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(&dir).expect("mkdir workspace dir");
    dir
}

fn write_root_manifest(root: &PathBuf, body: &str) {
    write_manifest(&root.join("effigy.toml"), body);
}

fn write_root_dev_task_manifest(root: &PathBuf) {
    write_root_manifest(root, "[tasks.dev]\nrun = \"printf root\"\n");
}

fn write_catalog_tasks(dir: &PathBuf, alias: &str, tasks: &[(&str, &str)]) {
    let mut manifest = format!("[catalog]\nalias = \"{}\"\n", alias);
    for (task, run) in tasks {
        manifest.push_str(&format!("[tasks.{}]\nrun = \"{}\"\n", task, run));
    }
    write_manifest(&dir.join("effigy.toml"), &manifest);
}

fn setup_root_and_farmyard_catalog(name: &str) -> PathBuf {
    let root = temp_workspace(name);
    let farmyard = create_workspace_dir(&root, "farmyard");
    write_root_dev_task_manifest(&root);
    write_catalog_tasks(&farmyard, "farmyard", &[("reset-db", "printf farmyard")]);
    root
}

fn setup_root_with_catalogs(name: &str, catalogs: &[(&str, &[(&str, &str)])]) -> PathBuf {
    let root = temp_workspace(name);
    for (dir_name, tasks) in catalogs {
        let dir = create_workspace_dir(&root, dir_name);
        write_catalog_tasks(&dir, dir_name, tasks);
    }
    root
}

fn write_managed_dev_manifest(root: &PathBuf, profile: &str) {
    write_root_manifest(
        root,
        &format!(
            r#"[tasks.dev]
mode = "tui"
concurrent = [{{ run = "printf api" }}]

[tasks.dev.profiles.{}]
concurrent = [{{ run = "printf api" }}]
"#,
            profile
        ),
    );
}

fn assert_builtin_ok_empty(root: PathBuf, task: &str, args: &[&str]) {
    let out = run_builtin_ok(root, task, args);
    assert_eq!(out, "");
}

fn json_task_column(parsed: &serde_json::Value, field: &str) -> Vec<String> {
    parsed[field]
        .as_array()
        .expect("json row array")
        .iter()
        .filter_map(|row| row["task"].as_str())
        .map(|task| task.to_owned())
        .collect::<Vec<_>>()
}

struct ManagedProfileListingCase {
    workspace: &'static str,
    profile: &'static str,
    filter: Option<&'static str>,
    output_json: bool,
    expected_field: &'static str,
}

#[test]
fn run_tasks_lists_catalogs_and_tasks() {
    let root = setup_root_and_farmyard_catalog("list-tasks");

    let out = run_tasks_from_repo(&root, None, None, false);
    assert_contains_all(&out, &["root", "farmyard", "reset-db"]);
}

#[test]
fn run_tasks_supports_compact_task_definitions() {
    let root = temp_workspace("compact-tasks");
    write_root_manifest(
        &root,
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
    write_root_manifest(
        &root,
        r#"[tasks]
api = "printf api"

[tasks.dev]
run = "printf dev"
"#,
    );

    assert_builtin_ok_empty(root.clone(), "api", &[]);

    let tasks = run_tasks_from_repo(&root, None, None, false);
    assert_contains_all(&tasks, &["api", "dev"]);
}

#[test]
fn run_tasks_supports_compact_sequence_task_definitions() {
    let root = temp_workspace("compact-sequence-tasks");
    write_root_manifest(
        &root,
        r#"[tasks]
drop-db = "printf drop-db"
migrate-db = "printf migrate-db"
reset-db = [{ task = "drop-db" }, { task = "migrate-db" }]
"#,
    );

    assert_builtin_ok_empty(root.clone(), "reset-db", &[]);

    let tasks = run_tasks_from_repo(&root, Some("reset-db"), None, false);
    assert_contains_all(&tasks, &["reset-db", "<sequence:2>"]);
}

#[test]
fn run_tasks_with_task_filter_reports_only_matches() {
    let root = setup_root_and_farmyard_catalog("task-filter");

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
    let root = setup_root_and_farmyard_catalog("tasks-json");

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
fn run_tasks_lists_managed_profiles_with_invocation_labels() {
    let cases = [
        ManagedProfileListingCase {
            workspace: "tasks-managed-profiles-list",
            profile: "admin",
            filter: None,
            output_json: false,
            expected_field: "managed_profiles",
        },
        ManagedProfileListingCase {
            workspace: "tasks-managed-profiles-filter",
            profile: "front",
            filter: Some("dev"),
            output_json: false,
            expected_field: "managed_profile_matches",
        },
        ManagedProfileListingCase {
            workspace: "tasks-managed-profiles-json-list",
            profile: "admin",
            filter: None,
            output_json: true,
            expected_field: "managed_profiles",
        },
        ManagedProfileListingCase {
            workspace: "tasks-managed-profiles-json-filter",
            profile: "front",
            filter: Some("dev"),
            output_json: true,
            expected_field: "managed_profile_matches",
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_managed_dev_manifest(&root, case.profile);
        let out = run_tasks_from_repo(&root, case.filter, None, case.output_json);

        if case.output_json {
            let parsed = parse_json_output(&out);
            let tasks = json_task_column(&parsed, case.expected_field);
            assert!(tasks.contains(&format!("dev {}", case.profile)));
            assert!(!tasks.contains(&"dev default".to_owned()));
        } else {
            if case.filter.is_some() {
                assert_contains_all(
                    &out,
                    &["Task Matches: dev", &format!("dev {}", case.profile)],
                );
            } else {
                assert_contains_all(
                    &out,
                    &[
                        "Tasks",
                        &format!("dev {}", case.profile),
                        &format!("<managed:tui profile:{}>", case.profile),
                    ],
                );
            }
            assert!(!out.contains("dev default"));
        }
    }
}

#[test]
fn run_manifest_task_prefixed_builtin_tasks_targets_catalog_root_only() {
    let root = setup_root_with_catalogs(
        "builtin-tasks-prefixed-catalog",
        &[
            ("farmyard", &[("api", "printf farmyard-api")]),
            ("dairy", &[("admin", "printf dairy-admin")]),
        ],
    );
    write_root_manifest(&root, "[tasks.root-only]\nrun = \"printf root\"\n");

    let out = run_builtin_ok(root, "farmyard/tasks", &[]);

    assert_contains_all(&out, &["Catalogs", "count: 1", "api"]);
    assert!(!out.contains("admin"));
    assert!(!out.contains("root-only"));
}

#[test]
fn run_manifest_task_relative_prefixed_builtin_tasks_target_catalog_root_only() {
    let root = setup_root_with_catalogs(
        "builtin-tasks-relative-prefixed-catalog",
        &[("froyo", &[("validate", "printf froyo-validate")])],
    );
    let dairy = create_workspace_dir(&root, "dairy");
    write_root_manifest(&root, "[tasks.root-only]\nrun = \"printf root\"\n");

    let out = run_builtin_ok(dairy, "../froyo/tasks", &[]);

    assert_contains_all(&out, &["Catalogs", "count: 1", "validate"]);
    assert!(!out.contains("root-only"));
}
