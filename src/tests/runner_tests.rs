use super::{
    builtin_test_max_parallel, discover_catalogs, parse_task_runtime_args, parse_task_selector,
    run_manifest_task_with_cwd, run_pulse, run_tasks, RunnerError, TaskRuntimeArgs,
};
use crate::{PulseArgs, TaskInvocation, TasksArgs};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

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
fn run_manifest_task_prefixed_uses_named_catalog() {
    let root = temp_workspace("prefixed");
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
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("run");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_unprefixed_prefers_nearest_catalog_in_scope() {
    let root = temp_workspace("nearest");
    let farmyard = root.join("farmyard");
    let nested = farmyard.join("crates/api");
    fs::create_dir_all(&nested).expect("mkdir");

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
            name: "ping".to_owned(),
            args: Vec::new(),
        },
        nested,
    )
    .expect("run");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_unprefixed_reports_ambiguity_on_equal_shallow_depth() {
    let root = temp_workspace("ambiguous");
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");

    write_manifest(
        &farmyard.join("effigy.toml"),
        "[tasks.reset-db]\nrun = \"printf farmyard\"\n",
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        "[tasks.reset-db]\nrun = \"printf dairy\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "reset-db".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect_err("expected ambiguity");

    match err {
        RunnerError::TaskAmbiguous { name, candidates } => {
            assert_eq!(name, "reset-db");
            assert_eq!(candidates.len(), 2);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_relative_prefix_resolves_catalog_by_path() {
    let root = temp_workspace("relative-prefix-path");
    let dairy = root.join("dairy");
    let froyo = root.join("froyo");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    fs::create_dir_all(&froyo).expect("mkdir froyo");

    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.dev]\nrun = \"printf dairy\"\n",
    );
    write_manifest(
        &froyo.join("effigy.toml"),
        "[catalog]\nalias = \"froyo\"\n[tasks.validate]\nrun = \"printf froyo-validate\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "../froyo/validate".to_owned(),
            args: Vec::new(),
        },
        dairy,
    )
    .expect("relative path task should resolve");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_relative_prefix_prefers_alias_collision_over_path_resolution() {
    let root = temp_workspace("relative-prefix-alias-collision");
    let dairy = root.join("dairy");
    let alias_override = root.join("alias-override");
    let froyo = root.join("froyo");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    fs::create_dir_all(&alias_override).expect("mkdir alias-override");
    fs::create_dir_all(&froyo).expect("mkdir froyo");

    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.dev]\nrun = \"printf dairy\"\n",
    );
    write_manifest(
        &alias_override.join("effigy.toml"),
        "[catalog]\nalias = \"../froyo\"\n[tasks.validate]\nrun = \"printf alias\"\n",
    );
    write_manifest(
        &froyo.join("effigy.toml"),
        "[catalog]\nalias = \"froyo\"\n[tasks.validate]\nrun = \"printf froyo\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "../froyo/validate".to_owned(),
            args: vec!["--verbose-root".to_owned()],
        },
        dairy,
    )
    .expect("relative prefix should resolve via alias first");

    assert!(out.contains("catalog-alias: ../froyo"));
    assert!(out.contains("selected catalog via explicit prefix `../froyo`"));
}

#[test]
fn run_manifest_task_relative_prefix_supports_multi_parent_traversal() {
    let root = temp_workspace("relative-prefix-multi-parent");
    let app = root.join("apps/web/src");
    let shared = root.join("shared");
    fs::create_dir_all(&app).expect("mkdir app");
    fs::create_dir_all(&shared).expect("mkdir shared");

    write_manifest(
        &shared.join("effigy.toml"),
        "[catalog]\nalias = \"shared\"\n[tasks.lint]\nrun = \"printf shared-lint\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "../../../shared/lint".to_owned(),
            args: vec!["--verbose-root".to_owned()],
        },
        app,
    )
    .expect("multi-parent relative task should resolve");

    assert!(out.contains("catalog-alias: shared"));
    assert!(out.contains("relative prefix `../../../shared` -> `shared`"));
}

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
fn run_manifest_task_verbose_root_includes_resolution_trace() {
    let _guard = test_lock().lock().expect("lock");
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

#[test]
fn run_manifest_task_run_array_supports_task_reference_steps() {
    let root = temp_workspace("run-array-task-refs");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.lint]
run = "printf lint-ok"

[tasks.validate]
run = [{ task = "lint" }, "printf validate-ok"]
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".to_owned(),
            args: vec!["--verbose-root".to_owned()],
        },
        root,
    )
    .expect("run");

    assert!(out.contains("printf lint-ok"));
    assert!(out.contains("printf validate-ok"));
}

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

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
        })
    })
    .expect("run tasks");

    assert!(out.contains("root"));
    assert!(out.contains("farmyard"));
    assert!(out.contains("reset-db"));
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

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
        })
    })
    .expect("run tasks");

    assert!(out.contains("api"));
    assert!(out.contains("jobs"));
    assert!(out.contains("printf api"));
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "api".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("run compact task");
    assert_eq!(out, "");

    let tasks = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
        })
    })
    .expect("run tasks");
    assert!(tasks.contains("api"));
    assert!(tasks.contains("dev"));
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "reset-db".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("run compact sequence task");
    assert_eq!(out, "");

    let tasks = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("reset-db".to_owned()),
        })
    })
    .expect("run tasks");
    assert!(tasks.contains("reset-db"));
    assert!(tasks.contains("<sequence:2>"));
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

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("reset-db".to_owned()),
        })
    })
    .expect("run tasks");

    assert!(out.contains("Task Matches: reset-db"));
    assert!(out.contains("farmyard"));
    assert!(out.contains("reset-db"));
    assert!(!out.contains("root      │ reset-db"));
}

#[test]
fn run_tasks_with_test_filter_shows_catalog_fallback_note() {
    let root = temp_workspace("task-filter-test-note");

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("test".to_owned()),
        })
    })
    .expect("run tasks");

    assert!(out.contains("Task Matches: test"));
    assert!(out.contains("Built-in Task Matches"));
    assert!(out.contains("built-in fallback supports `<catalog>/test`"));
}

#[test]
fn run_tasks_without_catalogs_still_lists_builtin_tasks() {
    let root = temp_workspace("builtins-only");

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
        })
    })
    .expect("run tasks");

    assert!(out.contains("Built-in Tasks"));
    assert!(out.contains("help"));
    assert!(out.contains("health"));
    assert!(out.contains("repo-pulse"));
    assert!(out.contains("test"));
    assert!(out.contains("<catalog>/test fallback"));
    assert!(out.contains("tasks"));
}

