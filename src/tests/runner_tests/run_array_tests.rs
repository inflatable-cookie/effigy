use super::*;

fn run_validate_ok(root: &PathBuf, args: &[&str]) -> String {
    run_builtin_ok(root.clone(), "validate", args)
}

fn run_validate_err(root: &PathBuf, args: &[&str]) -> RunnerError {
    run_builtin_err(root.clone(), "validate", args)
}

fn assert_task_command_failure(err: RunnerError, expected_code: Option<Option<i32>>) {
    match err {
        RunnerError::TaskCommandFailure { code, .. } => {
            if let Some(expected) = expected_code {
                assert_eq!(code, expected);
            }
        }
        other => panic!("unexpected error: {other}"),
    }
}

struct RunArrayInvocationErrorCase {
    workspace: &'static str,
    manifest: &'static str,
    expected: &'static [&'static str],
}

struct RunArrayTaskRefParseErrorCase {
    workspace: &'static str,
    manifest: &'static str,
    expected_tail: &'static str,
}

struct BuiltinTestTaskRefCase {
    workspace: &'static str,
    suite_name: &'static str,
    task_ref: &'static str,
}

fn write_validate_manifest(root: &PathBuf, body: &str) {
    write_manifest(&root.join("effigy.toml"), body);
}

#[test]
fn run_manifest_task_run_array_supports_task_reference_steps() {
    let root = temp_workspace("run-array-task-refs");
    write_validate_manifest(
        &root,
        r#"[tasks.lint]
run = "printf lint-ok"

[tasks.validate]
run = [{ task = "lint" }, "printf validate-ok"]
"#,
    );

    let out = run_validate_ok(&root, &["--verbose-root"]);
    assert_contains_all(&out, &["printf lint-ok", "printf validate-ok"]);
}

#[test]
fn run_manifest_task_run_array_accepts_dag_metadata() {
    let root = temp_workspace("run-array-dag-metadata");
    write_validate_manifest(
        &root,
        r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok" },
  { id = "build", run = "printf build-ok", depends_on = ["lint"] },
  { run = "printf validate-ok" }
]
"#,
    );

    let out = run_validate_ok(&root, &["--verbose-root"]);
    assert_contains_all(
        &out,
        &["printf lint-ok", "printf build-ok", "printf validate-ok"],
    );
}

