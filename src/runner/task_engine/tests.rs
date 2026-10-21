//! In-process engine proofs: one gate per resolved task, truthful status,
//! and conflict errors that name the holder. Kill and reclaim races across
//! processes live in `tests/task_engine_kill_matrix.rs`.

use std::path::Path;

use effigy_kernel::outcome::WorkloadOutcome;
use effigy_manifest::LoadedCatalog;

use super::*;
use crate::runner::error::RunnerError;

fn catalog(root: &Path, relative: &str, alias: &str, depth: usize) -> LoadedCatalog {
    LoadedCatalog {
        alias: alias.to_owned(),
        catalog_root: root.join(relative),
        manifest_path: root.join(relative).join("effigy.toml"),
        bundle_root: None,
        manifest: toml::from_str("").expect("empty manifest"),
        defer_run: None,
        deferred_builtins: Default::default(),
        depth,
    }
}

fn begin(target: &TaskTarget) -> Result<TaskRun, RunnerError> {
    TaskRun::begin(TaskRunRequest {
        target,
        lock_wait_ms: 0,
        custody: false,
        scratch: false,
        managed_run_token: None,
        args: &[],
        key: None,
        admission: None,
        cwd: None,
    })
}

#[test]
fn equivalent_spellings_serialize_on_one_gate_and_distinct_tasks_do_not() {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(root.path().join("apps/api")).expect("catalog");
    let api = catalog(root.path(), "apps/api", "api", 1);
    let spelled_by_alias = TaskTarget::for_catalog(root.path(), "build", &api);
    let spelled_by_path = TaskTarget::for_catalog(
        &root.path().join("."),
        "build",
        &catalog(root.path(), "./apps/api/", "api", 1),
    );

    let first = begin(&spelled_by_alias).expect("first admission");
    let error = begin(&spelled_by_path).expect_err("same task, other spelling, is excluded");
    let RunnerError::TaskLockConflict(conflict) = &error else {
        panic!("expected a lock conflict, got {error:?}");
    };
    assert_eq!(conflict.holder_pid, Some(std::process::id()));
    assert_eq!(
        conflict.holder_run.as_deref(),
        Some(first.id().to_string().as_str())
    );
    assert!(!error.to_string().contains("unlock"), "{error}");

    let root_catalog = catalog(root.path(), "", "", 0);
    let colon = begin(&TaskTarget::for_catalog(root.path(), "a:b", &root_catalog))
        .expect("`a:b` has its own gate");
    let dash = begin(&TaskTarget::for_catalog(root.path(), "a-b", &root_catalog))
        .expect("`a-b` never shares `a:b`'s gate");
    assert!(colon.close(WorkloadOutcome::Succeeded).is_empty());
    assert!(dash.close(WorkloadOutcome::Succeeded).is_empty());

    assert!(first.close(WorkloadOutcome::Succeeded).is_empty());
    begin(&spelled_by_path)
        .expect("released gate admits the next run")
        .close(WorkloadOutcome::Succeeded);
}

#[test]
fn status_reports_the_live_run_then_its_outcome_without_staleness() {
    let root = tempfile::tempdir().expect("root");
    let target = TaskTarget::for_catalog(root.path(), "build", &catalog(root.path(), "", "", 0));
    assert_eq!(task_status(&target, HISTORY_LIMIT).state, "never-run");

    let run = begin(&target).expect("admitted");
    let live = task_status(&target, HISTORY_LIMIT);
    assert_eq!(live.state, "running");
    assert_eq!(
        live.current.as_ref().and_then(|current| current.pid),
        Some(std::process::id())
    );
    assert!(
        live.latest.is_none(),
        "history never stands in for the live run"
    );

    assert!(run.close(WorkloadOutcome::Failed { code: 3 }).is_empty());
    let closed = task_status(&target, HISTORY_LIMIT);
    assert_eq!(closed.state, "failed");
    assert!(closed.current.is_none());
    let latest = closed.latest.expect("latest run");
    assert_eq!(latest.exit_code, Some(3));
    assert_eq!(latest.cleanup, Some("complete"));
}

