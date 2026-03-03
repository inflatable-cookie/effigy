use super::*;

fn run_dev(root: &PathBuf, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root.clone(),
    )
}

fn run_dev_with_repo(root: &PathBuf, args: &[&str]) -> Result<String, RunnerError> {
    let mut full_args = vec!["--repo".to_owned(), root.display().to_string()];
    full_args.extend(args.iter().map(|arg| (*arg).to_owned()));
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "dev".to_owned(),
            args: full_args,
        },
        root.clone(),
    )
}

fn run_unlock_with_repo(root: &PathBuf, scopes: &[&str]) -> Result<String, RunnerError> {
    let mut args = vec!["--repo".to_owned(), root.display().to_string()];
    args.extend(scopes.iter().map(|scope| (*scope).to_owned()));
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "unlock".to_owned(),
            args,
        },
        root.clone(),
    )
}

fn run_task_with_repo(root: &PathBuf, name: &str, args: &[&str]) -> Result<String, RunnerError> {
    let mut full_args = vec!["--repo".to_owned(), root.display().to_string()];
    full_args.extend(args.iter().map(|arg| (*arg).to_owned()));
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: full_args,
        },
        root.clone(),
    )
}

fn managed_tui_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))])
}

fn managed_stream_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))])
}

fn write_catalogs_with_tasks(root: &PathBuf, catalogs: &[(&str, &[(&str, &str)])]) {
    for (name, tasks) in catalogs {
        let dir = create_workspace_dir(root, name);
        write_catalog_tasks(&dir, Some(name), tasks);
    }
}

fn write_managed_admin_profile_manifest(root: &PathBuf) {
    write_root_manifest(
        root,
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
}

fn write_managed_stream_builtin_test_manifest(
    root: &PathBuf,
    suite: &str,
    test_task_ref: &str,
    marker: &PathBuf,
) {
    write_root_manifest(
        root,
        &format!(
            r#"[test.suites]
{} = "sh -lc 'printf called > \"{}\"'"

[tasks.dev]
mode = "tui"
concurrent = [{{ name = "tests", task = "{}" }}]
"#,
            suite,
            marker.display(),
            test_task_ref
        ),
    );
}

fn write_managed_stream_profile_manifest(root: &PathBuf) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "default-only", run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ name = "front-only", run = "printf front-ok" }]
"#,
    );
}

fn assert_run_dev_with_repo_contains(root: &PathBuf, args: &[&str], expected: &[&str]) {
    let out = run_dev_with_repo(root, args).expect("managed plan should render");
    assert_contains_all(&out, expected);
}

struct ManagedPlanCase {
    workspace: &'static str,
}

struct ManagedStreamBuiltinTestCase {
    workspace: &'static str,
    suite: &'static str,
    task_ref: &'static str,
}

struct ManagedTaskRefInvalidCase {
    workspace: &'static str,
    manifest: &'static str,
    expected_reference: &'static str,
    expected_detail: &'static str,
}

fn assert_managed_process_invalid_definition(
    err: RunnerError,
    expected_task: &str,
    expected_process: &str,
    expected_detail_substring: Option<&str>,
) {
    match err {
        RunnerError::TaskManagedProcessInvalidDefinition {
            task,
            process,
            detail,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(process, expected_process);
            if let Some(detail_substring) = expected_detail_substring {
                assert!(detail.contains(detail_substring));
            }
        }
        other => panic!("unexpected error: {other}"),
    }
}

