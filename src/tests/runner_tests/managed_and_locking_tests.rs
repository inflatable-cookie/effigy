use super::*;

#[test]
fn run_manifest_task_managed_tui_uses_default_profile_when_not_specified() {
    let _guard = lock_test();
    let root = temp_workspace("managed-default-profile");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "front", run = "vite dev", start = 2, tab = 2 },
  { name = "admin", run = "vite dev --config admin", start = 3, tab = 3 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "admin", run = "vite dev --config admin", start = 2, tab = 2 }
]
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
    let _guard = lock_test();
    let root = temp_workspace("managed-named-profile");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "front", run = "vite dev", start = 2, tab = 2 },
  { name = "admin", run = "vite dev --config admin", start = 3, tab = 3 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "admin", run = "vite dev --config admin", start = 2, tab = 2 }
]
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
fn run_manifest_task_managed_tui_supports_concurrent_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-concurrent-entries");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { task = "api", start = 1, tab = 3 },
  { run = "printf background", start = 2, tab = 2, start_after_ms = 250 },
  { task = "front", start = 3, tab = 1 }
]

[tasks.api]
run = "printf api"

[tasks.front]
run = "printf front"
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

    assert!(out.contains("Managed Task Plan"));
    assert!(out.contains("profile: default"));
    assert!(out.contains("tab-order: front, process-2, api"));
    assert!(out.contains("printf api"));
    assert!(out.contains("printf background"));
    assert!(out.contains("printf front"));
    assert!(out.contains("250"));
}

#[test]
fn run_manifest_task_managed_tui_rejects_concurrent_entry_with_both_task_and_run() {
    let root = temp_workspace("managed-concurrent-invalid-entry");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { task = "api", run = "printf oops", start = 1, tab = 1 }
]

[tasks.api]
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
    .expect_err("invalid concurrent entry should fail");

    match err {
        RunnerError::TaskManagedProcessInvalidDefinition {
            process, detail, ..
        } => {
            assert_eq!(process, "api");
            assert!(detail.contains("either `task` or `run`"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_managed_tui_supports_profile_specific_concurrent_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-concurrent-profile-specific");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { run = "printf default-api", start = 1, tab = 2 },
  { run = "printf default-front", start = 2, tab = 1 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { run = "printf admin-api", start = 1, tab = 1 }
]
"#,
    );

    let out_default = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root.clone(),
    )
    .expect("default managed plan should render");
    assert!(out_default.contains("profile: default"));
    assert!(out_default.contains("default-api"));
    assert!(out_default.contains("default-front"));
    assert!(!out_default.contains("admin-api"));

    let out_admin = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["admin".to_owned()],
        },
        root,
    )
    .expect("admin managed plan should render");
    assert!(out_admin.contains("profile: admin"));
    assert!(out_admin.contains("admin-api"));
    assert!(!out_admin.contains("default-front"));
}

#[test]
fn run_manifest_task_managed_tui_supports_independent_tab_order() {
    let _guard = lock_test();
    let root = temp_workspace("managed-tab-order");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api", start = 1, tab = 3 },
  { name = "jobs", run = "printf jobs", start = 2, tab = 4 },
  { name = "cream", run = "printf cream", start = 3, tab = 2 },
  { name = "dairy", run = "printf dairy", start = 4, tab = 1 }
]
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
    let _guard = lock_test();
    let root = temp_workspace("managed-tab-order-ranked");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api", start = 1, tab = 3 },
  { name = "jobs", run = "printf jobs", start = 2, tab = 4 },
  { name = "cream", run = "printf cream", start = 3, tab = 2 },
  { name = "dairy", run = "printf dairy", start = 4, tab = 1 }
]
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
    let _guard = lock_test();
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
concurrent = [
  { task = "farmyard/api", start = 1, tab = 3 },
  { task = "farmyard/jobs", start = 2, tab = 4 },
  { task = "cream/dev", start = 3, tab = 2 },
  { task = "dairy/dev", start = 4, tab = 1 }
]
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
    let _guard = lock_test();
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
concurrent = [
  { task = "farmyard/api", start = 1, tab = 3 },
  { task = "farmyard/jobs", start = 2, tab = 4, start_after_ms = 1200 },
  { task = "cream/dev", start = 3, tab = 2 },
  { task = "dairy/dev", start = 4, tab = 1 }
]
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
fn run_manifest_task_managed_tui_errors_when_concurrent_entry_missing_task_and_run() {
    let root = temp_workspace("managed-tab-order-invalid");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "jobs" }]
