use super::*;

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

    let out = run_builtin_ok(root, "validate", &["--verbose-root"]);
    assert_contains_all(&out, &["printf lint-ok", "printf validate-ok"]);
}

#[test]
fn run_manifest_task_run_array_accepts_dag_metadata() {
    let root = temp_workspace("run-array-dag-metadata");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok" },
  { id = "build", run = "printf build-ok", depends_on = ["lint"] },
  { run = "printf validate-ok" }
]
"#,
    );

    let out = run_builtin_ok(root, "validate", &["--verbose-root"]);
    assert_contains_all(
        &out,
        &["printf lint-ok", "printf build-ok", "printf validate-ok"],
    );
}

#[test]
fn run_manifest_task_run_array_rejects_depends_on_without_id() {
    let root = temp_workspace("run-array-depends-on-without-id");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok" },
  { run = "printf build-ok", depends_on = ["lint"] }
]
"#,
    );

    let err = run_builtin_err(root, "validate", &[]);
    assert_task_invocation_error_contains(
        err,
        &["defines `depends_on` but is missing a non-empty `id`"],
    );
}

#[test]
fn run_manifest_task_run_array_rejects_missing_dependency_step() {
    let root = temp_workspace("run-array-missing-dependency-step");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [
  { id = "build", run = "printf build-ok", depends_on = ["lint"] }
]
"#,
    );

    let err = run_builtin_err(root, "validate", &[]);
    assert_task_invocation_error_contains(err, &["depends on missing step `lint`"]);
}

#[test]
fn run_manifest_task_run_array_rejects_duplicate_step_ids() {
    let root = temp_workspace("run-array-duplicate-step-ids");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok" },
  { id = "lint", run = "printf lint-again" }
]
"#,
    );

    let err = run_builtin_err(root, "validate", &[]);
    assert_task_invocation_error_contains(err, &["duplicate step id `lint`"]);
}

#[test]
fn run_manifest_task_run_array_rejects_dependency_cycles() {
    let root = temp_workspace("run-array-dependency-cycles");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok", depends_on = ["build"] },
  { id = "build", run = "printf build-ok", depends_on = ["lint"] }
]
"#,
    );

    let err = run_builtin_err(root, "validate", &[]);

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("contains dependency cycle"));
            assert!(
                message.contains("build -> lint -> build")
                    || message.contains("lint -> build -> lint")
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_run_array_rejects_self_dependency_cycle() {
    let root = temp_workspace("run-array-self-dependency-cycle");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok", depends_on = ["lint"] }
]
"#,
    );

    let err = run_builtin_err(root, "validate", &[]);
    assert_task_invocation_error_contains(err, &["cannot depend on itself"]);
}

