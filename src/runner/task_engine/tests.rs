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
        managed_run_token: None,
        args: &[],
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
