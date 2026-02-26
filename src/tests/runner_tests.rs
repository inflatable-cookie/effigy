use super::{
    parse_task_runtime_args, parse_task_selector, run_manifest_task_with_cwd, run_tasks,
    RunnerError, TaskRuntimeArgs,
};
use crate::{TaskInvocation, TasksArgs};
use std::fs;
use std::path::PathBuf;
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
    let selector = parse_task_selector("farmyard:reset-db").expect("selector");
    assert_eq!(selector.prefix, Some("farmyard".to_owned()));
    assert_eq!(selector.task_name, "reset-db");
}

#[test]
fn run_manifest_task_prefixed_uses_named_catalog() {
    let root = temp_workspace("prefixed");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");

    write_manifest(
        &root.join("effigy.tasks.toml"),
        "[tasks.ping]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.tasks.toml"),
        "[tasks.ping]\nrun = \"printf farmyard\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard:ping".to_owned(),
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
        &root.join("effigy.tasks.toml"),
        "[tasks.ping]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.tasks.toml"),
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
        &farmyard.join("effigy.tasks.toml"),
        "[tasks.reset-db]\nrun = \"printf farmyard\"\n",
    );
    write_manifest(
        &dairy.join("effigy.tasks.toml"),
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
fn run_manifest_task_unknown_prefix_returns_catalog_error() {
    let root = temp_workspace("unknown-prefix");
    write_manifest(
        &root.join("effigy.tasks.toml"),
        "[tasks.reset-db]\nrun = \"printf root\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard:reset-db".to_owned(),
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
    let root = temp_workspace("verbose-trace");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(
        &root.join("effigy.tasks.toml"),
        "[tasks.ping]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.tasks.toml"),
        "[tasks.ping]\nrun = \"printf farmyard\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard:ping".to_owned(),
            args: vec!["--verbose-root".to_owned()],
        },
        root,
    )
    .expect("run");

    assert!(out.contains("# Task Resolution"));
    assert!(out.contains("catalog-alias: farmyard"));
    assert!(out.contains("farmyard"));
}

#[test]
fn run_tasks_lists_catalogs_and_tasks() {
    let root = temp_workspace("list-tasks");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(
        &root.join("effigy.tasks.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.tasks.toml"),
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
fn run_tasks_with_task_filter_reports_only_matches() {
    let root = temp_workspace("task-filter");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(
        &root.join("effigy.tasks.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.tasks.toml"),
        "[tasks.reset-db]\nrun = \"printf farmyard\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("reset-db".to_owned()),
        })
    })
    .expect("run tasks");

    assert!(out.contains("farmyard:reset-db"));
    assert!(!out.contains("root:reset-db"));
}

#[test]
fn run_tasks_reads_legacy_manifest_when_effigy_manifest_missing() {
    let root = temp_workspace("legacy-manifest");
    write_manifest(
        &root.join("underlay.tasks.toml"),
        "[tasks.dev]\nrun = \"printf legacy\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
        })
    })
    .expect("run tasks");

    assert!(out.contains("legacy"));
}

#[test]
fn run_manifest_task_defers_when_unprefixed_task_missing() {
    let root = temp_workspace("defer-missing");
    write_manifest(
        &root.join("effigy.tasks.toml"),
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
    let root = temp_workspace("defer-tokens");
    write_manifest(
        &root.join("effigy.tasks.toml"),
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
fn run_manifest_task_defers_to_prefixed_catalog_handler() {
    let root = temp_workspace("defer-prefixed");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(
        &root.join("effigy.tasks.toml"),
        "[defer]\nrun = \"false\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.tasks.toml"),
        "[catalog]\nalias = \"farmyard\"\n[defer]\nrun = \"printf farmyard-deferred\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "farmyard:missing".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("prefixed deferral should succeed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_deferral_loop_guard_fails() {
    let root = temp_workspace("defer-loop");
    write_manifest(
        &root.join("effigy.tasks.toml"),
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
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(cwd).expect("set cwd");
    let out = f();
    std::env::set_current_dir(original).expect("restore cwd");
    out
}
