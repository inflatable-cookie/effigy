use super::prelude::*;

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

    assert_validate_ok_empty(&root, &[]);
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
    assert_task_command_failure_code(err, Some(Some(124)));
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
    assert_task_command_failure_code(err, None);
    let body = fs::read_to_string(marker).expect("read marker");
    assert!(body.contains("good"));
}

#[test]
fn run_manifest_task_run_array_supports_compact_env_directive_entry() {
    let root = temp_workspace("run-array-compact-env-directive-entry");
    let marker = root.join("compact-env.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks]
api = [
  {{ env = {{ CARGO_HOME = "{{project}}/.cargo/home", CARGO_TARGET_DIR = "{{project}}/.cargo/target" }} }},
  {{ run = "sh -lc 'printf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );

    assert_eq!(run_builtin_ok(root.clone(), "api", &[]), "");
    let body = fs::read_to_string(marker).expect("read marker");
    let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
    assert_eq!(
        body,
        format!(
            "{}/.cargo/home|{}/.cargo/target",
            canonical_root.display(),
            canonical_root.display()
        )
    );
}

#[test]
fn run_manifest_task_run_array_supports_named_env_profile_directive() {
    let root = temp_workspace("run-array-env-profile-directive-entry");
    let marker = root.join("env-profile.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[env]
cargo = [
  {{ CARGO_HOME = "{{project}}/.cargo/home" }},
  {{ CARGO_TARGET_DIR = "{{project}}/.cargo/target" }}
]

[tasks]
api = [
  {{ env = "cargo" }},
  {{ run = "sh -lc 'printf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );

    assert_eq!(run_builtin_ok(root.clone(), "api", &[]), "");
    let body = fs::read_to_string(marker).expect("read marker");
    let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
    assert_eq!(
        body,
        format!(
            "{}/.cargo/home|{}/.cargo/target",
            canonical_root.display(),
            canonical_root.display()
        )
    );
}

#[test]
fn run_manifest_task_run_array_supports_named_env_value_directive() {
    let root = temp_workspace("run-array-env-value-directive-entry");
    let marker = root.join("env-value.out");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[env]
CARGO_HOME = "{{project}}/.cargo/home"
CARGO_TARGET_DIR = "{{project}}/.cargo/target"

[tasks]
api = [
  {{ env = "CARGO_HOME" }},
  {{ env = "CARGO_TARGET_DIR" }},
  {{ run = "sh -lc 'printf \"%s|%s\" \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );

    assert_eq!(run_builtin_ok(root.clone(), "api", &[]), "");
    let body = fs::read_to_string(marker).expect("read marker");
    let canonical_root = fs::canonicalize(&root).expect("canonicalize root");
    assert_eq!(
        body,
        format!(
            "{}/.cargo/home|{}/.cargo/target",
            canonical_root.display(),
            canonical_root.display()
        )
    );
}

#[test]
fn run_manifest_task_run_array_errors_for_unknown_env_profile() {
    let root = temp_workspace("run-array-env-profile-missing");
    write_validate_manifest(
        &root,
        r#"[tasks.validate]
run = [
  { env = "missing-profile" },
  { run = "printf unreachable" }
]
"#,
    );
    let err = run_validate_err(&root, &[]);
    assert_task_invocation_error_contains(err, &["unknown env entry `missing-profile`"]);
}

#[test]
fn run_manifest_task_run_array_supports_relative_catalog_env_reference() {
    let root = temp_workspace("run-array-env-relative-catalog-ref");
    let sub_project1 = root.join("sub-project1");
    let sub_project2 = root.join("sub-project2");
    fs::create_dir_all(&sub_project1).expect("mkdir sub-project1");
    fs::create_dir_all(&sub_project2).expect("mkdir sub-project2");
    let marker = sub_project2.join("env-cross-catalog.out");

    write_manifest(
        &sub_project1.join("effigy.toml"),
        r#"[catalog]
alias = "project1"

[env]
MY_VAR = "my value"
"#,
    );
    write_manifest(
        &sub_project2.join("effigy.toml"),
        &format!(
            r#"[catalog]
alias = "project2"

[tasks]
api = [
  {{ env = "../sub-project1/MY_VAR" }},
  {{ run = "sh -lc 'printf %s \"$MY_VAR\" > \"{}\"'" }}
]
"#,
            marker.display()
        ),
    );

    assert_eq!(run_builtin_ok(root.clone(), "project2/api", &[]), "");
    let body = fs::read_to_string(marker).expect("read marker");
    assert_eq!(body, "my value");
}

#[test]
fn run_manifest_task_run_array_errors_for_missing_relative_catalog_env_reference() {
    let root = temp_workspace("run-array-env-relative-catalog-ref-missing");
    let sub_project1 = root.join("sub-project1");
    let sub_project2 = root.join("sub-project2");
    fs::create_dir_all(&sub_project1).expect("mkdir sub-project1");
    fs::create_dir_all(&sub_project2).expect("mkdir sub-project2");

    write_manifest(
        &sub_project1.join("effigy.toml"),
        r#"[catalog]
alias = "project1"

[env]
MY_VAR = "my value"
"#,
    );
    write_manifest(
        &sub_project2.join("effigy.toml"),
        r#"[catalog]
alias = "project2"

[tasks]
api = [
  { env = "../sub-project1/MISSING_VAR" },
  { run = "printf unreachable" }
]
"#,
    );

    let err = run_builtin_err(root, "project2/api", &[]);
    assert_task_invocation_error_contains(err, &["unknown env entry `../sub-project1/MISSING_VAR`"]);
}
