use super::*;

fn run_task(root: &PathBuf, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root.clone(),
    )
}

fn assert_catalog_prefix_not_found(
    err: RunnerError,
    expected_prefix: &str,
    expected_available: &[&str],
) {
    match err {
        RunnerError::TaskCatalogPrefixNotFound { prefix, available } => {
            assert_eq!(prefix, expected_prefix);
            assert_eq!(
                available,
                expected_available
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>()
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

fn assert_lock_conflict(err: RunnerError, expected_scope: &str, expected_remediation: &str) {
    match err {
        RunnerError::TaskLockConflict {
            scope, remediation, ..
        } => {
            assert_eq!(scope, expected_scope);
            assert!(remediation.contains(expected_remediation));
        }
        other => panic!("unexpected error: {other}"),
    }
}

fn write_executable(path: &PathBuf, script: &str) {
    fs::write(path, script).expect("write executable");
    let mut perms = fs::metadata(path).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod");
}

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
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.reset-db]\nrun = \"printf root\"\n",
    );

    let err = run_task(&root, "farmyard/reset-db", &[]).expect_err("unknown prefix");
    assert_catalog_prefix_not_found(err, "farmyard", &["root"]);
}

#[test]
fn run_manifest_task_repo_pulse_shows_doctor_migration_message() {
    let root = temp_workspace("repo-pulse-migration-message");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let err = run_builtin_err(root, "repo-pulse", &[]);
    assert_task_invocation_error_contains(err, &["no longer a built-in command", "effigy doctor"]);
}

#[test]
fn run_manifest_task_health_without_definition_shows_doctor_migration_message() {
    let root = temp_workspace("health-migration-message");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let err = run_builtin_err(root, "health", &[]);
    assert_task_invocation_error_contains(
        err,
        &["no longer a built-in command", "define `tasks.health`"],
    );
}

#[test]
fn run_manifest_task_builtin_watch_without_help_requires_owner_policy() {
    let root = temp_workspace("builtin-watch-owner-required-legacy");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_builtin_err(root, "watch", &[]);
    assert_task_invocation_error_contains(err, &["--owner <effigy|external>` is required"]);
}

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
    write_manifest(&root.join("effigy.toml"), "[tasks]\nold = \"printf old\"\n");

    let err = run_builtin_err(root.clone(), "init", &[]);
    assert_task_invocation_error_contains(err, &["already exists", "`effigy init --force`"]);

    let existing = fs::read_to_string(root.join("effigy.toml")).expect("read existing");
    assert!(existing.contains("old = \"printf old\""));
}

#[test]
fn run_manifest_task_builtin_init_force_overwrites_existing_manifest() {
    let root = temp_workspace("builtin-init-force-overwrite");
    write_manifest(&root.join("effigy.toml"), "[tasks]\nold = \"printf old\"\n");

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
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "build": "npm run compile",
    "test": "vitest run"
  }
}
"#,
    )
    .expect("write package scripts");

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
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "build": "npm run compile",
    "test": "vitest run"
  }
}
"#,
    )
    .expect("write package scripts");

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
    let source = r#"{
  "scripts": {
    "build": "npm run compile"
  }
}
"#;
    fs::write(root.join("package.json"), source).expect("write package scripts");

    let _ = run_builtin_ok(root.clone(), "migrate", &["--apply"]);

    let package_after = fs::read_to_string(root.join("package.json")).expect("read package");
    assert_eq!(package_after, source, "migration must be non-destructive");
}