#[test]
fn run_manifest_task_run_array_executes_ready_steps_in_parallel() {
    let _guard = lock_test();
    let root = temp_workspace("run-array-parallel-ready-steps");
    let marker = root.join("parallel-ready.log");
    let _env = EnvGuard::set_many(&[("EFFIGY_DAG_MAX_PARALLEL", Some("2".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks.validate]
run = [
  {{ id = "seed", run = "echo seed > \"{}\"" }},
  {{ id = "a", run = "sh -lc 'echo a-start >> \"{}\"; sleep 0.8; echo a-end >> \"{}\"'", depends_on = ["seed"] }},
  {{ id = "b", run = "sh -lc 'echo b-start >> \"{}\"; sleep 0.8; echo b-end >> \"{}\"'", depends_on = ["seed"] }}
]
"#,
            marker.display(),
            marker.display(),
            marker.display(),
            marker.display(),
            marker.display()
        ),
    );

    let _ = run_builtin_ok(root.clone(), "validate", &[]);

    let body = fs::read_to_string(marker).expect("read marker");
    let lines: Vec<&str> = body.lines().collect();
    assert!(
        lines.contains(&"a-start")
            && lines.contains(&"b-start")
            && lines.contains(&"a-end")
            && lines.contains(&"b-end"),
        "expected start/end markers in log: {lines:?}"
    );
    let first_end_idx = lines
        .iter()
        .position(|line| *line == "a-end" || *line == "b-end")
        .expect("expected end marker");
    let starts_before_end = lines[..first_end_idx]
        .iter()
        .filter(|line| **line == "a-start" || **line == "b-start")
        .count();
    assert_eq!(
        starts_before_end, 2,
        "expected both ready steps to start before first completion, lines={lines:?}"
    );
}

#[test]
fn run_manifest_task_run_array_honors_parallel_cap() {
    let _guard = lock_test();
    let root = temp_workspace("run-array-parallel-cap");
    let marker = root.join("parallel-cap.log");
    let _env = EnvGuard::set_many(&[("EFFIGY_DAG_MAX_PARALLEL", Some("1".to_owned()))]);
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks.validate]
run = [
  {{ id = "seed", run = "printf seed > \"{}\"" }},
  {{ id = "a", run = "sh -lc 'sleep 0.8; printf a >> \"{}\"'", depends_on = ["seed"] }},
  {{ id = "b", run = "sh -lc 'sleep 0.8; printf b >> \"{}\"'", depends_on = ["seed"] }}
]
"#,
            marker.display(),
            marker.display(),
            marker.display()
        ),
    );

    let start = Instant::now();
    let _ = run_builtin_ok(root.clone(), "validate", &[]);
    let elapsed = start.elapsed();

    let body = fs::read_to_string(marker).expect("read marker");
    assert!(body.contains('a'));
    assert!(body.contains('b'));
    assert!(
        elapsed >= Duration::from_millis(1400),
        "expected capped schedule, elapsed={elapsed:?}"
    );
}

#[test]
fn run_manifest_task_run_array_retries_failing_step() {
    let root = temp_workspace("run-array-retry-step");
    let marker = root.join("retry.marker");
    let out_file = root.join("retry.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks.validate]
run = [
  {{ id = "flaky", run = "sh -lc 'if [ -f \"{}\" ]; then printf ok > \"{}\"; exit 0; else touch \"{}\"; exit 7; fi'", retry = 1, retry_delay_ms = 10 }}
]
"#,
            marker.display(),
            out_file.display(),
            marker.display()
        ),
    );

    let out = run_builtin_ok(root, "validate", &[]);
    assert_eq!(out, "");
    let body = fs::read_to_string(out_file).expect("read retry output");
    assert_eq!(body, "ok");
}

#[test]
fn run_manifest_task_run_array_enforces_timeout_ms() {
    let root = temp_workspace("run-array-timeout-step");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [
  { id = "slow", run = "sleep 1", timeout_ms = 100 }
]
"#,
    );

    let err = run_builtin_err(root, "validate", &[]);

    match err {
        RunnerError::TaskCommandFailure { code, .. } => {
            assert_eq!(code, Some(124));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_run_array_fail_fast_false_allows_other_ready_steps() {
    let _guard = lock_test();
    let root = temp_workspace("run-array-fail-fast-false");
    let marker = root.join("fail-fast-false.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks.validate]
run = [
  {{ id = "seed", run = "printf seed > \"{}\"" }},
  {{ id = "bad", run = "sh -lc 'sleep 0.1; exit 9'", depends_on = ["seed"], fail_fast = false }},
  {{ id = "good", run = "printf good >> \"{}\"", depends_on = ["seed"] }}
]
"#,
            marker.display(),
            marker.display()
        ),
    );

    let err = run_builtin_err(root.clone(), "validate", &[]);

    match err {
        RunnerError::TaskCommandFailure { .. } => {}
        other => panic!("unexpected error: {other}"),
    }
    let body = fs::read_to_string(marker).expect("read marker");
    assert!(body.contains("good"));
}

#[test]
fn run_manifest_task_run_array_task_reference_supports_inline_args() {
    let root = temp_workspace("run-array-task-ref-inline-args");
    let marker = root.join("task-ref-inline-args.log");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks.capture]
run = "sh -lc 'printf %s \"$1\" > \"{}\"' sh {{args}}"

[tasks.validate]
run = [{{ task = "capture hello-world" }}]
"#,
            marker.display()
        ),
    );

    let out = run_builtin_ok(root, "validate", &[]);

    assert_eq!(out, "");
    let body = fs::read_to_string(&marker).expect("read marker");
    assert_eq!(body, "hello-world");
}