#[test]
fn run_manifest_task_builtin_test_plan_renders_detection_summary() {
    let root = temp_workspace("builtin-test-plan");
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--plan".to_owned()],
        },
        root,
    )
    .expect("run test --plan");

    assert!(out.contains("Test Plan"));
    assert!(out.contains("targets:"));
    assert!(out.contains("runtime:"));
    assert!(out.contains("text"));
    assert!(out.contains("Target: root"));
    assert!(out.contains("runner:"));
    assert!(out.contains("available-suites:"));
    assert!(out.contains("vitest"));
    assert!(out.contains("fallback-chain"));
}

#[test]
fn run_manifest_task_builtin_test_executes_local_vitest() {
    let root = temp_workspace("builtin-test-exec-vitest");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    let marker = root.join("vitest-called.log");
    fs::write(
        &vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            marker.display()
        ),
    )
    .expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--run".to_owned()],
        },
        root.clone(),
    )
    .expect("run builtin test");

    assert!(out.contains("Test Results"));
    assert!(out.contains("targets:"));
    assert!(out.contains("root"));
    assert!(!out.contains("runner:vitest"));
    assert!(!out.contains("command:"));
    assert!(marker.exists(), "vitest stub should be invoked");
}

#[test]
fn run_manifest_task_builtin_test_executes_js_and_rust_suites_in_same_repo() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("builtin-test-multi-context");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/lib.rs"),
        "pub fn ok() -> bool { true }\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn smoke() {\n        assert!(super::ok());\n    }\n}\n",
    )
    .expect("write lib");

    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    let vitest_marker = root.join("vitest-called.log");
    fs::write(
        &vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            vitest_marker.display()
        ),
    )
    .expect("write vitest");
    let mut vitest_perms = fs::metadata(&vitest).expect("stat").permissions();
    vitest_perms.set_mode(0o755);
    fs::set_permissions(&vitest, vitest_perms).expect("chmod");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("run builtin multi-context test");

    assert!(out.contains("Test Results"));
    assert!(out.contains("root/vitest"));
    assert!(out.contains("root/cargo-"));
    assert!(vitest_marker.exists(), "vitest suite should run");
}

#[test]
fn run_manifest_task_builtin_test_with_named_args_errors_when_multi_suite_is_ambiguous() {
    let root = temp_workspace("builtin-test-multi-suite-ambiguous");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["user-service".to_owned()],
        },
        root,
    )
    .expect_err("expected ambiguity error");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("ambiguous"));
            assert!(message.contains("vitest"));
            assert!(message.contains("cargo-"));
            assert!(message.contains("Try one of:"));
            assert!(message.contains("effigy test vitest user-service"));
            assert!(message.contains("effigy test cargo-"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_test_plan_with_named_args_in_multi_suite_returns_recovery_output() {
    let root = temp_workspace("builtin-test-multi-suite-plan-recovery");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--plan".to_owned(), "user-service".to_owned()],
        },
        root,
    )
    .expect("plan should return recovery output");

    assert!(out.contains("Test Plan"));
    assert!(out.contains("runtime: plan-recovery"));
    assert!(out.contains("available-suites:"));
    assert!(out.contains("ambiguous"));
    assert!(out.contains("Try one of:"));
}

#[test]
fn run_manifest_task_builtin_test_supports_positional_suite_selector() {
    let root = temp_workspace("builtin-test-suite-selector");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    let vitest_marker = root.join("vitest-called.log");
    fs::write(
        &vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            vitest_marker.display()
        ),
    )
    .expect("write vitest");
    let mut vitest_perms = fs::metadata(&vitest).expect("stat").permissions();
    vitest_perms.set_mode(0o755);
    fs::set_permissions(&vitest, vitest_perms).expect("chmod");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["vitest".to_owned(), "user-service".to_owned()],
        },
        root.clone(),
    )
    .expect("run builtin suite-selected test");

    assert!(out.contains("Test Results"));
    assert!(out.contains("root/vitest"));
    assert!(!out.contains("root/cargo-"));
    assert!(vitest_marker.exists(), "vitest suite should run");
}

#[test]
fn run_manifest_task_builtin_test_plan_mistyped_suite_returns_recovery_output() {
    let root = temp_workspace("builtin-test-plan-mistyped-suite-recovery");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec![
                "--plan".to_owned(),
                "viteest".to_owned(),
                "user-service".to_owned(),
            ],
        },
        root,
    )
    .expect("plan should return typo recovery output");

    assert!(out.contains("Test Plan"));
    assert!(out.contains("runtime: plan-recovery"));
    assert!(out.contains("Did you mean `vitest`?"));
    assert!(out.contains("Try: effigy test vitest user-service"));
}

#[test]
fn run_manifest_task_builtin_test_errors_for_unavailable_positional_suite_selector() {
    let root = temp_workspace("builtin-test-suite-selector-unavailable");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["nextest".to_owned()],
        },
        root,
    )
    .expect_err("suite should be unavailable");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("not available"));
            assert!(message.contains("nextest"));
            assert!(message.contains("vitest"));
            assert!(message.contains("Try one of:"));
            assert!(message.contains("effigy test vitest"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_test_mistyped_suite_suggests_nearest_runner() {
    let root = temp_workspace("builtin-test-mistyped-suite-suggestion");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "test": "vitest run"
  }
}"#,
    )
    .expect("write package");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"multi\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo toml");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["viteest".to_owned(), "user-service".to_owned()],
        },
        root,
    )
    .expect_err("expected mistyped suite error");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("runner `viteest` is not available"));
            assert!(message.contains("Did you mean `vitest`?"));
            assert!(message.contains("Try: effigy test vitest user-service"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_explicit_test_task_overrides_builtin_auto_detection() {
    let root = temp_workspace("builtin-test-explicit-override");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.test]\nrun = \"printf explicit > explicit-test.log\"\n",
    );
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("run explicit test task");

    assert_eq!(out, "");
    assert!(
        root.join("explicit-test.log").exists(),
        "explicit task should run before builtin test detection"
    );
}