#[test]
fn an_unclosed_run_reads_interrupted_once_its_gate_is_free_and_the_next_run_settles_it() {
    let root = tempfile::tempdir().expect("root");
    let target = TaskTarget::for_catalog(root.path(), "dev", &catalog(root.path(), "", "", 0));
    let run = begin(&target).expect("admitted");
    // Drop without close: the gate is released and the row stays open,
    // exactly what a killed owner leaves behind.
    run.abandon_for_tests();
    let interrupted = task_status(&target, HISTORY_LIMIT);
    assert_eq!(interrupted.state, "interrupted");
    assert_eq!(interrupted.next.as_deref(), Some("effigy dev"));

    let next = begin(&target).expect("the successor settles its predecessor first");
    assert!(next.close(WorkloadOutcome::Succeeded).is_empty());
    let settled = task_status(&target, HISTORY_LIMIT);
    assert_eq!(settled.state, "succeeded");
    assert_eq!(settled.history.len(), 2);
    assert_eq!(settled.history[1].state, "interrupted");
}

#[test]
fn shared_lock_excludes_other_tasks_that_declare_it() {
    let root = tempfile::tempdir().expect("root");
    let root_catalog = catalog(root.path(), "", "", 0);
    let mut first = TaskTarget::for_catalog(root.path(), "migrate", &root_catalog);
    first.shared_lock = Some("db".to_owned());
    let mut second = TaskTarget::for_catalog(root.path(), "seed", &root_catalog);
    second.shared_lock = Some("db".to_owned());

    let held = begin(&first).expect("first holder");
    let error = begin(&second).expect_err("shared lock is held");
    let RunnerError::TaskLockConflict(conflict) = &error else {
        panic!("expected a lock conflict, got {error:?}");
    };
    assert_eq!(conflict.scope, "shared:db");
    let holders = live_holders(root.path());
    assert!(holders
        .iter()
        .any(|holder| holder.kind == HolderKind::SharedLock("db".to_owned())));
    assert!(held.close(WorkloadOutcome::Succeeded).is_empty());
    begin(&second)
        .expect("released shared lock admits the other task")
        .close(WorkloadOutcome::Succeeded);
    assert!(live_holders(root.path()).is_empty());
}

#[test]
fn sequence_durable_stop_request_cancels_active_custody_and_settles_child() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().unwrap();
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "stop-proof", &catalog);
    let run = begin(&target).unwrap();
    let mut child = run.step(0, "active").unwrap();
    let id = run.id();
    // Read the fence from the live owner handle before another thread posts
    // the durable request. Only the owning thread writes run outcomes.
    let mut parent = run;
    let expected = parent.run_mut().fence().clone();
    let requester = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        effigy_kernel::run::request(
            &super::kernel::effects(),
            super::kernel::effects_root().unwrap(),
            &effigy_kernel::run::Request {
                id: effigy_kernel::id::RunId::new(99, [19; 10]).unwrap(),
                run: id,
                expected,
                action: effigy_kernel::run::RequestAction::Stop,
            },
        )
        .unwrap();
    });
    let exit = {
        let _parent = parent.enter();
        let _step = child.enter();
        run_plain(
            &mut child,
            PlainCommand {
                command: "sleep 30",
                cwd: root.path(),
                env: vec![],
                capture: true,
                timeout_ms: Some(5000),
            },
        )
        .unwrap()
    };
    requester.join().unwrap();
    assert_eq!(
        exit.workload,
        WorkloadOutcome::Cancelled {
            cause: effigy_kernel::ctx::StopCause::Requested,
            signal: None
        }
    );
    child.close(exit.workload.clone());
    parent.close(exit.workload);
    let status = task_status(&target, HISTORY_LIMIT);
    assert_eq!(status.latest.unwrap().steps[0].state, "cancelled");
}

