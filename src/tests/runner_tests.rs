use super::{
    builtin_test_max_parallel, discover_catalogs, parse_task_runtime_args, parse_task_selector,
    run_doctor, run_manifest_task_with_cwd, run_tasks, RunnerError, TaskRuntimeArgs,
};
use crate::{DoctorArgs, TaskInvocation, TasksArgs};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

#[path = "runner_tests/catalog_discovery_tests.rs"]
mod catalog_discovery_tests;

#[test]
fn run_manifest_task_unknown_prefix_returns_catalog_error() {
    let root = temp_workspace("unknown-prefix");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.reset-db]\nrun = \"printf root\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard/reset-db".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect_err("unknown prefix");

    match err {
        RunnerError::TaskCatalogPrefixNotFound { prefix, available } => {
            assert_eq!(prefix, "farmyard");
            assert_eq!(available, vec!["root".to_owned()]);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_repo_pulse_shows_doctor_migration_message() {
    let root = temp_workspace("repo-pulse-migration-message");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "repo-pulse".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect_err("expected migration guidance");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("no longer a built-in command"));
            assert!(message.contains("effigy doctor"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_health_without_definition_shows_doctor_migration_message() {
    let root = temp_workspace("health-migration-message");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "health".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect_err("expected migration guidance");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("no longer a built-in command"));
            assert!(message.contains("define `tasks.health`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_watch_without_help_requires_owner_policy() {
    let root = temp_workspace("builtin-watch-owner-required-legacy");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "watch".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect_err("expected owner policy requirement");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("--owner <effigy|external>` is required"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_init_creates_scaffold_when_missing() {
    let root = temp_workspace("builtin-init-create");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "init".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("init should create scaffold");
    assert!(out.contains("Created effigy.toml"));

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

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "init".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect_err("init should refuse overwrite");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("already exists"));
            assert!(message.contains("`effigy init --force`"));
        }
        other => panic!("unexpected error: {other}"),
    }

    let existing = fs::read_to_string(root.join("effigy.toml")).expect("read existing");
    assert!(existing.contains("old = \"printf old\""));
}

#[test]
fn run_manifest_task_builtin_init_force_overwrites_existing_manifest() {
    let root = temp_workspace("builtin-init-force-overwrite");
    write_manifest(&root.join("effigy.toml"), "[tasks]\nold = \"printf old\"\n");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "init".to_owned(),
            args: vec!["--force".to_owned()],
        },
        root.clone(),
    )
    .expect("init --force should overwrite");
    assert!(out.contains("Overwrote effigy.toml"));

    let manifest = fs::read_to_string(root.join("effigy.toml")).expect("read overwritten");
    assert!(manifest.contains("ping = \"printf ok\""));
    assert!(!manifest.contains("old = \"printf old\""));
}

#[test]
fn run_manifest_task_builtin_init_dry_run_prints_scaffold_without_writing() {
    let root = temp_workspace("builtin-init-dry-run");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "init".to_owned(),
            args: vec!["--dry-run".to_owned()],
        },
        root.clone(),
    )
    .expect("init --dry-run should render scaffold");

    assert!(out.contains("[tasks]"));
    assert!(out.contains("# [tasks.dev]"));
    assert!(
        !root.join("effigy.toml").exists(),
        "dry-run should not write manifest"
    );
}

