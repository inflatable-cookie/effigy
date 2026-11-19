use super::now_ms;
use crate::runner::error::RunnerError;

#[test]
fn retained_step_capture_is_complete_private_and_named_in_text_and_json() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().expect("scratch");
    let run = effigy_kernel::id::RunId::new(0, [7; 10]).expect("run id");
    crate::runner::scratch::with_test_scope(root.path(), || {
        let stdout = format!("{}\nFAIL example::broken\nthread 'broken' panicked at lib.rs:1:1:\nassertion failed: useful cause\n{}", "progress\n".repeat(300), "PASS other\n".repeat(300));
        let retained =
            crate::runner::scratch::retain_step_output(&run, &stdout, "token=private", false);
        let path = std::path::Path::new(
            retained
                .strip_prefix("full failed-step output: ")
                .expect("retained path"),
        );
        assert!(path.starts_with(root.path()));
        let capture = std::fs::read_to_string(path).expect("full output exists");
        assert!(capture.contains(&stdout));
        assert!(!capture.contains("private"));
        assert_eq!(
            std::fs::metadata(path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let stderr = format!("{}\n{retained}", "progress\n".repeat(30));
        for error in [
            RunnerError::TaskCommandFailure {
                command: "test step".into(),
                code: Some(100),
                stdout: stdout.clone(),
                stderr: stderr.clone(),
            },
            RunnerError::task_json_failure(
                serde_json::json!({}),
                "test",
                100,
                effigy_kernel::outcome::WorkloadOutcome::Failed { code: 100 },
                &stdout,
                &stderr,
            ),
        ] {
            let human = error.human_error();
            let rendered = human.render_text();
            assert!(rendered.contains(&retained), "{rendered}");
            assert!(
                rendered.contains("assertion failed: useful cause"),
                "{rendered}"
            );
            assert!(
                human.to_json()["evidence"]
                    .as_array()
                    .expect("evidence")
                    .len()
                    <= effigy_ui::human::MAX_EVIDENCE_LINES
            );
        }
    });
}

/// A relayed step's record is written where a run-scratch teardown cannot
/// remove it, keeps no credential, and still names the panic in the summary
/// the operator reads (effigy#937, QA b0850d50).
#[test]
fn a_relayed_step_record_outlives_the_run_scratch_and_names_the_panic() {
    let scratch = tempfile::tempdir().expect("scratch root");
    let run_scratch = tempfile::tempdir().expect("run scratch");
    let run = effigy_kernel::id::RunId::new(0, [9; 10]).expect("run id");
    let stdout = "PASS fine\nFAIL effigy::cli_output_tests cohort\n".to_owned();
    let stderr = format!(
        "{}
thread 'cohort' panicked at crates/effigy/src/lib.rs:12:5:
assertion `left == right` failed: shared run stayed up
token=supersecret\n",
        "progress\n".repeat(400)
    );

    let retained =
        crate::runner::scratch::with_test_scratch(Some(scratch.path().to_path_buf()), true, || {
            crate::runner::scratch::with_test_scope(run_scratch.path(), || {
                crate::runner::scratch::retain_step_output(&run, &stdout, &stderr, true)
            })
        });
    let path = std::path::Path::new(
        retained
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("full failed-step output: "))
            .expect("retained path"),
    );
    assert!(
        path.starts_with(
            scratch
                .path()
                .join("effigy/failed-steps")
                .join(run.to_string())
        ),
        "the record lives on the operator's root, not in the run's temp: {}",
        path.display()
    );
    // The harness removes the run's own temporary directory at teardown.
    std::fs::remove_dir_all(run_scratch.path()).expect("run scratch removed");
    let record = std::fs::read_to_string(path).expect("the record survived the teardown");
    assert!(
        record.contains("assertion `left == right` failed"),
        "{record}"
    );
    assert!(!record.contains("supersecret"), "{record}");

    // The failure's own summary carries the panic line and the record's path.
    let human = RunnerError::TaskCommandFailure {
        command: "test -- --no-fail-fast".into(),
        code: Some(100),
        stdout,
        stderr: format!("error: test run failed\n{retained}"),
    }
    .human_error();
    let rendered = human.render_text();
    assert!(
        rendered.contains("thread 'cohort' panicked at crates/effigy/src/lib.rs:12:5:"),
        "{rendered}"
    );
    assert!(
        rendered.contains(path.to_string_lossy().as_ref()),
        "{rendered}"
    );
    assert!(!rendered.contains("supersecret"), "{rendered}");
    assert!(human.evidence_lines().len() <= effigy_ui::human::MAX_EVIDENCE_LINES);
}

/// The record directory is a retained tree like any other: a later sweep
/// retires it a day after the failure and never while it is fresh.
#[test]
fn a_failed_step_record_is_past_its_life_a_day_after_the_failure() {
    let scratch = tempfile::tempdir().expect("scratch root");
    let root = scratch.path().to_path_buf();
    let run = effigy_kernel::id::RunId::new(0, [11; 10]).expect("run id");
    let retained = crate::runner::scratch::with_test_scratch(Some(root.clone()), true, || {
        crate::runner::scratch::retain_step_output(&run, "out", "err", false)
    });
    let path = std::path::Path::new(
        retained
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("full failed-step output: "))
            .expect("retained path"),
    )
    .to_path_buf();
    let fresh = crate::runner::scratch::sweep_at(&root, now_ms(), std::time::Duration::MAX);
    assert!(path.exists(), "a fresh record is never swept: {fresh:?}");
    assert_eq!(fresh.removed, 0, "{fresh:?}");

    let later = now_ms() + 25 * 60 * 60 * 1000;
    let report = crate::runner::scratch::sweep_at(&root, later, std::time::Duration::MAX);
    assert_eq!(report.removed, 1, "{report:?}");
    assert!(!path.exists(), "the sweep retired the record's directory");
    assert!(
        path.parent()
            .expect("record dir")
            .parent()
            .expect("failed-steps")
            .parent()
            .expect("namespace")
            .exists(),
        "the sweep removes the record's run directory, not the namespace"
    );
}

/// A configured root the operator already named `effigy` shows the namespace
/// segment twice. That layout is the intended one (effigy#946): the `effigy/`
/// child is the namespace the sweep descends into, so a record left directly
/// under such a root would sit beside it and never be retired.
#[test]
fn a_record_under_a_root_named_effigy_stays_in_the_swept_namespace() {
    let owner = tempfile::tempdir().expect("owner");
    let root = owner.path().join("scratch").join("effigy");
    std::fs::create_dir_all(&root).expect("configured root");
    let run = effigy_kernel::id::RunId::new(0, [12; 10]).expect("run id");
    let retained = crate::runner::scratch::with_test_scratch(Some(root.clone()), true, || {
        crate::runner::scratch::retain_step_output(&run, "out", "err", false)
    });
    let path = std::path::Path::new(
        retained
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("full failed-step output: "))
            .expect("retained path"),
    )
    .to_path_buf();
    let expected = root.join("effigy/failed-steps").join(run.to_string());
    assert!(
        path.starts_with(&expected),
        "the record stays under the namespace the sweep reaches: {}",
        path.display()
    );
    let later = now_ms() + 25 * 60 * 60 * 1000;
    let report = crate::runner::scratch::sweep_at(&root, later, std::time::Duration::MAX);
    assert_eq!(report.removed, 1, "{report:?}");
    assert!(!path.exists(), "a later sweep retires the record");
}