#[test]
fn run_manifest_task_builtin_migrate_conflicts_require_manual_remediation() {
    let root = temp_workspace("builtin-migrate-conflicts");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks]\nbuild = \"printf old\"\n",
    );
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "build": "npm run compile",
    "lint": "eslint ."
  }
}
"#,
    )
    .expect("write package scripts");

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
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks]\nbuild = \"printf old\"\n",
    );
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "build": "npm run compile",
    "test": "vitest run"
  }
}
"#,
    )
    .expect("write package scripts");

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
fn run_manifest_task_builtin_watch_help_renders_topic() {
    let root = temp_workspace("builtin-watch-help");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_builtin_ok(root, "watch", &["--help"]);
    assert_contains_all(
        &out,
        &[
            "watch Help",
            "--owner <effigy|external>",
            "--debounce-ms <MS>",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_watch_rejects_unknown_args() {
    let root = temp_workspace("builtin-watch-unknown-arg");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_builtin_err(root, "watch", &["--wat"]);
    assert_task_invocation_error_contains(
        err,
        &["unknown argument(s) for built-in `watch`: --wat"],
    );
}

#[test]
fn run_manifest_task_builtin_watch_requires_explicit_owner_policy() {
    let root = temp_workspace("builtin-watch-owner-required");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let err = run_builtin_err(root, "watch", &["build", "--once"]);
    assert_task_invocation_error_contains(err, &["--owner <effigy|external>` is required"]);
}

#[test]
fn run_manifest_task_builtin_watch_external_owner_rejects_nested_loop() {
    let root = temp_workspace("builtin-watch-owner-external");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let err = run_builtin_err(root, "watch", &["--owner", "external", "build", "--once"]);
    assert_task_invocation_error_contains(
        err,
        &["watch owner `external`", "Run the task directly"],
    );
}

#[test]
fn run_manifest_task_builtin_watch_once_executes_target_task() {
    let root = temp_workspace("builtin-watch-once-exec");
    let marker = root.join("watch-once.log");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            "[tasks.build]\nrun = \"printf watched > '{}'\"\n",
            marker.display()
        ),
    );

    let out = run_builtin_ok(root, "watch", &["--owner", "effigy", "--once", "build"]);
    assert_contains_all(&out, &["watch complete after 1 run(s)."]);
    assert!(marker.exists(), "watch --once should execute the target");
}

#[test]
fn run_manifest_task_builtin_watch_rejects_concurrent_watch_owner_for_same_target() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-watch-lock-conflict");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"sleep 2\"\n",
    );

    let root_for_thread = root.clone();
    let join = thread::spawn(move || {
        run_task(
            &root_for_thread,
            "watch",
            &["--owner", "effigy", "--once", "build"],
        )
    });

    let watch_lock = root.join(".effigy/locks/task-watch-build.lock");
    let started = Instant::now();
    while !watch_lock.exists() {
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "watch lock was not created in time"
        );
        thread::sleep(Duration::from_millis(20));
    }

    let err = run_task(&root, "watch", &["--owner", "effigy", "--once", "build"])
        .expect_err("second watch owner should conflict on watch scope lock");
    assert_lock_conflict(err, "task:watch:build", "effigy unlock task:watch:build");

    let first = join.join().expect("thread join");
    first.expect("first watch should complete");
}

#[test]
fn run_manifest_task_builtin_init_help_renders_topic() {
    let root = temp_workspace("builtin-init-help");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_builtin_ok(root, "init", &["--help"]);
    assert_contains_all(
        &out,
        &["init Help", "effigy init [--dry-run] [--force] [--json]"],
    );
}

#[test]
fn run_manifest_task_builtin_migrate_help_json_uses_help_schema() {
    let root = temp_workspace("builtin-migrate-help-json");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_builtin_ok(root, "migrate", &["--help", "--json"]);
    assert_contains_all(
        &out,
        &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"migrate\""],
    );
}

#[test]
fn run_manifest_task_builtin_completion_help_renders_topic() {
    let root = temp_workspace("builtin-completion-help");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_builtin_ok(root, "completion", &["--help"]);
    assert_contains_all(
        &out,
        &[
            "completion Help",
            "effigy completion <bash|zsh|fish> [--json]",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_completion_bash_outputs_script() {
    let root = temp_workspace("builtin-completion-bash");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_builtin_ok(root, "completion", &["bash"]);
    assert_contains_all(&out, &["complete -F _effigy effigy", "cache completion"]);
}

#[test]
fn run_manifest_task_builtin_completion_json_uses_completion_schema() {
    let root = temp_workspace("builtin-completion-json");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_builtin_ok(root, "completion", &["zsh", "--json"]);
    assert_contains_all(
        &out,
        &[
            "\"schema\": \"effigy.completion.v1\"",
            "\"shell\": \"zsh\"",
            "\"commands\"",
        ],
    );
}

#[test]
fn run_manifest_task_verbose_root_includes_resolution_trace() {
    let _guard = lock_test();
    let _env = EnvGuard::set_many(&[("EFFIGY_COLOR", None), ("NO_COLOR", None)]);
    let root = temp_workspace("verbose-trace");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf root\"\n",
    );
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
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.local]\nrun = \"local-tool\"\n",
    );
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    write_executable(&local_bin.join("local-tool"), "#!/bin/sh\nexit 0\n");

    let out = run_task(&root, "local", &[]).expect("run local tool");

    assert_eq!(out, "");
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
    write_manifest(
        &root.join("effigy.toml"),
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
    write_manifest(
        &root.join("effigy.toml"),
        r#"[test]
max_parallel = 0
"#,
    );
    let catalogs = discover_catalogs(&root).expect("discover catalogs");
    assert_eq!(builtin_test_max_parallel(&catalogs, &root), 3);
}