#[test]
fn run_manifest_task_builtin_test_falls_through_to_deferral_when_no_detection_matches() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("builtin-test-defers");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"test {request} = 'test' && test {args} = '--watch'\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--watch".to_owned()],
        },
        root,
    )
    .expect("builtin test should defer when detection is unavailable");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_builtin_test_fans_out_across_catalog_roots() {
    let root = temp_workspace("builtin-test-fanout");
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );

    fs::write(
        farmyard.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write farmyard package");
    fs::write(
        dairy.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write dairy package");

    let farmyard_bin = farmyard.join("node_modules/.bin");
    fs::create_dir_all(&farmyard_bin).expect("mkdir farmyard bin");
    let dairy_bin = dairy.join("node_modules/.bin");
    fs::create_dir_all(&dairy_bin).expect("mkdir dairy bin");
    let farmyard_marker = farmyard.join("vitest-called.log");
    let dairy_marker = dairy.join("vitest-called.log");

    let farmyard_vitest = farmyard_bin.join("vitest");
    fs::write(
        &farmyard_vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            farmyard_marker.display()
        ),
    )
    .expect("write farmyard vitest");
    let mut farmyard_perms = fs::metadata(&farmyard_vitest).expect("stat").permissions();
    farmyard_perms.set_mode(0o755);
    fs::set_permissions(&farmyard_vitest, farmyard_perms).expect("chmod");

    let dairy_vitest = dairy_bin.join("vitest");
    fs::write(
        &dairy_vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            dairy_marker.display()
        ),
    )
    .expect("write dairy vitest");
    let mut dairy_perms = fs::metadata(&dairy_vitest).expect("stat").permissions();
    dairy_perms.set_mode(0o755);
    fs::set_permissions(&dairy_vitest, dairy_perms).expect("chmod");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("builtin test fanout");

    assert!(out.contains("Test Results"));
    assert!(out.contains("targets:"));
    assert!(out.contains("dairy"));
    assert!(out.contains("farmyard"));
    assert!(!out.contains("runner:vitest"));
    assert!(!out.contains("command:"));
    assert!(farmyard_marker.exists(), "farmyard vitest should run");
    assert!(dairy_marker.exists(), "dairy vitest should run");
}

#[test]
fn run_manifest_task_prefixed_builtin_test_targets_catalog_root_only() {
    let root = temp_workspace("builtin-test-prefixed-catalog");
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );

    fs::write(
        farmyard.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write farmyard package");
    fs::write(
        dairy.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write dairy package");

    let farmyard_bin = farmyard.join("node_modules/.bin");
    fs::create_dir_all(&farmyard_bin).expect("mkdir farmyard bin");
    let dairy_bin = dairy.join("node_modules/.bin");
    fs::create_dir_all(&dairy_bin).expect("mkdir dairy bin");
    let farmyard_marker = farmyard.join("vitest-called.log");
    let dairy_marker = dairy.join("vitest-called.log");

    let farmyard_vitest = farmyard_bin.join("vitest");
    fs::write(
        &farmyard_vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            farmyard_marker.display()
        ),
    )
    .expect("write farmyard vitest");
    let mut farmyard_perms = fs::metadata(&farmyard_vitest).expect("stat").permissions();
    farmyard_perms.set_mode(0o755);
    fs::set_permissions(&farmyard_vitest, farmyard_perms).expect("chmod");

    let dairy_vitest = dairy_bin.join("vitest");
    fs::write(
        &dairy_vitest,
        format!(
            "#!/bin/sh\nprintf called > \"{}\"\nexit 0\n",
            dairy_marker.display()
        ),
    )
    .expect("write dairy vitest");
    let mut dairy_perms = fs::metadata(&dairy_vitest).expect("stat").permissions();
    dairy_perms.set_mode(0o755);
    fs::set_permissions(&dairy_vitest, dairy_perms).expect("chmod");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard/test".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("prefixed builtin test");

    assert!(out.contains("Test Results"));
    assert!(out.contains("farmyard"));
    assert!(!out.contains("dairy"));
    assert!(farmyard_marker.exists(), "farmyard vitest should run");
    assert!(!dairy_marker.exists(), "dairy vitest should not run");
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard/tasks".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("prefixed builtin tasks");

    assert!(out.contains("Task Catalogs"));
    assert!(out.contains("catalogs: 1"));
    assert!(out.contains("api"));
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "../froyo/tasks".to_owned(),
            args: Vec::new(),
        },
        dairy,
    )
    .expect("relative prefixed builtin tasks");

    assert!(out.contains("Task Catalogs"));
    assert!(out.contains("catalogs: 1"));
    assert!(out.contains("validate"));
    assert!(!out.contains("root-only"));
}

#[test]
fn run_manifest_task_builtin_catalogs_renders_diagnostics_and_resolution_probe() {
    let root = temp_workspace("builtin-catalogs");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");

    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec!["--resolve".to_owned(), "farmyard/api".to_owned()],
        },
        root,
    )
    .expect("builtin catalogs");

    assert!(out.contains("Catalog Diagnostics"));
    assert!(out.contains("Routing Precedence"));
    assert!(out.contains("Resolution Probe: farmyard/api"));
    assert!(out.contains("catalog: farmyard"));
}

#[test]
fn run_manifest_task_builtin_catalogs_json_renders_probe_payload() {
    let root = temp_workspace("builtin-catalogs-json");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");

    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--resolve".to_owned(),
                "farmyard/api".to_owned(),
            ],
        },
        root,
    )
    .expect("builtin catalogs json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json parse");
    assert!(parsed["catalogs"].is_array());
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["catalog"], "farmyard");
    assert_eq!(parsed["resolve"]["task"], "api");
    assert!(parsed["precedence"].is_array());
}

#[test]
fn run_manifest_task_builtin_catalogs_json_reports_resolution_errors() {
    let root = temp_workspace("builtin-catalogs-json-error");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--resolve".to_owned(),
                "farmyard/api".to_owned(),
            ],
        },
        root,
    )
    .expect("builtin catalogs json error");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json parse");
    assert_eq!(parsed["resolve"]["status"], "error");
    assert_eq!(parsed["resolve"]["catalog"], serde_json::Value::Null);
    assert!(parsed["resolve"]["error"]
        .as_str()
        .is_some_and(|msg| msg.contains("prefix `farmyard` not found")));
}

#[test]
fn run_manifest_task_builtin_catalogs_json_compact_output_has_no_newlines() {
    let root = temp_workspace("builtin-catalogs-json-compact");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--pretty".to_owned(),
                "false".to_owned(),
                "--resolve".to_owned(),
                "farmyard/api".to_owned(),
            ],
        },
        root,
    )
    .expect("builtin catalogs compact json");

    assert!(!out.contains('\n'));
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("json parse");
    assert_eq!(parsed["resolve"]["status"], "ok");
}

