use super::prelude::*;

#[test]
fn parse_task_runtime_args_extracts_repo_verbose_and_passthrough() {
    let args = vec![
        "--repo".to_owned(),
        "/tmp/x".to_owned(),
        "--verbose-root".to_owned(),
        "--flag".to_owned(),
        "abc".to_owned(),
    ];
    let parsed = parse_task_runtime_args(&args).expect("parse");
    assert_eq!(
        parsed,
        TaskRuntimeArgs {
            repo_override: Some(PathBuf::from("/tmp/x")),
            verbose_root: true,
            passthrough: vec!["--flag".to_owned(), "abc".to_owned()],
        }
    );
}

#[test]
fn parse_task_selector_supports_prefixed_task() {
    let selector = parse_task_selector("farmyard/reset-db").expect("selector");
    assert_eq!(selector.prefix, Some("farmyard".to_owned()));
    assert_eq!(selector.task_name, "reset-db");
}

#[test]
fn parse_task_selector_supports_relative_prefixed_task() {
    let selector = parse_task_selector("../froyo/validate").expect("selector");
    assert_eq!(selector.prefix, Some("../froyo".to_owned()));
    assert_eq!(selector.task_name, "validate");
}

#[test]
fn run_manifest_task_unknown_prefix_returns_catalog_error() {
    let root = temp_workspace("unknown-prefix");
    write_root_manifest(&root, "[tasks.reset-db]\nrun = \"printf root\"\n");

    let err = run_task(&root, "farmyard/reset-db", &[]).expect_err("unknown prefix");
    assert_catalog_prefix_not_found(err, "farmyard", &["root"]);
}

#[test]
fn run_manifest_task_removed_builtins_show_migration_message() {
    let cases = [
        BuiltinErrorCase {
            workspace: "repo-pulse-migration-message",
            command: "repo-pulse",
            args: &[],
            manifest: "[tasks.build]\nrun = \"printf ok\"\n",
            expected: &["no longer a built-in command", "effigy doctor"],
        },
        BuiltinErrorCase {
            workspace: "health-migration-message",
            command: "health",
            args: &[],
            manifest: "[tasks.build]\nrun = \"printf ok\"\n",
            expected: &["no longer a built-in command", "define `tasks.health`"],
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_root_manifest(&root, case.manifest);
        let err = run_builtin_err(root, case.command, case.args);
        assert_task_invocation_error_contains(err, case.expected);
    }
}

#[test]
fn run_manifest_task_verbose_root_includes_resolution_trace() {
    let _guard = lock_test();
    let _env = EnvGuard::set_many(&[("EFFIGY_COLOR", None), ("NO_COLOR", None)]);
    let root = temp_workspace("verbose-trace");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_root_manifest(&root, "[tasks.ping]\nrun = \"printf root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf farmyard\"\n",
    );

    let out = run_task(&root, "farmyard/ping", &["--verbose-root"]).expect("run");

    assert!(out.contains("Task Resolution"));
    assert!(out.contains("catalog-alias: farmyard"));
    assert!(out.contains("farmyard"));
}

#[test]
fn run_manifest_task_includes_local_node_modules_bin_in_path() {
    let root = temp_workspace("local-node-bin-path");
    write_root_manifest(&root, "[tasks.local]\nrun = \"local-tool\"\n");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    write_executable(&local_bin.join("local-tool"), "#!/bin/sh\nexit 0\n");

    assert_run_task_ok_empty(&root, "local", &[]);
}

#[test]
fn run_manifest_task_prefixed_builtin_help_is_supported() {
    let root = temp_workspace("builtin-help-prefixed-catalog");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &farmyard.join("effigy.toml"),
        r#"[catalog]
alias = "farmyard"
"#,
    );

    let out = run_builtin_ok(root, "farmyard/help", &[]);
    assert_contains_all(&out, &["Commands", "effigy help"]);
}

#[test]
fn builtin_test_max_parallel_reads_root_manifest_config() {
    let root = temp_workspace("builtin-test-max-parallel-config");
    write_root_manifest(
        &root,
        r#"[test]
max_parallel = 1
"#,
    );
    let catalogs = discover_catalogs(&root).expect("discover catalogs");
    assert_eq!(builtin_test_max_parallel(&catalogs, &root), 1);
}

#[test]
fn builtin_test_max_parallel_falls_back_when_invalid_or_missing() {
    let root = temp_workspace("builtin-test-max-parallel-default");
    write_root_manifest(
        &root,
        r#"[test]
max_parallel = 0
"#,
    );
    let catalogs = discover_catalogs(&root).expect("discover catalogs");
    assert_eq!(builtin_test_max_parallel(&catalogs, &root), 3);
}
