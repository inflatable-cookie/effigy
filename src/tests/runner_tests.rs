use super::{
    parse_task_runtime_args, parse_task_selector, run_manifest_task_with_cwd, run_pulse, run_tasks,
    RunnerError, TaskRuntimeArgs,
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
        &root.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
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
fn run_manifest_task_unknown_prefix_returns_catalog_error() {
    let root = temp_workspace("unknown-prefix");
    write_manifest(
        &root.join("effigy.toml"),
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
        &root.join("effigy.toml"),
        "[tasks.ping]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
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

    assert!(out.contains("Task Resolution"));
    assert!(out.contains("catalog-alias: farmyard"));
    assert!(out.contains("farmyard"));
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
fn run_tasks_without_catalogs_still_lists_builtin_tasks() {
    let root = temp_workspace("builtins-only");

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
        })
    })
    .expect("run tasks");

    assert!(out.contains("no task catalogs found; showing built-in tasks only"));
    assert!(out.contains("Built-in Tasks"));
    assert!(out.contains("repo-pulse"));
    assert!(out.contains("tasks"));
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

    assert!(out.contains("\u{1b}[38;5;117m`health:workspace`"));
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
fn run_manifest_task_defers_to_prefixed_catalog_handler() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("defer-prefixed");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"false\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
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
fn run_manifest_task_implicitly_defers_to_legacy_root_when_no_configured_deferral() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("implicit-legacy-defer");
    fs::write(root.join("effigy.json"), "{}\n").expect("write legacy marker");
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
    .expect("implicit legacy deferral should succeed");

    assert_eq!(out, "");
    let args = fs::read_to_string(args_log).expect("read composer args");
    assert_eq!(args, "global\nexec\neffigy\n--\nversion\n--dry-run\n");
}

#[test]
fn run_manifest_task_explicit_deferral_wins_over_implicit_legacy_fallback() {
    let _guard = test_lock().lock().expect("lock");
    let root = temp_workspace("explicit-over-implicit");
    fs::write(root.join("effigy.json"), "{}\n").expect("write legacy marker");
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
    let root = temp_workspace("managed-default-profile");
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

[tasks.dev.shell]
enabled = true
run = "$SHELL"
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
    assert!(out.contains("shell-tab: enabled"));
}

#[test]
fn run_manifest_task_managed_tui_accepts_named_profile_argument() {
    let root = temp_workspace("managed-named-profile");
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
    let root = temp_workspace("managed-task-refs");
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
task = "farmyard:api"

[tasks.dev.processes.front]
task = "cream:dev"
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
    let root = temp_workspace("managed-compact-profile-refs");
    let farmyard = root.join("farmyard");
    let cream = root.join("cream");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&cream).expect("mkdir cream");

    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"

[tasks.dev.profiles]
default = ["farmyard:api", "cream:dev"]
admin = ["farmyard:api"]
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
    assert!(out.contains("farmyard:api"));
    assert!(out.contains("cream:dev"));
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
    assert!(out.contains("process `api` exit=0"));
    assert!(out.contains("process `front` exit=0"));
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