#[test]
fn run_manifest_task_builtin_catalogs_pretty_requires_json() {
    let root = temp_workspace("builtin-catalogs-pretty-requires-json");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec!["--pretty".to_owned(), "false".to_owned()],
        },
        root,
    )
    .expect_err("expected --pretty requires --json");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--pretty` is only supported together with `--json`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_catalogs_rejects_invalid_pretty_value() {
    let root = temp_workspace("builtin-catalogs-invalid-pretty");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--pretty".to_owned(),
                "nope".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected invalid --pretty value");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("value `nope` is invalid"));
        }
        other => panic!("unexpected error: {other}"),
    }
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

#[test]
fn run_tasks_rejects_legacy_builtin_config_group() {
    let root = temp_workspace("reject-legacy-builtin-group");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[builtin.test]
max_parallel = 2
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root.clone()),
        task_name: None,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error.to_string().contains("unknown field `builtin`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_test_config_field() {
    let root = temp_workspace("reject-unknown-test-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[test]
max_parallels = 2
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error.to_string().contains("unknown field `max_parallels`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_package_manager_field() {
    let root = temp_workspace("reject-unknown-package-manager-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[package_manager]
jss = "pnpm"
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error.to_string().contains("unknown field `jss`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_test_runner_override_field() {
    let root = temp_workspace("reject-unknown-test-runner-override-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[test.runners.vitest]
cmd = "vitest run"
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error.to_string().contains("unknown field `cmd`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_task_field() {
    let root = temp_workspace("reject-unknown-task-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
run = "printf dev"
fial_on_non_zero = true
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error
                .to_string()
                .contains("unknown field `fial_on_non_zero`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_process_field() {
    let root = temp_workspace("reject-unknown-process-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "managed"
[tasks.dev.processes.api]
run = "printf api"
tas = "api"
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error.to_string().contains("unknown field `tas`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_run_step_field() {
    let root = temp_workspace("reject-unknown-run-step-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.reset-db]
run = [
  { run = "echo one", rnu = "echo two" }
]
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error.to_string().contains("unknown field `rnu`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_tasks_rejects_unknown_catalog_field() {
    let root = temp_workspace("reject-unknown-catalog-field");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[catalog]
alias = "farmyard"
aliass = "dup"
"#,
    );

    let err = run_tasks(TasksArgs {
        repo_override: Some(root),
        task_name: None,
    })
    .expect_err("expected manifest parse failure");

    match err {
        RunnerError::TaskManifestParse { error, .. } => {
            assert!(error.to_string().contains("unknown field `aliass`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_health_falls_back_to_repo_pulse() {
    let root = temp_workspace("builtin-health-fallback");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "dev": "echo dev"
  }
}"#,
    )
    .expect("write package");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "health".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("builtin health fallback");

    assert!(out.contains("Pulse Report"));
}

#[test]
fn run_manifest_task_prefixed_builtin_health_targets_catalog_root() {
    let root = temp_workspace("builtin-health-prefixed");
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");

    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.admin]\nrun = \"printf admin\"\n",
    );
    fs::write(
        farmyard.join("package.json"),
        r#"{
  "scripts": {
    "farmyard-only": "echo ok"
  }
}"#,
    )
    .expect("write farmyard package");
    fs::write(
        dairy.join("package.json"),
        r#"{
  "scripts": {
    "dairy-only": "echo ok"
  }
}"#,
    )
    .expect("write dairy package");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard/health".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("prefixed health fallback");

    assert!(out.contains("Pulse Report"));
    assert!(out.contains("farmyard-only"));
    assert!(!out.contains("dairy-only"));
}

#[test]
fn run_manifest_task_builtin_test_failure_keeps_rendered_results_summary() {
    let root = temp_workspace("builtin-test-fanout-failure-summary");
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );

    fs::write(
        farmyard.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write farmyard package");
    fs::write(
        dairy.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write dairy package");

    let farmyard_bin = farmyard.join("node_modules/.bin");
    fs::create_dir_all(&farmyard_bin).expect("mkdir farmyard bin");
    let dairy_bin = dairy.join("node_modules/.bin");
    fs::create_dir_all(&dairy_bin).expect("mkdir dairy bin");

    let farmyard_vitest = farmyard_bin.join("vitest");
    fs::write(&farmyard_vitest, "#!/bin/sh\nexit 1\n").expect("write farmyard vitest");
    let mut farmyard_perms = fs::metadata(&farmyard_vitest).expect("stat").permissions();
    farmyard_perms.set_mode(0o755);
    fs::set_permissions(&farmyard_vitest, farmyard_perms).expect("chmod");

    let dairy_vitest = dairy_bin.join("vitest");
    fs::write(&dairy_vitest, "#!/bin/sh\nexit 0\n").expect("write dairy vitest");
    let mut dairy_perms = fs::metadata(&dairy_vitest).expect("stat").permissions();
    dairy_perms.set_mode(0o755);
    fs::set_permissions(&dairy_vitest, dairy_perms).expect("chmod");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect_err("builtin test fanout should fail");

    match err {
        RunnerError::BuiltinTestNonZero { failures, rendered } => {
            assert_eq!(failures, vec![("farmyard".to_owned(), Some(1))]);
            assert!(rendered.contains("Test Results"));
            assert!(rendered.contains("dairy"));
            assert!(rendered.contains("ok"));
            assert!(rendered.contains("farmyard"));
            assert!(rendered.contains("exit=1"));
            assert!(!rendered.contains("runner:vitest"));
            assert!(!rendered.contains("command:"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_test_failure_with_suite_filter_shows_no_match_hint() {
    let root = temp_workspace("builtin-test-filtered-failure-hint");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 1\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["vitest".to_owned(), "user-service".to_owned()],
        },
        root,
    )
    .expect_err("filtered suite run should fail");

    match err {
        RunnerError::BuiltinTestNonZero { rendered, .. } => {
            assert!(rendered.contains("Hint"));
            assert!(rendered.contains("often means no tests matched"));
            assert!(rendered.contains("vitest run 'user-service'"));
            assert!(rendered.contains("Try again without the filter"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_test_verbose_results_include_runner_root_and_command() {
    let root = temp_workspace("builtin-test-verbose-results");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 0\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--verbose-results".to_owned(), "--run".to_owned()],
        },
        root,
    )
    .expect("run builtin test");

    assert!(out.contains("Test Results"));
    assert!(out.contains("runner:vitest"));
    assert!(out.contains("root:"));
    assert!(out.contains("command:vitest run '--run'"));
}

#[test]
fn run_manifest_task_builtin_test_tui_flag_falls_back_to_text_when_non_interactive() {
    let root = temp_workspace("builtin-test-tui-fallback");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 0\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--tui".to_owned()],
        },
        root,
    )
    .expect("run builtin test with tui flag");

    assert!(out.contains("Test Results"));
    assert!(out.contains("root"));
}

#[test]
fn run_manifest_task_builtin_test_plan_respects_configured_package_manager() {
    let root = temp_workspace("builtin-test-plan-package-manager");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[package_manager]
js = "pnpm"
"#,
    );
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--plan".to_owned()],
        },
        root,
    )
    .expect("run test --plan");

    assert!(out.contains("pnpm exec vitest run"));
    assert!(out.contains("package_manager.js=pnpm"));
}

#[test]
fn run_manifest_task_builtin_test_exec_uses_configured_package_manager() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("builtin-test-exec-package-manager");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[package_manager]
js = "bun"
"#,
    );
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let bun_stub = bin_dir.join("bun");
    let args_log = root.join("bun-args.log");
    fs::write(
        &bun_stub,
        "#!/bin/sh\nprintf \"%s\\n\" \"$@\" > \"$EFFIGY_TEST_BUN_ARGS_FILE\"\n",
    )
    .expect("write bun stub");
    let mut perms = fs::metadata(&bun_stub).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bun_stub, perms).expect("chmod");

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        ("SHELL", Some("/bin/sh".to_owned())),
        (
            "EFFIGY_TEST_BUN_ARGS_FILE",
            Some(args_log.display().to_string()),
        ),
    ]);

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["vitest".to_owned()],
        },
        root,
    )
    .expect("run builtin test");

    assert!(out.contains("Test Results"));
    let args = fs::read_to_string(args_log).expect("read bun args");
    assert_eq!(args, "x\nvitest\nrun\n");
}

#[test]
fn run_manifest_task_builtin_test_plan_respects_runner_command_override() {
    let root = temp_workspace("builtin-test-plan-runner-override");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[test.runners]
vitest = "pnpm exec vitest run --config vitest.config.ts"
"#,
    );
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--plan".to_owned(), "vitest".to_owned()],
        },
        root,
    )
    .expect("run test --plan");

    assert!(out.contains("pnpm exec vitest run --config vitest.config.ts"));
    assert!(out.contains("test.runners.vitest command override applied"));
}

#[test]
fn run_manifest_task_builtin_test_runner_override_wins_over_package_manager() {
    let root = temp_workspace("builtin-test-plan-override-precedence");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[package_manager]
js = "bun"

[test.runners]
vitest = "npx vitest run --reporter=dot"
"#,
    );
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--plan".to_owned(), "vitest".to_owned()],
        },
        root,
    )
    .expect("run test --plan");

    assert!(out.contains("npx vitest run --reporter=dot"));
    assert!(out.contains("package_manager.js=bun"));
    assert!(out.contains("test.runners.vitest command override applied"));
}

#[test]
fn run_manifest_task_builtin_config_prints_reference() {
    let root = temp_workspace("builtin-config");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("run config");

    assert!(out.contains("effigy.toml Reference"));
    assert!(out.contains("[test.runners]"));
    assert!(out.contains("[tasks]"));
    assert!(out.contains("Compact tasks entries are shorthand"));
    assert!(out.contains("task = \"../froyo/validate\""));
    assert!(out.contains("Cross-repo task references support aliases"));
}

#[test]
fn run_manifest_task_builtin_config_schema_prints_canonical_template() {
    let root = temp_workspace("builtin-config-schema");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--schema".to_owned()],
        },
        root,
    )
    .expect("run config --schema");

    assert!(out.contains("Canonical strict-valid effigy.toml schema template"));
    assert!(out.contains("[package_manager]"));
    assert!(out.contains("[test.runners]"));
    assert!(out.contains("[tasks.dev.profiles.default]"));
    assert!(out.contains("task = \"../froyo/validate\""));
    assert!(out.contains("start = [\"api\", \"validate-stack\"]"));
}

#[test]
fn run_manifest_task_builtin_config_schema_minimal_prints_starter_template() {
    let root = temp_workspace("builtin-config-schema-minimal");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--schema".to_owned(), "--minimal".to_owned()],
        },
        root,
    )
    .expect("run config --schema --minimal");

    assert!(out.contains("Minimal strict-valid effigy.toml starter"));
    assert!(out.contains("[package_manager]"));
    assert!(out.contains("[test.runners]"));
    assert!(out.contains("[tasks]"));
    assert!(!out.contains("[tasks.dev.profiles.default]"));
}

#[test]
fn run_manifest_task_builtin_config_schema_target_prints_selected_section() {
    let root = temp_workspace("builtin-config-schema-target");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec![
                "--schema".to_owned(),
                "--target".to_owned(),
                "test".to_owned(),
            ],
        },
        root,
    )
    .expect("run config --schema --target test");

    assert!(out.contains("(test target)"));
    assert!(out.contains("[test.runners]"));
    assert!(!out.contains("[tasks]"));
}

#[test]
fn run_manifest_task_builtin_config_schema_target_test_runner_prints_single_runner_snippet() {
    let root = temp_workspace("builtin-config-schema-target-test-runner");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec![
                "--schema".to_owned(),
                "--target".to_owned(),
                "test".to_owned(),
                "--runner".to_owned(),
                "nextest".to_owned(),
            ],
        },
        root,
    )
    .expect("run config --schema --target test --runner nextest");

    assert!(out.contains("(test target, runner: cargo-nextest)"));
    assert!(out.contains("\"cargo-nextest\" = \"cargo nextest run\""));
    assert!(!out.contains("vitest = "));
    assert!(!out.contains("\"cargo-test\" = \"cargo test\""));
}

#[test]
fn run_manifest_task_builtin_config_target_requires_schema_flag() {
    let root = temp_workspace("builtin-config-target-requires-schema");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--target".to_owned(), "test".to_owned()],
        },
        root,
    )
    .expect_err("expected --target precondition failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--target` requires `--schema`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_runner_requires_schema_flag() {
    let root = temp_workspace("builtin-config-runner-requires-schema");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--runner".to_owned(), "vitest".to_owned()],
        },
        root,
    )
    .expect_err("expected --runner precondition failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--runner` requires `--schema`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_runner_requires_test_target() {
    let root = temp_workspace("builtin-config-runner-requires-test-target");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec![
                "--schema".to_owned(),
                "--target".to_owned(),
                "tasks".to_owned(),
                "--runner".to_owned(),
                "vitest".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected --runner target guard");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--runner` requires `--target test`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_rejects_invalid_runner_value() {
    let root = temp_workspace("builtin-config-invalid-runner");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec![
                "--schema".to_owned(),
                "--target".to_owned(),
                "test".to_owned(),
                "--runner".to_owned(),
                "jest".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected invalid --runner failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("invalid `--runner` value `jest`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_target_requires_value() {
    let root = temp_workspace("builtin-config-target-requires-value");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--schema".to_owned(), "--target".to_owned()],
        },
        root,
    )
    .expect_err("expected --target value failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--target` requires a value"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_rejects_invalid_target_value() {
    let root = temp_workspace("builtin-config-invalid-target");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec![
                "--schema".to_owned(),
                "--target".to_owned(),
                "python".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected invalid --target failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("invalid `--target` value `python`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_minimal_requires_schema_flag() {
    let root = temp_workspace("builtin-config-minimal-requires-schema");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--minimal".to_owned()],
        },
        root,
    )
    .expect_err("expected --minimal precondition failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--minimal` requires `--schema`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_config_rejects_unknown_args() {
    let root = temp_workspace("builtin-config-unknown-args");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--wat".to_owned()],
        },
        root,
    )
    .expect_err("expected config argument failure");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("unknown argument(s) for built-in `config`: --wat"));
        }
        other => panic!("unexpected error: {other}"),
    }
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