#[test]
fn run_manifest_task_run_array_rejects_invalid_dag_metadata() {
    let cases = [
        RunArrayInvocationErrorCase {
            workspace: "run-array-depends-on-without-id",
            manifest: "[tasks.validate]\nrun = [\n  { id = \"lint\", run = \"printf lint-ok\" },\n  { run = \"printf build-ok\", depends_on = [\"lint\"] }\n]\n",
            expected: &["defines `depends_on` but is missing a non-empty `id`"],
        },
        RunArrayInvocationErrorCase {
            workspace: "run-array-missing-dependency-step",
            manifest: "[tasks.validate]\nrun = [\n  { id = \"build\", run = \"printf build-ok\", depends_on = [\"lint\"] }\n]\n",
            expected: &["depends on missing step `lint`"],
        },
        RunArrayInvocationErrorCase {
            workspace: "run-array-duplicate-step-ids",
            manifest: "[tasks.validate]\nrun = [\n  { id = \"lint\", run = \"printf lint-ok\" },\n  { id = \"lint\", run = \"printf lint-again\" }\n]\n",
            expected: &["duplicate step id `lint`"],
        },
        RunArrayInvocationErrorCase {
            workspace: "run-array-self-dependency-cycle",
            manifest: "[tasks.validate]\nrun = [\n  { id = \"lint\", run = \"printf lint-ok\", depends_on = [\"lint\"] }\n]\n",
            expected: &["cannot depend on itself"],
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_validate_manifest(&root, case.manifest);
        let err = run_validate_err(&root, &[]);
        assert_task_invocation_error_contains(err, case.expected);
    }
}

#[test]
fn run_manifest_task_run_array_rejects_dependency_cycles() {
    let root = temp_workspace("run-array-dependency-cycles");
    write_validate_manifest(
        &root,
        r#"[tasks.validate]
run = [
  { id = "lint", run = "printf lint-ok", depends_on = ["build"] },
  { id = "build", run = "printf build-ok", depends_on = ["lint"] }
]
"#,
    );

    let err = run_validate_err(&root, &[]);

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
fn run_manifest_task_run_array_executes_ready_steps_in_parallel() {
    let _guard = lock_test();
    let root = temp_workspace("run-array-parallel-ready-steps");
    let marker = root.join("parallel-ready.log");
    let _env = EnvGuard::set_many(&[("EFFIGY_DAG_MAX_PARALLEL", Some("2".to_owned()))]);
    write_validate_manifest(
        &root,
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

    let _ = run_validate_ok(&root, &[]);

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
    write_validate_manifest(
        &root,
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
    let _ = run_validate_ok(&root, &[]);
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
    write_validate_manifest(
        &root,
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

    let out = run_validate_ok(&root, &[]);
    assert_eq!(out, "");
    let body = fs::read_to_string(out_file).expect("read retry output");
    assert_eq!(body, "ok");
}

#[test]
fn run_manifest_task_run_array_enforces_timeout_ms() {
    let root = temp_workspace("run-array-timeout-step");
    write_validate_manifest(
        &root,
        r#"[tasks.validate]
run = [
  { id = "slow", run = "sleep 1", timeout_ms = 100 }
]
"#,
    );

    let err = run_validate_err(&root, &[]);
    assert_task_command_failure(err, Some(Some(124)));
}

#[test]
fn run_manifest_task_run_array_fail_fast_false_allows_other_ready_steps() {
    let _guard = lock_test();
    let root = temp_workspace("run-array-fail-fast-false");
    let marker = root.join("fail-fast-false.out");
    write_validate_manifest(
        &root,
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

    let err = run_validate_err(&root, &[]);
    assert_task_command_failure(err, None);
    let body = fs::read_to_string(marker).expect("read marker");
    assert!(body.contains("good"));
}

#[test]
fn run_manifest_task_run_array_task_reference_supports_inline_args() {
    let root = temp_workspace("run-array-task-ref-inline-args");
    let marker = root.join("task-ref-inline-args.log");
    write_validate_manifest(
        &root,
        &format!(
            r#"[tasks.capture]
run = "sh -lc 'printf %s \"$1\" > \"{}\"' sh {{args}}"

[tasks.validate]
run = [{{ task = "capture hello-world" }}]
"#,
            marker.display()
        ),
    );

    let out = run_validate_ok(&root, &[]);

    assert_eq!(out, "");
    let body = fs::read_to_string(&marker).expect("read marker");
    assert_eq!(body, "hello-world");
}

#[test]
fn run_manifest_task_run_array_task_reference_supports_quoted_inline_args() {
    let root = temp_workspace("run-array-task-ref-quoted-inline-args");
    let marker = root.join("task-ref-quoted-inline-args.log");
    write_validate_manifest(
        &root,
        &format!(
            r#"[tasks.capture]
run = "sh -lc 'printf \"%s|%s\" \"$1\" \"$2\" > \"{}\"' sh {{args}}"

[tasks.validate]
run = [{{ task = 'capture alpha "two words"' }}]
"#,
            marker.display()
        ),
    );

    let out = run_validate_ok(&root, &[]);

    assert_eq!(out, "");
    let body = fs::read_to_string(&marker).expect("read marker");
    assert_eq!(body, "alpha|two words");
}

#[test]
fn run_manifest_task_run_array_task_reference_rejects_invalid_inline_args() {
    let cases = [
        RunArrayTaskRefParseErrorCase {
            workspace: "run-array-task-ref-unterminated-quote",
            manifest: "[tasks.validate]\nrun = [{ task = 'test \"unterminated' }]\n",
            expected_tail: "unterminated quote",
        },
        RunArrayTaskRefParseErrorCase {
            workspace: "run-array-task-ref-trailing-escape",
            manifest: "[tasks.validate]\nrun = [{ task = \"test vitest \\\\\" }]\n",
            expected_tail: "trailing escape",
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_validate_manifest(&root, case.manifest);
        let err = run_validate_err(&root, &[]);
        assert_task_invocation_error_contains(err, &["run step task ref", case.expected_tail]);
    }
}

#[test]
fn run_manifest_task_run_array_supports_builtin_test_task_reference_steps() {
    let cases = [
        BuiltinTestTaskRefCase {
            workspace: "run-array-builtin-test-task-ref",
            suite_name: "unit",
            task_ref: "test",
        },
        BuiltinTestTaskRefCase {
            workspace: "run-array-builtin-test-task-ref-inline-suite",
            suite_name: "vitest",
            task_ref: "test vitest",
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        let marker = root.join("builtin-test-called.log");
        write_validate_manifest(
            &root,
            &format!(
                "[test.suites]\n{} = \"sh -lc 'printf called > \\\"{}\\\"'\"\n\n[tasks.validate]\nrun = [{{ task = \"{}\" }}, \"printf validate-ok\"]\n",
                case.suite_name,
                marker.display(),
                case.task_ref
            ),
        );

        let out = run_validate_ok(&root, &["--verbose-root"]);
        assert_contains_all(&out, &["validate-ok"]);
        assert!(marker.exists(), "built-in test task ref should execute");
    }
}

#[test]
fn run_manifest_task_run_array_supports_prefixed_builtin_test_task_reference_steps() {
    let root = temp_workspace("run-array-prefixed-builtin-test-task-ref");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    let marker = farmyard.join("builtin-test-called.log");
    write_validate_manifest(
        &root,
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

    let out = run_validate_ok(&root, &["--verbose-root"]);
    assert_contains_all(&out, &["validate-ok"]);
    assert!(
        marker.exists(),
        "prefixed built-in test task ref should execute"
    );
}