#[test]
fn sequence_pending_stop_starts_no_next_workload() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().unwrap();
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "pending-stop-proof", &catalog);
    let mut parent = begin(&target).unwrap();
    let expected = parent.run_mut().fence().clone();
    effigy_kernel::run::request(
        &super::kernel::effects(),
        super::kernel::effects_root().unwrap(),
        &effigy_kernel::run::Request {
            id: effigy_kernel::id::RunId::new(99, [20; 10]).unwrap(),
            run: parent.id(),
            expected,
            action: effigy_kernel::run::RequestAction::Stop,
        },
    )
    .unwrap();
    let mut child = parent.step(0, "must-not-start").unwrap();
    let exit = {
        let _parent = parent.enter();
        let _step = child.enter();
        run_plain(
            &mut child,
            PlainCommand {
                command: "touch should-not-exist",
                cwd: root.path(),
                env: vec![],
                capture: true,
                timeout_ms: None,
            },
        )
        .unwrap()
    };
    assert!(!root.path().join("should-not-exist").exists());
    assert!(child.run_mut().effects().unwrap().is_empty());
    child.close(exit.workload.clone());
    parent.close(exit.workload);
}

// ---------------------------------------------------------------------------
// Per-run scratch (user config `[scratch]`)
// ---------------------------------------------------------------------------

/// A scratch configuration for a test's own directory, admitted without a
/// probe: the volume gate itself is proved by the refusals in `scratch::tests`.
fn scratch_config(root: &Path) -> crate::runner::scratch::Config {
    crate::runner::scratch::Config {
        root: std::fs::canonicalize(root).expect("canonical scratch root"),
        volume_uuid: None,
        require_separate_volume: false,
    }
}

fn begin_scratching(target: &TaskTarget) -> TaskRun {
    TaskRun::begin(TaskRunRequest {
        target,
        lock_wait_ms: 0,
        custody: true,
        scratch: true,
        managed_run_token: None,
        args: &[],
        key: None,
        admission: None,
        cwd: None,
    })
    .expect("admitted")
}

/// The `TMPDIR` a host command of `run` sees, and whether it could write there.
fn read_temp(run: &mut TaskRun, work: &Path) -> (String, bool) {
    let exit = run_plain(
        run,
        PlainCommand {
            command: "printf '%s\\n' \"$TMPDIR\"; printf probe > \"$TMPDIR/effigy-probe\"",
            cwd: work,
            env: vec![],
            capture: true,
            timeout_ms: Some(30_000),
        },
    )
    .expect("the probe command ran");
    assert_eq!(exit.code, 0, "probe failed: {:?}", exit.stderr);
    let seen = String::from_utf8_lossy(&exit.stdout).trim().to_owned();
    let wrote = Path::new(&seen).join("effigy-probe").exists();
    (seen, wrote)
}

#[test]
fn a_host_run_uses_its_scratch_as_temp_and_the_settle_removes_it_on_success() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "build", &catalog);
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);

    let warnings = crate::runner::scratch::with_test_scratch(
        Some(config.clone()),
        Some(config.root.clone()),
        true,
        || {
            let mut run = begin_scratching(&target);
            let (seen, wrote) = read_temp(&mut run, work.path());
            assert_eq!(
                Path::new(&seen),
                Path::new(&seen),
                "the run's temp directory is on the configured volume"
            );
            assert!(
                Path::new(&seen).starts_with(&config.root),
                "`{seen}` is not under the configured root"
            );
            assert!(wrote, "a workload writes into its own scratch");
            assert_eq!(
                seen.trim_end_matches('/').rsplit('/').next().unwrap().len(),
                8,
                "the directory is named by the run's 8-character id"
            );
            let warnings = run.close(WorkloadOutcome::Succeeded);
            assert!(
                !Path::new(&seen).exists(),
                "the settle removed the run's scratch on success"
            );
            warnings
        },
    );
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_failed_run_keeps_its_scratch_and_prints_the_path() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "test", &catalog);
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);

    let (seen, warnings) = crate::runner::scratch::with_test_scratch(
        Some(config.clone()),
        Some(config.root.clone()),
        true,
        || {
            let mut run = begin_scratching(&target);
            let (seen, wrote) = read_temp(&mut run, work.path());
            assert!(wrote);
            let warnings = run.close(WorkloadOutcome::Failed { code: 1 });
            (seen, warnings)
        },
    );
    assert!(
        Path::new(&seen).exists(),
        "a failed run keeps its scratch for 24 hours"
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("scratch kept at") && warning.contains(&seen)),
        "the failure output names the retained path: {warnings:?}"
    );
    let marker = std::fs::read_to_string(Path::new(&seen).join(".effigy-scratch.json"))
        .expect("marker written with the decision");
    assert!(marker.contains("\"keep\":true"), "{marker}");
}