#[test]
fn run_manifest_task_builtin_init_json_reports_write_status() {
    let root = temp_workspace("builtin-init-json");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "init".to_owned(),
            args: vec!["--json".to_owned()],
        },
        root.clone(),
    )
    .expect("init --json should succeed");

    assert!(out.contains("\"schema\": \"effigy.init.v1\""));
    assert!(out.contains("\"written\": true"));
    assert!(out.contains("\"dry_run\": false"));
    assert!(out.contains("\"content\":"));
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "migrate".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("migrate preview should succeed");

    assert!(out.contains("Migrate Preview"));
    assert!(out.contains("candidate scripts: 2"));
    assert!(out.contains("+ tasks.build = \"npm run compile\""));
    assert!(out.contains("+ tasks.test = \"vitest run\""));
    assert!(out.contains("No files were modified."));
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "migrate".to_owned(),
            args: vec!["--apply".to_owned()],
        },
        root.clone(),
    )
    .expect("migrate apply should succeed");

    assert!(out.contains("mode: apply"));
    assert!(out.contains("Applied: wrote"));
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

    let _ = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "migrate".to_owned(),
            args: vec!["--apply".to_owned()],
        },
        root.clone(),
    )
    .expect("migrate apply should succeed");

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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "migrate".to_owned(),
            args: vec!["--apply".to_owned()],
        },
        root.clone(),
    )
    .expect("migrate apply with conflicts should succeed");

    assert!(out.contains("Manual Remediation"));
    assert!(out.contains("skip `build` (already defined in `[tasks]`)"));
    assert!(out.contains("+ tasks.lint = \"eslint .\""));
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "migrate".to_owned(),
            args: vec!["--json".to_owned()],
        },
        root,
    )
    .expect("migrate --json should succeed");
    assert!(out.contains("\"schema\": \"effigy.migrate.v1\""));
    assert!(out.contains("\"apply\": false"));
    assert!(out.contains("\"name\": \"test\""));
    assert!(out.contains("\"name\": \"build\""));
}

#[test]
fn run_manifest_task_builtin_watch_help_renders_topic() {
    let root = temp_workspace("builtin-watch-help");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "watch".to_owned(),
            args: vec!["--help".to_owned()],
        },
        root,
    )
    .expect("run watch --help");
    assert!(out.contains("watch Help"));
    assert!(out.contains("--owner <effigy|external>"));
    assert!(out.contains("--debounce-ms <MS>"));
}

