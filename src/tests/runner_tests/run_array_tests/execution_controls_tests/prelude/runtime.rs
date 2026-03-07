use super::{
    assert_case_table, assert_task_command_failure_code, dag_max_parallel_env, fs,
    run_validate_err, run_validate_ok, temp_workspace, Duration, Instant, Path, PathBuf,
};

pub(in crate::runner::tests::run_array_tests::execution_controls_tests) struct RunArrayRuntimeFlowCase
{
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) workspace: &'static str,
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) args:
        &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) marker_rel:
        &'static str,
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) dag_max_parallel:
        Option<u32>,
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) expected_marker:
        &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) start_markers:
        &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) end_markers:
        &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) min_elapsed_ms:
        Option<u64>,
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) setup: fn(&Path, &Path),
}

pub(in crate::runner::tests::run_array_tests::execution_controls_tests) struct RunArrayRuntimeErrorCase
{
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) workspace: &'static str,
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) args:
        &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) marker_rel:
        Option<&'static str>,
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) expected_marker:
        &'static [&'static str],
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) expected_code:
        Option<Option<i32>>,
    pub(in crate::runner::tests::run_array_tests::execution_controls_tests) setup:
        fn(&Path, Option<&Path>),
}

fn flow_workspace(case: &RunArrayRuntimeFlowCase) -> (PathBuf, PathBuf) {
    let root = temp_workspace(case.workspace);
    let marker = root.join(case.marker_rel);
    (case.setup)(&root, &marker);
    (root, marker)
}

fn error_workspace(case: &RunArrayRuntimeErrorCase) -> (PathBuf, Option<PathBuf>) {
    let root = temp_workspace(case.workspace);
    let marker = case.marker_rel.map(|relative| root.join(relative));
    (case.setup)(&root, marker.as_deref());
    (root, marker)
}

fn assert_marker_contains_all(marker: &Path, expected: &[&str]) {
    let body = read_marker_text(marker);
    for needle in expected {
        assert!(
            body.contains(needle),
            "expected marker output to contain `{needle}`, got:\n{body}"
        );
    }
}

fn assert_elapsed_at_least(elapsed: Duration, min_ms: u64) {
    assert!(
        elapsed >= Duration::from_millis(min_ms),
        "expected elapsed >= {min_ms}ms, elapsed={elapsed:?}"
    );
}

pub(in crate::runner::tests::run_array_tests::execution_controls_tests) fn read_marker_text(
    marker: &Path,
) -> String {
    fs::read_to_string(marker).expect("read marker")
}

pub(in crate::runner::tests::run_array_tests::execution_controls_tests) fn read_marker_lines(
    marker: &Path,
) -> Vec<String> {
    read_marker_text(marker)
        .lines()
        .map(ToOwned::to_owned)
        .collect()
}

pub(in crate::runner::tests::run_array_tests::execution_controls_tests) fn assert_started_before_any_end(
    lines: &[String],
    start_markers: &[&str],
    end_markers: &[&str],
) {
    assert!(
        start_markers
            .iter()
            .all(|marker| lines.iter().any(|line| line == marker)),
        "expected start markers {start_markers:?}, got lines={lines:?}"
    );
    assert!(
        end_markers
            .iter()
            .all(|marker| lines.iter().any(|line| line == marker)),
        "expected end markers {end_markers:?}, got lines={lines:?}"
    );
    let first_end_idx = lines
        .iter()
        .position(|line| end_markers.iter().any(|marker| line == marker))
        .expect("expected at least one end marker");
    let starts_before_end = lines[..first_end_idx]
        .iter()
        .filter(|line| start_markers.iter().any(|marker| line == marker))
        .count();
    assert_eq!(
        starts_before_end,
        start_markers.len(),
        "expected all start markers before first end, lines={lines:?}"
    );
}

pub(in crate::runner::tests::run_array_tests::execution_controls_tests) fn assert_run_array_runtime_flow_case_table(
    cases: &[RunArrayRuntimeFlowCase],
) {
    assert_case_table(cases.iter(), |case| {
        let (root, marker) = flow_workspace(case);
        let _env = case.dag_max_parallel.map(dag_max_parallel_env);

        let start = Instant::now();
        let _ = run_validate_ok(&root, case.args);
        let elapsed = start.elapsed();

        assert_marker_contains_all(&marker, case.expected_marker);
        if !case.start_markers.is_empty() && !case.end_markers.is_empty() {
            let lines = read_marker_lines(&marker);
            assert_started_before_any_end(&lines, case.start_markers, case.end_markers);
        }
        if let Some(min_elapsed_ms) = case.min_elapsed_ms {
            assert_elapsed_at_least(elapsed, min_elapsed_ms);
        }
    });
}

pub(in crate::runner::tests::run_array_tests::execution_controls_tests) fn assert_run_array_runtime_error_case_table(
    cases: &[RunArrayRuntimeErrorCase],
) {
    assert_case_table(cases.iter(), |case| {
        let (root, marker) = error_workspace(case);

        let err = run_validate_err(&root, case.args);
        assert_task_command_failure_code(err, case.expected_code);
        if let Some(marker) = marker {
            assert_marker_contains_all(&marker, case.expected_marker);
        }
    });
}

pub(in crate::runner::tests::run_array_tests::execution_controls_tests) fn expected_cargo_paths(
    root: &Path,
) -> String {
    let canonical_root = fs::canonicalize(root).expect("canonicalize root");
    format!(
        "{}/.cargo/home|{}/.cargo/target",
        canonical_root.display(),
        canonical_root.display()
    )
}