#[test]
fn run_manifest_task_run_array_task_reference_supports_quoted_inline_args() {
    let root = temp_workspace("run-array-task-ref-quoted-inline-args");
    let marker = root.join("task-ref-quoted-inline-args.log");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks.capture]
run = "sh -lc 'printf \"%s|%s\" \"$1\" \"$2\" > \"{}\"' sh {{args}}"

[tasks.validate]
run = [{{ task = 'capture alpha "two words"' }}]
"#,
            marker.display()
        ),
    );

    let out = run_builtin_ok(root, "validate", &[]);

    assert_eq!(out, "");
    let body = fs::read_to_string(&marker).expect("read marker");
    assert_eq!(body, "alpha|two words");
}

#[test]
fn run_manifest_task_run_array_task_reference_rejects_unterminated_quoted_args() {
    let root = temp_workspace("run-array-task-ref-unterminated-quote");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [{ task = 'test "unterminated' }]
"#,
    );

    let err = run_builtin_err(root, "validate", &[]);
    assert_task_invocation_error_contains(err, &["run step task ref", "unterminated quote"]);
}

#[test]
fn run_manifest_task_run_array_task_reference_rejects_trailing_escape() {
    let root = temp_workspace("run-array-task-ref-trailing-escape");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [{ task = "test vitest \\" }]
"#,
    );

    let err = run_builtin_err(root, "validate", &[]);
    assert_task_invocation_error_contains(err, &["run step task ref", "trailing escape"]);
}

#[test]
fn run_manifest_task_run_array_supports_builtin_test_task_reference_steps() {
    let root = temp_workspace("run-array-builtin-test-task-ref");
    let marker = root.join("builtin-test-called.log");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[test.suites]
unit = "sh -lc 'printf called > \"{}\"'"

[tasks.validate]
run = [{{ task = "test" }}, "printf validate-ok"]
"#,
            marker.display()
        ),
    );

    let out = run_builtin_ok(root.clone(), "validate", &["--verbose-root"]);
    assert_contains_all(&out, &["validate-ok"]);
    assert!(marker.exists(), "built-in test task ref should execute");
}

#[test]
fn run_manifest_task_run_array_supports_builtin_test_task_reference_with_inline_suite_arg() {
    let root = temp_workspace("run-array-builtin-test-task-ref-inline-suite");
    let marker = root.join("builtin-test-called.log");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[test.suites]
vitest = "sh -lc 'printf called > \"{}\"'"

[tasks.validate]
run = [{{ task = "test vitest" }}, "printf validate-ok"]
"#,
            marker.display()
        ),
    );

    let out = run_builtin_ok(root.clone(), "validate", &["--verbose-root"]);
    assert_contains_all(&out, &["validate-ok"]);
    assert!(
        marker.exists(),
        "built-in test task ref with suite arg should execute"
    );
}

#[test]
fn run_manifest_task_run_array_supports_prefixed_builtin_test_task_reference_steps() {
    let root = temp_workspace("run-array-prefixed-builtin-test-task-ref");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    let marker = farmyard.join("builtin-test-called.log");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [{ task = "farmyard/test" }, "printf validate-ok"]
"#,
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "farmyard"
[test.suites]
unit = "sh -lc 'printf called > \"{}\"'"
"#,
            marker.display()
        ),
    );

    let out = run_builtin_ok(root.clone(), "validate", &["--verbose-root"]);
    assert_contains_all(&out, &["validate-ok"]);
    assert!(
        marker.exists(),
        "prefixed built-in test task ref should execute"
    );
}
