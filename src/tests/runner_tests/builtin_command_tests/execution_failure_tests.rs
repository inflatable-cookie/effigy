use crate::runner::tests::prelude::{
    assert_builtin_test_non_zero, assert_json_array_field, assert_json_string_field_eq,
    assert_output_excludes_all, fs, install_local_vitest, parse_json_output_with_schema,
    run_builtin_err, setup_fanout_catalog_repo, temp_workspace,
    write_package_json_with_test_script, RunnerError,
};

#[test]
fn run_manifest_task_builtin_test_failure_keeps_rendered_results_summary() {
    let root = temp_workspace("builtin-test-fanout-failure-summary");
    let (catalog_a, catalog_b) = setup_fanout_catalog_repo(&root);
    install_local_vitest(&catalog_a, "#!/bin/sh\nexit 1\n");
    install_local_vitest(&catalog_b, "#!/bin/sh\nexit 0\n");

    let err = run_builtin_err(root, "test", &[]);

    assert_builtin_test_non_zero(
        err,
        Some(vec![("catalog_a".to_owned(), Some(1))]),
        &["Test Results", "catalog_b", "ok", "catalog_a", "exit=1"],
        &["runner:vitest", "command:"],
    );
}

#[test]
fn run_manifest_task_builtin_test_json_failure_includes_results_and_failures() {
    let root = temp_workspace("builtin-test-fanout-failure-json");
    let (catalog_a, catalog_b) = setup_fanout_catalog_repo(&root);
    install_local_vitest(&catalog_a, "#!/bin/sh\nexit 1\n");
    install_local_vitest(&catalog_b, "#!/bin/sh\nexit 0\n");

    let err = run_builtin_err(root, "test", &["--json"]);
    match err {
        RunnerError::BuiltinTestNonZero {
            failures, rendered, ..
        } => {
            assert_eq!(failures, vec![("catalog_a".to_owned(), Some(1))]);
            let parsed = parse_json_output_with_schema(&rendered, "effigy.test.results.v1");
            assert_json_string_field_eq(&parsed["failures"][0], "target", "catalog_a");
            let target_names = parsed["targets"]
                .as_array()
                .expect("targets array")
                .iter()
                .filter_map(|entry| entry["target"].as_str())
                .collect::<Vec<&str>>();
            assert!(target_names.contains(&"catalog_b"));
            assert!(target_names.contains(&"catalog_a"));
            assert!(parsed["targets"]
                .as_array()
                .expect("targets array")
                .iter()
                .all(|entry| entry["cargo_env_match"].is_string()));
            // Every target names its outcome, and only the suite that ran and
            // passed reports `passed`.
            let outcomes = parsed["targets"]
                .as_array()
                .expect("targets array")
                .iter()
                .map(|entry| {
                    (
                        entry["target"].as_str().unwrap_or_default().to_owned(),
                        entry["outcome"].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect::<std::collections::BTreeMap<String, String>>();
            assert_eq!(outcomes["catalog_a"], "failed");
            assert_eq!(outcomes["catalog_b"], "passed");
        }
        other => panic!("unexpected error: {other}"),
    }
}

/// The run that owns a cancelled builtin test closes as cancelled; an ordinary
/// builtin failure still closes as a failed check. A closeout that guessed
/// `Failed { code: 1 }` would record a cancellation as a failure.
#[test]
fn a_builtin_test_error_records_the_outcome_its_run_closes_with() {
    use effigy_kernel::outcome::WorkloadOutcome;

    let cancelled = RunnerError::BuiltinTestNonZero {
        failures: vec![("suite".to_owned(), None)],
        cancelled: true,
        rendered: "{}".to_owned(),
    };
    assert!(
        matches!(
            cancelled.workload_outcome(),
            WorkloadOutcome::Cancelled { .. }
        ),
        "a cancelled builtin test closes its run as cancelled: {:?}",
        cancelled.workload_outcome()
    );

    let failed = RunnerError::BuiltinTestNonZero {
        failures: vec![("suite".to_owned(), Some(7))],
        cancelled: false,
        rendered: "{}".to_owned(),
    };
    assert!(matches!(
        failed.workload_outcome(),
        WorkloadOutcome::Failed { code: 1 }
    ));
}

/// An operator stop is a cancellation, not a failed check: the task carries
/// the interrupted exit code so durable run records read the same outcome the
/// text and JSON do, and a plain failure keeps the ordinary exit.
#[test]
fn a_cancelled_builtin_test_run_carries_the_interrupted_exit_code() {
    let cancelled = RunnerError::BuiltinTestNonZero {
        failures: vec![("suite".to_owned(), None)],
        cancelled: true,
        rendered: "{}".to_owned(),
    };
    assert_eq!(cancelled.process_exit_code(), 130);
    assert!(
        cancelled.to_string().contains("cancelled"),
        "the failure names the cancellation: {cancelled}"
    );

    let failed = RunnerError::BuiltinTestNonZero {
        failures: vec![("suite".to_owned(), Some(7))],
        cancelled: false,
        rendered: "{}".to_owned(),
    };
    assert_eq!(failed.process_exit_code(), 1);
}

#[test]
fn run_manifest_task_builtin_test_json_failure_does_not_include_text_hint_footer() {
    let root = temp_workspace("builtin-test-json-filtered-failure-no-hint");
    write_package_json_with_test_script(&root);
    install_local_vitest(&root, "#!/bin/sh\nexit 1\n");

    let err = run_builtin_err(root, "test", &["--json", "vitest", "user-service"]);
    match err {
        RunnerError::BuiltinTestNonZero { rendered, .. } => {
            assert_output_excludes_all(&rendered, &["\nHint\n────\n"]);
            let parsed = parse_json_output_with_schema(&rendered, "effigy.test.results.v1");
            assert_json_array_field(&parsed, "failures");
            assert_json_string_field_eq(&parsed["hint"], "kind", "selected-suite-filter-no-match");
            assert_json_string_field_eq(
                &parsed["hint"],
                "suggestion",
                "Try again without the filter to verify suite execution.",
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_manifest_task_builtin_test_failure_with_suite_filter_shows_no_match_hint() {
    let root = temp_workspace("builtin-test-filtered-failure-hint");
    write_package_json_with_test_script(&root);
    install_local_vitest(&root, "#!/bin/sh\nexit 1\n");

    let err = run_builtin_err(root, "test", &["vitest", "user-service"]);

    assert_builtin_test_non_zero(
        err,
        None,
        &[
            "Hint",
            "often means no tests matched",
            "vitest run 'user-service'",
            "Try again without the filter",
        ],
        &[],
    );
}

/// A suite that leaves a process in its own group is named, reaped, and fails
/// the route. The suite itself exits 0, so only the suite sweep can report the
/// leak; a kernel-custody payload's descriptor plan would hide it from the
/// selector marker, which is why the suite opens the sweep's own descriptor.
#[test]
fn run_manifest_task_builtin_test_sweeps_a_leaked_suite_process() {
    let root = temp_workspace("builtin-test-suite-sweep");
    let leaked = root.join("leaked.pid");
    write_package_json_with_test_script(&root);
    install_local_vitest(
        &root,
        &format!(
            "#!/bin/sh\nset -m\n\
             sh -c 'trap \"\" TERM HUP; echo $$ > \"{pid}\"; while :; do /bin/sleep 0.05; done' \
             >/dev/null 2>&1 &\n\
             while [ ! -s \"{pid}\" ]; do /bin/sleep 0.01; done\n\
             exit 0\n",
            pid = leaked.display()
        ),
    );

    let err = run_builtin_err(root, "test", &[]);
    let message = err.to_string();
    let pid: u32 = fs::read_to_string(&leaked)
        .expect("the suite leaked its child")
        .trim()
        .parse()
        .expect("leaked pid");
    let alive = || {
        // SAFETY: signal 0 only probes the exact PID the fixture leaked.
        unsafe { libc::kill(pid as i32, 0) == 0 }
    };
    let swept = !alive();
    // SAFETY: retires the documented fixture even when an assertion fails first.
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
    assert!(message.contains("outlived the test suite"), "{message}");
    assert!(message.contains(&pid.to_string()), "{message}");
    assert!(swept, "the sweep reaps and names leaked process {pid}");
}
