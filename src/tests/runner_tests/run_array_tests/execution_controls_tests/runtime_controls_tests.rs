use super::prelude::*;

fn setup_parallel_ready_steps(root: &Path, marker: &Path) {
    write_validate_manifest_template(
        root,
        r#"[tasks.validate]
run = [
  { id = "seed", run = "echo seed > \"__MARKER__\"" },
  { id = "a", run = "sh -lc 'echo a-start >> \"__MARKER__\"; sleep 0.8; echo a-end >> \"__MARKER__\"'", depends_on = ["seed"] },
  { id = "b", run = "sh -lc 'echo b-start >> \"__MARKER__\"; sleep 0.8; echo b-end >> \"__MARKER__\"'", depends_on = ["seed"] }
]
"#,
        &[("__MARKER__", marker)],
    );
}

fn setup_parallel_cap(root: &Path, marker: &Path) {
    write_validate_manifest_template(
        root,
        r#"[tasks.validate]
run = [
  { id = "seed", run = "printf seed > \"__MARKER__\"" },
  { id = "a", run = "sh -lc 'sleep 0.8; printf a >> \"__MARKER__\"'", depends_on = ["seed"] },
  { id = "b", run = "sh -lc 'sleep 0.8; printf b >> \"__MARKER__\"'", depends_on = ["seed"] }
]
"#,
        &[("__MARKER__", marker)],
    );
}

fn setup_retry_step(root: &Path, out_file: &Path) {
    let marker = root.join("retry.marker");
    write_validate_manifest_template(
        root,
        r#"[tasks.validate]
run = [
  { id = "flaky", run = "sh -lc 'if [ -f \"__RETRY_MARKER__\" ]; then printf ok > \"__OUT_FILE__\"; exit 0; else touch \"__RETRY_MARKER__\"; exit 7; fi'", retry = 1, retry_delay_ms = 10 }
]
"#,
        &[("__RETRY_MARKER__", &marker), ("__OUT_FILE__", out_file)],
    );
}

fn setup_timeout_step(root: &Path) {
    write_validate_manifest(
        root,
        r#"[tasks.validate]
run = [
  { id = "slow", run = "sleep 1", timeout_ms = 100 }
]
"#,
    );
}

fn setup_fail_fast_false(root: &Path, marker: &Path) {
    write_validate_manifest_template(
        root,
        r#"[tasks.validate]
run = [
  { id = "seed", run = "printf seed > \"__MARKER__\"" },
  { id = "bad", run = "sh -lc 'sleep 0.1; exit 9'", depends_on = ["seed"], fail_fast = false },
  { id = "good", run = "printf good >> \"__MARKER__\"", depends_on = ["seed"] }
]
"#,
        &[("__MARKER__", marker)],
    );
}

fn setup_timeout_step_case(root: &Path, _marker: Option<&Path>) {
    setup_timeout_step(root);
}

fn setup_fail_fast_false_case(root: &Path, marker: Option<&Path>) {
    setup_fail_fast_false(root, marker.expect("fail-fast marker"));
}

#[test]
fn run_manifest_task_run_array_runtime_flow_contract_table() {
    let _guard = lock_test();
    let cases = [
        RunArrayRuntimeFlowCase {
            workspace: "run-array-parallel-ready-steps",
            args: &[],
            marker_rel: "parallel-ready.log",
            dag_max_parallel: Some(2),
            expected_marker: &["a-start", "a-end", "b-start", "b-end"],
            start_markers: &["a-start", "b-start"],
            end_markers: &["a-end", "b-end"],
            min_elapsed_ms: None,
            setup: setup_parallel_ready_steps,
        },
        RunArrayRuntimeFlowCase {
            workspace: "run-array-parallel-cap",
            args: &[],
            marker_rel: "parallel-cap.log",
            dag_max_parallel: Some(1),
            expected_marker: &["a", "b"],
            start_markers: &[],
            end_markers: &[],
            min_elapsed_ms: Some(1400),
            setup: setup_parallel_cap,
        },
    ];

    assert_run_array_runtime_flow_case_table(&cases);
}

#[test]
fn run_manifest_task_run_array_retries_failing_step() {
    let cases = [RunArrayTaskOutputCase {
        workspace: "run-array-retry-step",
        task: "validate",
        marker_rel: "retry.out",
        expected: "ok",
        setup: setup_retry_step,
    }];

    assert_run_array_task_output_case_table(&cases);
}

#[test]
fn run_manifest_task_run_array_runtime_error_contract_table() {
    let _guard = lock_test();
    let cases = [
        RunArrayRuntimeErrorCase {
            workspace: "run-array-timeout-step",
            args: &[],
            marker_rel: None,
            expected_marker: &[],
            expected_code: Some(Some(124)),
            setup: setup_timeout_step_case,
        },
        RunArrayRuntimeErrorCase {
            workspace: "run-array-fail-fast-false",
            args: &[],
            marker_rel: Some("fail-fast-false.out"),
            expected_marker: &["good"],
            expected_code: None,
            setup: setup_fail_fast_false_case,
        },
    ];

    assert_run_array_runtime_error_case_table(&cases);
}