#[test]
fn run_pulse_renders_widget_sections() {
    let root = temp_workspace("pulse-render");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "health:workspace": "echo ok"
  }
}"#,
    )
    .expect("write package");

    let out = run_pulse(PulseArgs {
        repo_override: Some(root),
        verbose_root: false,
    })
    .expect("pulse");

    assert!(out.contains("Pulse Report"));
    assert!(!out.contains("repo:"));
    assert!(out.contains("signals:"));
    assert!(out.contains("Signals"));
    assert!(out.contains("Risks"));
    assert!(out.contains("Actions"));
    assert!(out.contains("No risk items.") || out.contains("- Root "));
    assert!(
        out.contains("No high-priority structural gaps detected by pulse v0 signals.")
            || out.contains("health:workspace")
    );
    assert!(out.contains("summary  ok:"));
}

#[test]
fn run_pulse_verbose_renders_root_resolution_section() {
    let root = temp_workspace("pulse-verbose");

    let out = run_pulse(PulseArgs {
        repo_override: Some(root),
        verbose_root: true,
    })
    .expect("pulse");

    assert!(out.contains("Root Resolution"));
    assert!(out.contains("resolved-root:"));
    assert!(out.contains("mode:"));
    assert!(out.contains("Pulse Report"));
}

#[test]
fn run_pulse_colorizes_inline_code_segments_when_color_enabled() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("pulse-inline-code-color");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
run = "echo dev"
"#,
    );
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "dev": "echo dev"
  }
}"#,
    )
    .expect("write package");

    let _env = EnvGuard::set_many(&[
        ("EFFIGY_COLOR", Some("always".to_owned())),
        ("NO_COLOR", None),
    ]);
    let out = run_pulse(PulseArgs {
        repo_override: Some(root),
        verbose_root: false,
    })
    .expect("pulse");

    assert!(out.contains("\u{1b}[38;5;117m`tasks.health`"));
}