#[test]
fn run_manifest_task_builtin_watch_rejects_unknown_args() {
    let root = temp_workspace("builtin-watch-unknown-arg");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "watch".to_owned(),
            args: vec!["--wat".to_owned()],
        },
        root,
    )
    .expect_err("expected watch unknown-arg failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("unknown argument(s) for built-in `watch`: --wat"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_watch_requires_explicit_owner_policy() {
    let root = temp_workspace("builtin-watch-owner-required");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "watch".to_owned(),
            args: vec!["build".to_owned(), "--once".to_owned()],
        },
        root,
    )
    .expect_err("expected owner-policy failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("--owner <effigy|external>` is required"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_watch_external_owner_rejects_nested_loop() {
    let root = temp_workspace("builtin-watch-owner-external");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "watch".to_owned(),
            args: vec![
                "--owner".to_owned(),
                "external".to_owned(),
                "build".to_owned(),
                "--once".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected external-owner failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("watch owner `external`"));
            assert!(message.contains("Run the task directly"));
        }
        other => panic!("unexpected error: {other}"),
    }
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "watch".to_owned(),
            args: vec![
                "--owner".to_owned(),
                "effigy".to_owned(),
                "--once".to_owned(),
                "build".to_owned(),
            ],
        },
        root,
    )
    .expect("watch once should run task");

    assert!(out.contains("watch complete after 1 run(s)."));
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
        run_manifest_task_with_cwd(
            &TaskInvocation {
                name: "watch".to_owned(),
                args: vec![
                    "--owner".to_owned(),
                    "effigy".to_owned(),
                    "--once".to_owned(),
                    "build".to_owned(),
                ],
            },
            root_for_thread,
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

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "watch".to_owned(),
            args: vec![
                "--owner".to_owned(),
                "effigy".to_owned(),
                "--once".to_owned(),
                "build".to_owned(),
            ],
        },
        root.clone(),
    )
    .expect_err("second watch owner should conflict on watch scope lock");

    match err {
        RunnerError::TaskLockConflict {
            scope, remediation, ..
        } => {
            assert_eq!(scope, "task:watch:build");
            assert!(remediation.contains("effigy unlock task:watch:build"));
        }
        other => panic!("unexpected error: {other}"),
    }

    let first = join.join().expect("thread join");
    first.expect("first watch should complete");
}

#[test]
fn run_manifest_task_builtin_init_help_renders_topic() {
    let root = temp_workspace("builtin-init-help");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "init".to_owned(),
            args: vec!["--help".to_owned()],
        },
        root,
    )
    .expect("run init --help");
    assert!(out.contains("init Help"));
    assert!(out.contains("effigy init [--dry-run] [--force] [--json]"));
}

#[test]
fn run_manifest_task_builtin_migrate_help_json_uses_help_schema() {
    let root = temp_workspace("builtin-migrate-help-json");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "migrate".to_owned(),
            args: vec!["--help".to_owned(), "--json".to_owned()],
        },
        root,
    )
    .expect("run migrate --help --json");
    assert!(out.contains("\"schema\": \"effigy.help.v1\""));
    assert!(out.contains("\"topic\": \"migrate\""));
}

#[test]
fn run_manifest_task_builtin_completion_help_renders_topic() {
    let root = temp_workspace("builtin-completion-help");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "completion".to_owned(),
            args: vec!["--help".to_owned()],
        },
        root,
    )
    .expect("run completion --help");
    assert!(out.contains("completion Help"));
    assert!(out.contains("effigy completion <bash|zsh|fish> [--json]"));
}

#[test]
fn run_manifest_task_builtin_completion_bash_outputs_script() {
    let root = temp_workspace("builtin-completion-bash");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "completion".to_owned(),
            args: vec!["bash".to_owned()],
        },
        root,
    )
    .expect("run completion bash");
    assert!(out.contains("complete -F _effigy effigy"));
    assert!(out.contains("cache completion"));
}

#[test]
fn run_manifest_task_builtin_completion_json_uses_completion_schema() {
    let root = temp_workspace("builtin-completion-json");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "completion".to_owned(),
            args: vec!["zsh".to_owned(), "--json".to_owned()],
        },
        root,
    )
    .expect("run completion zsh --json");
    assert!(out.contains("\"schema\": \"effigy.completion.v1\""));
    assert!(out.contains("\"shell\": \"zsh\""));
    assert!(out.contains("\"commands\""));
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard/ping".to_owned(),
            args: vec!["--verbose-root".to_owned()],
        },
        root,
    )
    .expect("run");

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
    let tool = local_bin.join("local-tool");
    fs::write(&tool, "#!/bin/sh\nexit 0\n").expect("write local tool");
    let mut perms = fs::metadata(&tool).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&tool, perms).expect("chmod");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "local".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("run local tool");

    assert_eq!(out, "");
}

#[path = "runner_tests/run_array_tests.rs"]
mod run_array_tests;

#[path = "runner_tests/tasks_listing_tests.rs"]
mod tasks_listing_tests;

#[path = "runner_tests/builtin_command_tests.rs"]
mod builtin_command_tests;

#[path = "runner_tests/catalogs_builtin_tests.rs"]
mod catalogs_builtin_tests;

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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard/help".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("prefixed builtin help");

    assert!(out.contains("Commands"));
    assert!(out.contains("effigy help"));
}

#[path = "runner_tests/tasks_and_doctor_command_tests.rs"]
mod tasks_and_doctor_command_tests;

#[path = "runner_tests/config_builtin_tests.rs"]
mod config_builtin_tests;

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

#[cfg(unix)]
#[path = "runner_tests/doctor_text_output_tests.rs"]
mod doctor_text_output_tests;

#[path = "runner_tests/deferral_tests.rs"]
mod deferral_tests;

#[path = "runner_tests/managed_and_locking_tests.rs"]
mod managed_and_locking_tests;

fn write_manifest(path: &PathBuf, body: &str) {
    fs::write(path, body).expect("write manifest");
}

fn temp_dir(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("effigy-runner-{name}-{ts}"))
}

fn temp_workspace(name: &str) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}

fn with_cwd<F, T>(cwd: &PathBuf, f: F) -> T
where
    F: FnOnce() -> T,
{
    let _guard = lock_test();
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(cwd).expect("set cwd");
    let out = f();
    std::env::set_current_dir(original).expect("restore cwd");
    out
}

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_test() -> MutexGuard<'static, ()> {
    match test_lock().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

struct EnvGuard {
    original: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    fn set_many(entries: &[(&str, Option<String>)]) -> Self {
        let mut original = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            original.push(((*key).to_owned(), std::env::var(key).ok()));
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        Self { original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.original {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}