"#,
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect_err("invalid concurrent entry should fail");

    match err {
        RunnerError::TaskManagedProcessInvalidDefinition {
            task,
            process,
            detail,
        } => {
            assert_eq!(task, "dev");
            assert_eq!(process, "jobs");
            assert!(detail.contains("missing both `task` and `run`"));
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
concurrent = [{ name = "api", run = "cargo run -p api" }]
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
    let _guard = lock_test();
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
concurrent = [
  { name = "api", task = "farmyard/api" },
  { name = "front", task = "cream/dev" }
]
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
concurrent = [{ name = "api", run = "printf api", task = "api" }]
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
    let _guard = lock_test();
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
concurrent = [{ task = "farmyard/api" }, { task = "cream/dev" }]

[tasks.dev.profiles.admin]
concurrent = [{ task = "farmyard/api" }]
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
fn run_manifest_task_managed_tui_rejects_unterminated_quote_in_compact_profile_task_ref() {
    let _guard = lock_test();
    let root = temp_workspace("managed-compact-profile-ref-unterminated-quote");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = 'test "unterminated' }]
"#,
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect_err("invalid compact profile task ref should fail");

    match err {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task,
            process,
            reference,
            detail,
        } => {
            assert_eq!(task, "dev");
            assert_eq!(process, "tests");
            assert_eq!(reference, "test \"unterminated");
            assert!(detail.contains("unterminated quote"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_managed_tui_process_run_array_supports_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-process-run-array");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);

    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "combo", task = "combo" }]

[tasks.combo]
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
fn run_manifest_task_managed_tui_rejects_unterminated_quote_in_process_task_ref() {
    let _guard = lock_test();
    let root = temp_workspace("managed-process-task-ref-unterminated-quote");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = 'test "unterminated' }]
"#,
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect_err("invalid process task ref should fail");

    match err {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task,
            process,
            reference,
            detail,
        } => {
            assert_eq!(task, "dev");
            assert_eq!(process, "tests");
            assert_eq!(reference, "test \"unterminated");
            assert!(detail.contains("unterminated quote"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_managed_tui_rejects_trailing_escape_in_process_task_ref() {
    let _guard = lock_test();
    let root = temp_workspace("managed-process-task-ref-trailing-escape");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = "test vitest \\" }]
"#,
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--repo".to_owned(), root.display().to_string()],
        },
        root,
    )
    .expect_err("invalid process task ref should fail");

    match err {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task,
            process,
            reference,
            detail,
        } => {
            assert_eq!(task, "dev");
            assert_eq!(process, "tests");
            assert_eq!(reference, "test vitest \\");
            assert!(detail.contains("trailing escape"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_managed_tui_supports_relative_task_refs() {
    let _guard = lock_test();
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
concurrent = [{ name = "validate-stack", task = "../froyo/validate" }]
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
    let _guard = lock_test();
    let root = temp_workspace("managed-shell-enabled");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
shell = true
concurrent = [{ name = "api", run = "printf api" }]
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
    let _guard = lock_test();
    let root = temp_workspace("managed-shell-global-override");
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        r#"[shell]
run = "exec ${SHELL:-/bin/bash} -i"

[tasks.dev]
mode = "tui"
shell = true
concurrent = [{ name = "api", run = "printf api" }]
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
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-runtime");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api-ok" },
  { name = "front", run = "printf front-ok" }
]
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
fn run_manifest_task_managed_stream_uses_named_profile_concurrent_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-runtime-profile-specific");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "default-only", run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ name = "front-only", run = "printf front-ok" }]
"#,
    );
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))]);

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["front".to_owned()],
        },
        root,
    )
    .expect("managed stream run with named profile");

    assert!(out.contains("Managed Task Runtime"));
    assert!(out.contains("profile: front"));
    assert!(out.contains("[front-only] front-ok"));
    assert!(out.contains("process `front-only` exit=0"));
    assert!(!out.contains("default-only"));
    assert!(!out.contains("default-ok"));
}

#[test]
fn run_manifest_task_managed_stream_errors_for_unknown_profile_with_available_profiles() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-unknown-profile");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "default-only", run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ name = "front-only", run = "printf front-ok" }]
"#,
    );
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))]);

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["admin".to_owned()],
        },
        root,
    )
    .expect_err("unknown managed profile should fail");

    match err {
        RunnerError::TaskManagedProfileNotFound {
            task,
            profile,
            available,
        } => {
            assert_eq!(task, "dev");
            assert_eq!(profile, "admin");
            assert_eq!(available, vec!["default".to_owned(), "front".to_owned()]);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_managed_stream_process_task_ref_supports_builtin_test() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-builtin-test-task-ref");
    let marker = root.join("builtin-test-called.log");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[test.suites]
unit = "sh -lc 'printf called > \"{}\"'"

[tasks.dev]
mode = "tui"
concurrent = [{{ name = "tests", task = "test" }}]
"#,
            marker.display()
        ),
    );
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))]);

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["default".to_owned()],
        },
        root,
    )
    .expect("run managed stream with builtin test task ref");

    assert!(out.contains("Managed Task Runtime"));
    assert!(out.contains("root: ok"));
    assert!(marker.exists(), "built-in test task ref should execute");
}