#[test]
fn run_manifest_task_defers_when_unprefixed_task_missing() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("defer-missing");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "unknown-task".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("deferred run should succeed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_defers_and_supports_request_and_args_tokens() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("defer-tokens");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"test {request} = 'unknown-task' && test {args} = '--dry-run'\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "unknown-task".to_owned(),
            args: vec!["--dry-run".to_owned()],
        },
        root,
    )
    .expect("deferred token substitution should succeed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_defers_for_path_like_request_when_prefix_not_found() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("defer-path-like-request");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"test {request} = 'services/api/dev' && test {args} = '--watch'\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "services/api/dev".to_owned(),
            args: vec!["--watch".to_owned()],
        },
        root,
    )
    .expect("path-like deferred request should succeed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_defers_to_prefixed_catalog_handler() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("defer-prefixed");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(&root.join("effigy.toml"), "[defer]\nrun = \"false\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[defer]\nrun = \"printf farmyard-deferred\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard/missing".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("prefixed deferral should succeed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_deferral_loop_guard_fails() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("defer-loop");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\n",
    );

    std::env::set_var("EFFIGY_DEFER_DEPTH", "1");
    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "unknown-task".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect_err("loop guard should fail");
    std::env::remove_var("EFFIGY_DEFER_DEPTH");

    match err {
        RunnerError::DeferLoopDetected { depth } => assert_eq!(depth, 1),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_implicitly_defers_to_root_when_no_configured_deferral() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("implicit-root-defer");
    fs::write(root.join("effigy.json"), "{}\n").expect("write effigy marker");
    fs::write(root.join("composer.json"), "{}\n").expect("write composer marker");

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let composer_stub = bin_dir.join("composer");
    let args_log = root.join("composer-args.log");
    fs::write(
        &composer_stub,
        "#!/bin/sh\nprintf \"%s\\n\" \"$@\" > \"$EFFIGY_TEST_COMPOSER_ARGS_FILE\"\n",
    )
    .expect("write composer stub");
    let mut perms = fs::metadata(&composer_stub)
        .expect("metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&composer_stub, perms).expect("chmod");

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        ("SHELL", Some("/bin/sh".to_owned())),
        (
            "EFFIGY_TEST_COMPOSER_ARGS_FILE",
            Some(args_log.display().to_string()),
        ),
    ]);

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "version".to_owned(),
            args: vec!["--dry-run".to_owned()],
        },
        root.clone(),
    )
    .expect("implicit root deferral should succeed");

    assert_eq!(out, "");
    let args = fs::read_to_string(args_log).expect("read composer args");
    assert_eq!(args, "global\nexec\neffigy\n--\nversion\n--dry-run\n");
}