fn assert_managed_profile_not_found(
    err: RunnerError,
    expected_task: &str,
    expected_profile: &str,
    expected_available: &[&str],
) {
    match err {
        RunnerError::TaskManagedProfileNotFound {
            task,
            profile,
            available,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(profile, expected_profile);
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

fn assert_managed_task_reference_invalid(
    err: RunnerError,
    expected_task: &str,
    expected_process: &str,
    expected_reference: &str,
    expected_detail_substring: &str,
) {
    match err {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task,
            process,
            reference,
            detail,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(process, expected_process);
            assert_eq!(reference, expected_reference);
            assert!(detail.contains(expected_detail_substring));
        }
        other => panic!("unexpected error: {other}"),
    }
}

fn assert_managed_non_zero_exit(
    err: RunnerError,
    expected_task: &str,
    expected_profile: &str,
    expected_processes: &[(&str, &str)],
) {
    match err {
        RunnerError::TaskManagedNonZeroExit {
            task,
            profile,
            processes,
        } => {
            assert_eq!(task, expected_task);
            assert_eq!(profile, expected_profile);
            assert_eq!(
                processes,
                expected_processes
                    .iter()
                    .map(|(process, exit)| ((*process).to_owned(), (*exit).to_owned()))
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

#[test]
fn run_manifest_task_managed_tui_uses_default_profile_when_not_specified() {
    let _guard = lock_test();
    let root = temp_workspace("managed-default-profile");
    let _env = managed_tui_env();
    write_managed_admin_profile_manifest(&root);

    let out = run_dev_with_repo(&root, &[]).expect("managed plan should render");
    assert_contains_all(
        &out,
        &[
            "Managed Task Plan",
            "profile: default",
            "api",
            "front",
            "admin",
            "fail-on-non-zero: enabled",
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_accepts_named_profile_argument() {
    let _guard = lock_test();
    let root = temp_workspace("managed-named-profile");
    let _env = managed_tui_env();
    write_managed_admin_profile_manifest(&root);

    let out = run_dev(&root, &["admin"]).expect("managed plan should render");
    assert_contains_all(&out, &["profile: admin", "api", "admin"]);
    assert!(!out.contains("front"));
}

#[test]
fn run_manifest_task_managed_tui_supports_concurrent_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-concurrent-entries");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
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

    assert_run_dev_with_repo_contains(
        &root,
        &[],
        &[
            "Managed Task Plan",
            "profile: default",
            "tab-order: front, process-2, api",
            "printf api",
            "printf background",
            "printf front",
            "250",
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_rejects_concurrent_entry_with_both_task_and_run() {
    let root = temp_workspace("managed-concurrent-invalid-entry");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { task = "api", run = "printf oops", start = 1, tab = 1 }
]

[tasks.api]
run = "printf api"
"#,
    );

    let err = run_dev_with_repo(&root, &[]).expect_err("invalid concurrent entry should fail");
    assert_managed_process_invalid_definition(err, "dev", "api", Some("either `task` or `run`"));
}

#[test]
fn run_manifest_task_managed_tui_supports_profile_specific_concurrent_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-concurrent-profile-specific");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
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

    let out_default = run_dev_with_repo(&root, &[]).expect("default managed plan should render");
    assert_contains_all(
        &out_default,
        &["profile: default", "default-api", "default-front"],
    );
    assert!(!out_default.contains("admin-api"));

    let out_admin = run_dev(&root, &["admin"]).expect("admin managed plan should render");
    assert_contains_all(&out_admin, &["profile: admin", "admin-api"]);
    assert!(!out_admin.contains("default-front"));
}

#[test]
fn run_manifest_task_managed_tui_supports_ranked_tab_order_map() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedPlanCase {
            workspace: "managed-tab-order",
        },
        ManagedPlanCase {
            workspace: "managed-tab-order-ranked",
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_root_manifest(
            &root,
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

        let out = run_dev_with_repo(&root, &[]).expect("managed plan should render");
        assert_contains_all(&out, &["tab-order: dairy, cream, api, jobs"]);
    }
}

#[test]
fn run_manifest_task_managed_tui_supports_ranked_tab_order_map_with_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-tab-order-ranked-refs");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
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
    write_catalogs_with_tasks(
        &root,
        &[
            (
                "farmyard",
                &[
                    ("api", "printf farmyard-api"),
                    ("jobs", "printf farmyard-jobs"),
                ],
            ),
            ("cream", &[("dev", "printf cream-dev")]),
            ("dairy", &[("dev", "printf dairy-dev")]),
        ],
    );

    assert_run_dev_with_repo_contains(
        &root,
        &[],
        &["tab-order: dairy/dev, cream/dev, farmyard/api, farmyard/jobs"],
    );
}

#[test]
fn run_manifest_task_managed_tui_supports_single_definition_ordered_profile_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-single-definition-ordered-profile");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
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
    write_catalogs_with_tasks(
        &root,
        &[
            (
                "farmyard",
                &[
                    ("api", "printf farmyard-api"),
                    ("jobs", "printf farmyard-jobs"),
                ],
            ),
            ("cream", &[("dev", "printf cream-dev")]),
            ("dairy", &[("dev", "printf dairy-dev")]),
        ],
    );

    assert_run_dev_with_repo_contains(
        &root,
        &[],
        &[
            "tab-order: dairy/dev, cream/dev, farmyard/api, farmyard/jobs",
            "start-after-ms",
            "1200",
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_errors_when_concurrent_entry_missing_task_and_run() {
    let root = temp_workspace("managed-tab-order-invalid");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "jobs" }]
"#,
    );

    let err = run_dev_with_repo(&root, &[]).expect_err("invalid concurrent entry should fail");
    assert_managed_process_invalid_definition(
        err,
        "dev",
        "jobs",
        Some("missing both `task` and `run`"),
    );
}

#[test]
fn run_manifest_task_managed_tui_errors_for_unknown_profile() {
    let root = temp_workspace("managed-unknown-profile");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "cargo run -p api" }]
"#,
    );

    let err = run_dev(&root, &["admin"]).expect_err("unknown profile should fail");
    assert_managed_profile_not_found(err, "dev", "admin", &["default"]);
}

#[test]
fn run_manifest_task_managed_tui_processes_can_reference_other_tasks() {
    let _guard = lock_test();
    let root = temp_workspace("managed-task-refs");
    let _env = managed_tui_env();
    let farmyard = root.join("farmyard");
    let cream = root.join("cream");

    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", task = "farmyard/api" },
  { name = "front", task = "cream/dev" }
]
"#,
    );
    write_catalogs_with_tasks(
        &root,
        &[
            ("farmyard", &[("api", "printf farmyard-api")]),
            ("cream", &[("dev", "printf cream-dev")]),
        ],
    );

    let out = run_dev(&root, &[]).expect("managed plan should render");

    assert_contains_all(
        &out,
        &[
            "farmyard-api",
            "cream-dev",
            &farmyard.display().to_string(),
            &cream.display().to_string(),
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_errors_when_process_has_run_and_task() {
    let root = temp_workspace("managed-invalid-process-def");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "printf api", task = "api" }]
"#,
    );

    let err = run_dev_with_repo(&root, &[]).expect_err("invalid process definition should fail");
    assert_managed_process_invalid_definition(err, "dev", "api", None);
}

#[test]
fn run_manifest_task_managed_tui_supports_compact_profile_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-compact-profile-refs");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ task = "farmyard/api" }, { task = "cream/dev" }]

[tasks.dev.profiles.admin]
concurrent = [{ task = "farmyard/api" }]
"#,
    );
    write_catalogs_with_tasks(
        &root,
        &[
            ("farmyard", &[("api", "printf farmyard-api")]),
            ("cream", &[("dev", "printf cream-dev")]),
        ],
    );

    let out = run_dev_with_repo(&root, &[]).expect("managed compact plan should render");
    assert_contains_all(
        &out,
        &[
            "profile: default",
            "farmyard-api",
            "cream-dev",
            "farmyard/api",
            "cream/dev",
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_process_run_array_supports_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-process-run-array");
    let farmyard = create_workspace_dir(&root, "farmyard");
    let _env = managed_tui_env();

    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "combo", task = "combo" }]