#[test]
fn run_manifest_task_managed_stream_process_task_ref_supports_builtin_test_with_inline_suite_arg() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-builtin-test-task-ref-inline-suite");
    let marker = root.join("builtin-test-called.log");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[test.suites]
vitest = "sh -lc 'printf called > \"{}\"'"

[tasks.dev]
mode = "tui"
concurrent = [{{ name = "tests", task = "test vitest" }}]
"#,
            marker.display()
        ),
    );
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))]);

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["default".to_owned()],
        },
        root,
    )
    .expect("run managed stream with builtin test task ref and suite arg");

    assert!(out.contains("Managed Task Runtime"));
    assert!(out.contains("root: ok"));
    assert!(
        marker.exists(),
        "built-in test task ref with suite arg should execute"
    );
}

#[test]
fn run_manifest_task_managed_stream_profile_entry_supports_builtin_test() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-builtin-test-profile-entry");
    let marker = root.join("builtin-test-called.log");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[test.suites]
unit = "sh -lc 'printf called > \"{}\"'"

[tasks.dev]
mode = "tui"
concurrent = [{{ name = "tests", task = "test" }}]
"#,
            marker.display()
        ),
    );
    let _env = EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))]);

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["default".to_owned()],
        },
        root,
    )
    .expect("run managed stream with builtin profile entry");

    assert!(out.contains("Managed Task Runtime"));
    assert!(out.contains("root: ok"));
    assert!(
        marker.exists(),
        "built-in test profile entry should execute"
    );
}

#[test]
fn run_manifest_task_managed_stream_fails_when_process_exits_non_zero_by_default() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-fail-on-non-zero-default");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "sh -lc 'exit 7'" }]
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
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-fail-on-non-zero-disabled");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
fail_on_non_zero = false
concurrent = [{ name = "api", run = "sh -lc 'exit 9'" }]
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

#[test]
fn run_manifest_task_rejects_live_lock_conflict() {
    let _guard = lock_test();
    let root = temp_workspace("lock-conflict-live");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
run = "sleep 1"
"#,
    );

    let root_for_thread = root.clone();
    let join = thread::spawn(move || {
        run_manifest_task_with_cwd(
            &TaskInvocation {
                name: "dev".to_owned(),
                args: Vec::new(),
            },
            root_for_thread,
        )
    });

    std::thread::sleep(Duration::from_millis(120));

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect_err("second run should conflict on lock");

    match err {
        RunnerError::TaskLockConflict {
            scope, remediation, ..
        } => {
            assert_eq!(scope, "workspace");
            assert!(remediation.contains("effigy unlock workspace"));
        }
        other => panic!("unexpected error: {other}"),
    }

    join.join()
        .expect("thread join")
        .expect("first run should complete");
}

#[test]
fn run_manifest_task_reclaims_stale_lock_from_dead_pid() {
    let _guard = lock_test();
    let root = temp_workspace("lock-stale-reclaim");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.dev]
run = "printf ok"
"#,
    );

    let locks_dir = root.join(".effigy/locks");
    fs::create_dir_all(&locks_dir).expect("create locks dir");
    fs::write(
        locks_dir.join("workspace.lock"),
        r#"{"scope":"workspace","pid":999999,"started_at_epoch_ms":0}"#,
    )
    .expect("write stale lock");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("stale lock should be reclaimed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_builtin_unlock_clears_explicit_scopes() {
    let _guard = lock_test();
    let root = temp_workspace("unlock-explicit-scopes");
    fs::create_dir_all(root.join(".effigy/locks")).expect("mkdir locks");
    fs::write(root.join(".effigy/locks/workspace.lock"), "{}").expect("write workspace lock");
    fs::write(root.join(".effigy/locks/task-dev.lock"), "{}").expect("write task lock");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "unlock".to_owned(),
            args: vec![
                "--repo".to_owned(),
                root.display().to_string(),
                "workspace".to_owned(),
                "task:dev".to_owned(),
            ],
        },
        root.clone(),
    )
    .expect("unlock should run");

    assert!(out.contains("removed: 2"));
    assert!(!root.join(".effigy/locks/workspace.lock").exists());
    assert!(!root.join(".effigy/locks/task-dev.lock").exists());
}