#[test]
fn run_manifest_task_explicit_deferral_wins_over_implicit_root_deferral() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("explicit-over-implicit");
    fs::write(root.join("effigy.json"), "{}\n").expect("write effigy marker");
    fs::write(root.join("composer.json"), "{}\n").expect("write composer marker");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"printf explicit\"\n",
    );

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let composer_stub = bin_dir.join("composer");
    let marker = root.join("composer-called.log");
    fs::write(
        &composer_stub,
        "#!/bin/sh\nprintf called > \"$EFFIGY_TEST_COMPOSER_ARGS_FILE\"\nexit 99\n",
    )
    .expect("write composer stub");
    let mut perms = fs::metadata(&composer_stub)
        .expect("metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&composer_stub, perms).expect("chmod");

    let prior_path = std::env::var("PATH").ok().unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), prior_path);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(path)),
        ("SHELL", Some("/bin/sh".to_owned())),
        (
            "EFFIGY_TEST_COMPOSER_ARGS_FILE",
            Some(marker.display().to_string()),
        ),
    ]);

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "missing".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("explicit deferral should succeed");

    assert_eq!(out, "");
    assert!(!marker.exists(), "composer fallback should not be invoked");
}

#[test]
fn run_manifest_task_managed_tui_uses_default_profile_when_not_specified() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-default-profile");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles.default]
processes = ["api", "front", "admin"]

[tasks.dev.profiles.admin]
processes = ["api", "admin"]

[tasks.dev.processes.api]
run = "cargo run -p api"

[tasks.dev.processes.front]
run = "vite dev"

[tasks.dev.processes.admin]
run = "vite dev --config admin"
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root.clone(),
    )
    .expect("managed plan should render");

    assert!(out.contains("Managed Task Plan"));
    assert!(out.contains("profile: default"));
    assert!(out.contains("api"));
    assert!(out.contains("front"));
    assert!(out.contains("admin"));
    assert!(out.contains("fail-on-non-zero: enabled"));
}

#[test]
fn run_manifest_task_managed_tui_accepts_named_profile_argument() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-named-profile");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles.default]
processes = ["api", "front", "admin"]

[tasks.dev.profiles.admin]
processes = ["api", "admin"]

[tasks.dev.processes.api]
run = "cargo run -p api"

[tasks.dev.processes.front]
run = "vite dev"

[tasks.dev.processes.admin]
run = "vite dev --config admin"
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["admin".to_owned()],
        },
        root,
    )
    .expect("managed plan should render");

    assert!(out.contains("profile: admin"));
    assert!(out.contains("api"));
    assert!(out.contains("admin"));
    assert!(!out.contains("front"));
}

#[test]
fn run_manifest_task_managed_tui_supports_independent_tab_order() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-tab-order");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles]
default = { start = ["api", "jobs", "cream", "dairy"], tabs = ["dairy", "cream", "api", "jobs"] }

[tasks.dev.processes.api]
run = "printf api"

[tasks.dev.processes.jobs]
run = "printf jobs"

[tasks.dev.processes.cream]
run = "printf cream"

[tasks.dev.processes.dairy]
run = "printf dairy"
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect("managed plan should render");

    assert!(out.contains("tab-order: dairy, cream, api, jobs"));
}

#[test]
fn run_manifest_task_managed_tui_supports_ranked_tab_order_map() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-tab-order-ranked");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles]
default = { start = ["api", "jobs", "cream", "dairy"], tabs = { dairy = 1, cream = 2, api = 3, jobs = 4 } }

[tasks.dev.processes.api]
run = "printf api"

[tasks.dev.processes.jobs]
run = "printf jobs"

[tasks.dev.processes.cream]
run = "printf cream"

[tasks.dev.processes.dairy]
run = "printf dairy"
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect("managed plan should render");

    assert!(out.contains("tab-order: dairy, cream, api, jobs"));
}

#[test]
fn run_manifest_task_managed_tui_supports_ranked_tab_order_map_with_task_refs() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-tab-order-ranked-refs");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    let farmyard = root.join("farmyard");
    let cream = root.join("cream");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&cream).expect("mkdir cream");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles]
default = { start = ["farmyard/api", "farmyard/jobs", "cream/dev", "dairy/dev"], tabs = { "dairy/dev" = 1, "cream/dev" = 2, "farmyard/api" = 3, "farmyard/jobs" = 4 } }
"#,
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        r#"[catalog]
alias = "farmyard"
[tasks.api]
run = "printf farmyard-api"
[tasks.jobs]
run = "printf farmyard-jobs"
"#,
    );
    write_manifest(
        &cream.join("effigy.toml"),
        r#"[catalog]
alias = "cream"
[tasks.dev]
run = "printf cream-dev"
"#,
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        r#"[catalog]
alias = "dairy"
[tasks.dev]
run = "printf dairy-dev"
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect("managed plan should render");

    assert!(out.contains("tab-order: dairy/dev, cream/dev, farmyard/api, farmyard/jobs"));
}

#[test]
fn run_manifest_task_managed_tui_supports_single_definition_ordered_profile_entries() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-single-definition-ordered-profile");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    let farmyard = root.join("farmyard");
    let cream = root.join("cream");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&cream).expect("mkdir cream");
    fs::create_dir_all(&dairy).expect("mkdir dairy");

    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles]
default = { "farmyard/api" = { start = 1, tab = 3 }, "farmyard/jobs" = { start = 2, tab = 4, start_after_ms = 1200 }, "cream/dev" = { start = 3, tab = 2 }, "dairy/dev" = { start = 4, tab = 1 } }
"#,
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        r#"[catalog]
alias = "farmyard"
[tasks.api]
run = "printf farmyard-api"
[tasks.jobs]
run = "printf farmyard-jobs"
"#,
    );
    write_manifest(
        &cream.join("effigy.toml"),
        r#"[catalog]
alias = "cream"
[tasks.dev]
run = "printf cream-dev"
"#,
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        r#"[catalog]
alias = "dairy"
[tasks.dev]
run = "printf dairy-dev"
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect("managed plan should render");

    assert!(out.contains("tab-order: dairy/dev, cream/dev, farmyard/api, farmyard/jobs"));
    assert!(out.contains("start-after-ms"));
    assert!(out.contains("1200"));
}

#[test]
fn run_manifest_task_managed_tui_errors_when_tab_order_references_unknown_process() {
    let root = temp_workspace("managed-tab-order-invalid");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles.default]
processes = ["api"]
tabs = ["api", "jobs"]