[tasks.combo]
run = ["printf start", { task = "farmyard/api" }, "printf done"]
"#,
    );
    write_catalog_tasks(
        &farmyard,
        Some("farmyard"),
        &[("api", "printf farmyard-api")],
    );

    let out = run_dev_with_repo(&root, &[]).expect("managed plan should render");
    assert_contains_all(&out, &["printf start", "farmyard-api", "printf done", "cd"]);
}

#[test]
fn run_manifest_task_managed_tui_rejects_invalid_task_ref_syntax() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedTaskRefInvalidCase {
            workspace: "managed-compact-profile-ref-unterminated-quote",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = 'test "unterminated' }]
"#,
            expected_reference: "test \"unterminated",
            expected_detail: "unterminated quote",
        },
        ManagedTaskRefInvalidCase {
            workspace: "managed-process-task-ref-unterminated-quote",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = 'test "unterminated' }]
"#,
            expected_reference: "test \"unterminated",
            expected_detail: "unterminated quote",
        },
        ManagedTaskRefInvalidCase {
            workspace: "managed-process-task-ref-trailing-escape",
            manifest: r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "tests", task = "test vitest \\" }]
"#,
            expected_reference: "test vitest \\",
            expected_detail: "trailing escape",
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_manifest(&root.join("effigy.toml"), case.manifest);
        let err = run_dev_with_repo(&root, &[]).expect_err("invalid process task ref should fail");
        assert_managed_task_reference_invalid(
            err,
            "dev",
            "tests",
            case.expected_reference,
            case.expected_detail,
        );
    }
}