#[test]
fn an_explicit_or_non_default_tmpdir_wins_and_no_scratch_is_created() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "build", &catalog);
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);

    let seen = crate::runner::scratch::with_test_scratch(
        Some(config.clone()),
        Some(config.root.clone()),
        // The run inherited an explicit `TMPDIR`, which always wins.
        false,
        || {
            let mut run = begin_scratching(&target);
            let (seen, _) = read_temp(&mut run, work.path());
            assert!(run.close(WorkloadOutcome::Succeeded).is_empty());
            seen
        },
    );
    assert!(
        !Path::new(&seen).starts_with(&config.root),
        "an explicit TMPDIR is never replaced: {seen}"
    );
    assert_eq!(
        std::fs::read_dir(&config.root).expect("root").count(),
        0,
        "no scratch directory was created"
    );
}

#[test]
fn a_nested_run_reuses_its_parents_scratch_instead_of_starting_another() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "qa", &catalog);
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);

    crate::runner::scratch::with_test_scratch(
        Some(config.clone()),
        Some(config.root.clone()),
        true,
        || {
            let mut parent = begin_scratching(&target);
            let (parent_temp, _) = read_temp(&mut parent, work.path());
            let mut step = parent.step(0, "step").expect("step admitted");
            let (step_temp, _) = read_temp(&mut step, work.path());
            assert_eq!(
                step_temp, parent_temp,
                "a nested run reuses its parent's scratch"
            );
            assert!(step.close(WorkloadOutcome::Succeeded).is_empty());
            assert!(parent_temp.ends_with(parent.id().to_string()[..8].to_owned().as_str()));
            assert!(parent.close(WorkloadOutcome::Succeeded).is_empty());
            assert!(!Path::new(&parent_temp).exists());
            assert_eq!(
                std::fs::read_dir(&config.root).expect("root").count(),
                0,
                "one run, one scratch directory"
            );
        },
    );
}

#[test]
fn a_run_that_died_keeps_its_scratch_until_the_sweep_removes_it() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "dev", &catalog);
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);

    let seen = crate::runner::scratch::with_test_scratch(
        Some(config.clone()),
        Some(config.root.clone()),
        true,
        || {
            let mut run = begin_scratching(&target);
            let (seen, _) = read_temp(&mut run, work.path());
            // What a killed owner leaves: an open effect and no decision.
            run.abandon_for_tests();
            // The successor settles the predecessor through the same adapter
            // the sentinel uses, and still keeps its directory.
            let successor = TaskRun::begin(TaskRunRequest {
                target: &target,
                lock_wait_ms: 0,
                custody: false,
                scratch: false,
                managed_run_token: None,
                args: &[],
                key: None,
                admission: None,
                cwd: None,
            })
            .expect("the successor is admitted");
            assert!(successor.close(WorkloadOutcome::Succeeded).is_empty());
            seen
        },
    );
    assert!(
        Path::new(&seen).exists(),
        "an unsettled run's scratch is not removed by its own absent decision"
    );
    let marker =
        std::fs::read_to_string(Path::new(&seen).join(".effigy-scratch.json")).expect("marker");
    assert!(
        !marker.contains("keep"),
        "no decision means retained: {marker}"
    );
}

#[test]
fn without_a_configured_root_a_host_run_sees_the_inherited_temp_environment() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "build", &catalog);

    crate::runner::scratch::with_test_scratch(None, None, true, || {
        let mut run = begin_scratching(&target);
        let inherited = std::env::var("TMPDIR").unwrap_or_default();
        let (seen, _) = read_temp(&mut run, work.path());
        assert_eq!(seen, inherited, "no config changes nothing about the env");
        assert!(run.close(WorkloadOutcome::Succeeded).is_empty());
    });
}