[tasks.dev.processes.api]
run = "printf api"
"#,
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect_err("invalid tab entry should fail");

    match err {
        RunnerError::TaskManagedProfileTabOrderInvalid {
            task,
            profile,
            detail,
        } => {
            assert_eq!(task, "dev");
            assert_eq!(profile, "default");
            assert!(detail.contains("jobs"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_managed_tui_errors_for_unknown_profile() {
    let root = temp_workspace("managed-unknown-profile");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles.default]
processes = ["api"]

[tasks.dev.processes.api]
run = "cargo run -p api"
"#,
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["admin".to_owned()],
        },
        root,
    )
    .expect_err("unknown profile should fail");

    match err {
        RunnerError::TaskManagedProfileNotFound {
            task,
            profile,
            available,
        } => {
            assert_eq!(task, "dev");
            assert_eq!(profile, "admin");
            assert_eq!(available, vec!["default".to_owned()]);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_managed_tui_processes_can_reference_other_tasks() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-task-refs");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    let farmyard = root.join("farmyard");
    let cream = root.join("cream");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&cream).expect("mkdir cream");

    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles.default]
processes = ["api", "front"]

[tasks.dev.processes.api]
task = "farmyard/api"

[tasks.dev.processes.front]
task = "cream/dev"
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
        &cream.join("effigy.toml"),
        r#"[catalog]
alias = "cream"
[tasks.dev]
run = "printf cream-dev"
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("managed plan should render");

    assert!(out.contains("farmyard-api"));
    assert!(out.contains("cream-dev"));
    assert!(out.contains(&farmyard.display().to_string()));
    assert!(out.contains(&cream.display().to_string()));
}

#[test]
fn run_manifest_task_managed_tui_errors_when_process_has_run_and_task() {
    let root = temp_workspace("managed-invalid-process-def");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles.default]
processes = ["api"]

[tasks.dev.processes.api]
run = "printf api"
task = "api"
"#,
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect_err("invalid process definition should fail");

    match err {
        RunnerError::TaskManagedProcessInvalidDefinition { task, process, .. } => {
            assert_eq!(task, "dev");
            assert_eq!(process, "api");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_managed_tui_supports_compact_profile_task_refs() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-compact-profile-refs");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    let farmyard = root.join("farmyard");
    let cream = root.join("cream");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&cream).expect("mkdir cream");

    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles]
default = ["farmyard/api", "cream/dev"]
admin = ["farmyard/api"]
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
        &cream.join("effigy.toml"),
        r#"[catalog]
alias = "cream"
[tasks.dev]
run = "printf cream-dev"
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect("managed compact plan should render");

    assert!(out.contains("profile: default"));
    assert!(out.contains("farmyard-api"));
    assert!(out.contains("cream-dev"));
    assert!(out.contains("farmyard/api"));
    assert!(out.contains("cream/dev"));
}

#[test]
fn run_manifest_task_managed_tui_process_run_array_supports_task_refs() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-process-run-array");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);

    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles.default]
processes = ["combo"]

[tasks.dev.processes.combo]
run = ["printf start", { task = "farmyard/api" }, "printf done"]
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect("managed plan should render");

    assert!(out.contains("printf start"));
    assert!(out.contains("farmyard-api"));
    assert!(out.contains("printf done"));
    assert!(out.contains("cd"));
}

#[test]
fn run_manifest_task_managed_tui_supports_relative_task_refs() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-relative-task-ref");
    let dairy = root.join("dairy");
    let froyo = root.join("froyo");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    fs::create_dir_all(&froyo).expect("mkdir froyo");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);

    write_manifest(
        &dairy.join("effigy.toml"),
        r#"[catalog]
alias = "dairy"
[tasks.dev]
mode = "tui"
[tasks.dev.profiles.default]
processes = ["validate-stack"]
[tasks.dev.processes.validate-stack]
task = "../froyo/validate"
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dairy/dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect("managed plan should render");

    assert!(out.contains("validate-stack"));
    assert!(out.contains("froyo-validate"));
    assert!(out.contains(&froyo.display().to_string()));
}

#[test]
fn run_manifest_task_managed_tui_appends_shell_process_when_enabled() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-shell-enabled");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
shell = true

[tasks.dev.profiles]
default = ["api"]

[tasks.dev.processes.api]
run = "printf api"
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("managed plan should include shell process");

    assert!(out.contains("shell"));
    assert!(out.contains("exec ${SHELL:-/bin/zsh} -i"));
}

#[test]
fn run_manifest_task_managed_tui_uses_global_shell_run_override() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-shell-global-override");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[shell]
run = "exec ${SHELL:-/bin/bash} -i"

[tasks.dev]
mode = "tui"
shell = true

[tasks.dev.profiles]
default = ["api"]

[tasks.dev.processes.api]
run = "printf api"
"#,
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("managed plan should include configured shell process");

    assert!(out.contains("shell"));
    assert!(out.contains("exec ${SHELL:-/bin/bash} -i"));
}

#[test]
fn run_manifest_task_managed_stream_executes_selected_profile_processes() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-stream-runtime");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles.default]
processes = ["api", "front"]

[tasks.dev.processes.api]
run = "printf api-ok"

[tasks.dev.processes.front]
run = "printf front-ok"
"#,
    );
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))]);

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("managed stream run");

    assert!(out.contains("Managed Task Runtime"));
    assert!(out.contains("[api] api-ok"));
    assert!(out.contains("[front] front-ok"));
    assert!(out.contains("fail-on-non-zero: enabled"));
    assert!(out.contains("process `api` exit=0"));
    assert!(out.contains("process `front` exit=0"));
}

#[test]
fn run_manifest_task_managed_stream_fails_when_process_exits_non_zero_by_default() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-stream-fail-on-non-zero-default");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles.default]
processes = ["api"]

[tasks.dev.processes.api]
run = "sh -lc 'exit 7'"
"#,
    );
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))]);

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect_err("managed stream should fail for non-zero exit by default");

    match err {
        RunnerError::TaskManagedNonZeroExit {
            task,
            profile,
            processes,
        } => {
            assert_eq!(task, "dev");
            assert_eq!(profile, "default");
            assert_eq!(processes, vec![("api".to_owned(), "exit=7".to_owned())]);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_managed_stream_allows_non_zero_when_disabled() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("managed-stream-fail-on-non-zero-disabled");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
fail_on_non_zero = false

[tasks.dev.profiles.default]
processes = ["api"]

[tasks.dev.processes.api]
run = "sh -lc 'exit 9'"
"#,
    );
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))]);

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("managed stream should allow non-zero when disabled");

    assert!(out.contains("Managed Task Runtime"));
    assert!(out.contains("fail-on-non-zero: disabled"));
    assert!(out.contains("process `api` exit=9"));
}

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
    let _guard = test_lock().lock().expect("lock");
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