#[test]
fn run_manifest_task_managed_tui_supports_relative_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-relative-task-ref");
    let dairy = create_workspace_dir(&root, "dairy");
    let froyo = root.join("froyo");
    let _env = managed_tui_env();

    write_manifest(
        &dairy.join("effigy.toml"),
        r#"[catalog]
alias = "dairy"
[tasks.dev]
mode = "tui"
concurrent = [{ name = "validate-stack", task = "../froyo/validate" }]
"#,
    );
    write_catalogs_with_tasks(
        &root,
        &[("froyo", &[("validate", "printf froyo-validate")])],
    );

    let out = run_task_with_repo(&root, "dairy/dev", &[]).expect("managed plan should render");
    assert_contains_all(
        &out,
        &[
            "validate-stack",
            "froyo-validate",
            &froyo.display().to_string(),
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_appends_shell_process_when_enabled() {
    let _guard = lock_test();
    let root = temp_workspace("managed-shell-enabled");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
shell = true
concurrent = [{ name = "api", run = "printf api" }]
"#,
    );

    let out = run_dev(&root, &[]).expect("managed plan should include shell process");
    assert_contains_all(&out, &["shell", "exec ${SHELL:-/bin/zsh} -i"]);
}

#[test]
fn run_manifest_task_managed_tui_uses_global_shell_run_override() {
    let _guard = lock_test();
    let root = temp_workspace("managed-shell-global-override");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[shell]
run = "exec ${SHELL:-/bin/bash} -i"

[tasks.dev]
mode = "tui"
shell = true
concurrent = [{ name = "api", run = "printf api" }]
"#,
    );

    let out = run_dev(&root, &[]).expect("managed plan should include configured shell process");
    assert_contains_all(&out, &["shell", "exec ${SHELL:-/bin/bash} -i"]);
}

#[test]
fn run_manifest_task_managed_stream_executes_selected_profile_processes() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-runtime");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api-ok" },
  { name = "front", run = "printf front-ok" }
]
"#,
    );
    let _env = managed_stream_env();

    let out = run_dev(&root, &[]).expect("managed stream run");
    assert_contains_all(
        &out,
        &[
            "Managed Task Runtime",
            "[api] api-ok",
            "[front] front-ok",
            "fail-on-non-zero: enabled",
            "process `api` exit=0",
            "process `front` exit=0",
        ],
    );
}

#[test]
fn run_manifest_task_managed_stream_uses_named_profile_concurrent_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-runtime-profile-specific");
    write_managed_stream_profile_manifest(&root);
    let _env = managed_stream_env();

    let out = run_dev(&root, &["front"]).expect("managed stream run with named profile");
    assert_contains_all(
        &out,
        &[
            "Managed Task Runtime",
            "profile: front",
            "[front-only] front-ok",
            "process `front-only` exit=0",
        ],
    );
    assert!(!out.contains("default-only"));
    assert!(!out.contains("default-ok"));
}

#[test]
fn run_manifest_task_managed_stream_errors_for_unknown_profile_with_available_profiles() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-unknown-profile");
    write_managed_stream_profile_manifest(&root);
    let _env = managed_stream_env();

    let err = run_dev(&root, &["admin"]).expect_err("unknown managed profile should fail");
    assert_managed_profile_not_found(err, "dev", "admin", &["default", "front"]);
}

#[test]
fn run_manifest_task_managed_stream_profile_entry_supports_builtin_test() {
    let _guard = lock_test();
    let _env = managed_stream_env();
    let cases = [
        ManagedStreamBuiltinTestCase {
            workspace: "managed-stream-builtin-test-task-ref",
            suite: "unit",
            task_ref: "test",
        },
        ManagedStreamBuiltinTestCase {
            workspace: "managed-stream-builtin-test-task-ref-inline-suite",
            suite: "vitest",
            task_ref: "test vitest",
        },
        ManagedStreamBuiltinTestCase {
            workspace: "managed-stream-builtin-test-profile-entry",
            suite: "unit",
            task_ref: "test",
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        let marker = root.join("builtin-test-called.log");
        write_managed_stream_builtin_test_manifest(&root, case.suite, case.task_ref, &marker);

        let out =
            run_dev(&root, &["default"]).expect("run managed stream with builtin profile entry");
        assert_contains_all(&out, &["Managed Task Runtime", "root: ok"]);
        assert!(marker.exists(), "built-in test task ref should execute");
    }
}

#[test]
fn run_manifest_task_managed_stream_fails_when_process_exits_non_zero_by_default() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-fail-on-non-zero-default");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "sh -lc 'exit 7'" }]
"#,
    );
    let _env = managed_stream_env();

    let err =
        run_dev(&root, &[]).expect_err("managed stream should fail for non-zero exit by default");
    assert_managed_non_zero_exit(err, "dev", "default", &[("api", "exit=7")]);
}

#[test]
fn run_manifest_task_managed_stream_allows_non_zero_when_disabled() {
    let _guard = lock_test();
    let root = temp_workspace("managed-stream-fail-on-non-zero-disabled");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
fail_on_non_zero = false
concurrent = [{ name = "api", run = "sh -lc 'exit 9'" }]
"#,
    );
    let _env = managed_stream_env();

    let out = run_dev(&root, &[]).expect("managed stream should allow non-zero when disabled");
    assert_contains_all(
        &out,
        &[
            "Managed Task Runtime",
            "fail-on-non-zero: disabled",
            "process `api` exit=9",
        ],
    );
}

#[test]
fn run_manifest_task_rejects_live_lock_conflict() {
    let _guard = lock_test();
    let root = temp_workspace("lock-conflict-live");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "sleep 1"
"#,
    );

    let root_for_thread = root.clone();
    let join = thread::spawn(move || run_dev(&root_for_thread, &[]));

    std::thread::sleep(Duration::from_millis(120));

    let err = run_dev(&root, &[]).expect_err("second run should conflict on lock");
    assert_lock_conflict(err, "workspace", "effigy unlock workspace");

    join.join()
        .expect("thread join")
        .expect("first run should complete");
}

#[test]
fn run_manifest_task_reclaims_stale_lock_from_dead_pid() {
    let _guard = lock_test();
    let root = temp_workspace("lock-stale-reclaim");
    write_root_manifest(
        &root,
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

    let out = run_dev(&root, &[]).expect("stale lock should be reclaimed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_builtin_unlock_clears_explicit_scopes() {
    let _guard = lock_test();
    let root = temp_workspace("unlock-explicit-scopes");
    fs::create_dir_all(root.join(".effigy/locks")).expect("mkdir locks");
    fs::write(root.join(".effigy/locks/workspace.lock"), "{}").expect("write workspace lock");
    fs::write(root.join(".effigy/locks/task-dev.lock"), "{}").expect("write task lock");

    let out = run_unlock_with_repo(&root, &["workspace", "task:dev"]).expect("unlock should run");
    assert_contains_all(&out, &["removed: 2"]);
    assert!(!root.join(".effigy/locks/workspace.lock").exists());
    assert!(!root.join(".effigy/locks/task-dev.lock").exists());
}
