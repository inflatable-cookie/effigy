//! In-process engine proofs: one gate per resolved task, truthful status,
//! and conflict errors that name the holder. Kill and reclaim races across
//! processes live in `tests/task_engine_kill_matrix.rs`.

use std::path::Path;

use effigy_kernel::outcome::WorkloadOutcome;
use effigy_manifest::LoadedCatalog;

use super::*;
use crate::runner::error::RunnerError;

fn catalog(root: &Path, relative: &str, alias: &str, depth: usize) -> LoadedCatalog {
    LoadedCatalog::for_test(
        alias,
        root.join(relative),
        toml::from_str("").expect("empty manifest"),
        depth,
    )
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
        workspace_admission: None,
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
    assert_eq!(task_status(&target, HISTORY_LIMIT).state, "not-recorded");

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
    assert_eq!(closed.state, "not-recorded");
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
    assert_eq!(settled.state, "not-recorded");
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
    let expected = parent.run_mut().expect("durable test run").fence().clone();
    let state = super::kernel::effects_root(root.path()).unwrap();
    let requester = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        effigy_kernel::run::request(
            &super::kernel::effects(),
            state,
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
    let expected = parent.run_mut().expect("durable test run").fence().clone();
    effigy_kernel::run::request(
        &super::kernel::effects(),
        super::kernel::effects_root(root.path()).unwrap(),
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
    assert!(child
        .run_mut()
        .expect("durable test run")
        .effects()
        .unwrap()
        .is_empty());
    child.close(exit.workload.clone());
    parent.close(exit.workload);
}

// ---------------------------------------------------------------------------
// Per-run scratch (user config `[scratch]`)
// ---------------------------------------------------------------------------

/// A scratch configuration for a test's own directory.
fn scratch_config(root: &Path) -> crate::runner::scratch::Config {
    crate::runner::scratch::Config {
        root: std::fs::canonicalize(root).expect("canonical scratch root"),
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
        workspace_admission: None,
        cwd: None,
    })
    .expect("admitted")
}

/// The `TMPDIR` a host command of `run` sees, and whether it could write there.
/// A run without a scratch directory leaves `TMPDIR` unset where the platform
/// has no default (Linux), so the probe writes to the working directory then.
fn read_temp(run: &mut TaskRun, work: &Path) -> (String, bool) {
    let exit = run_plain(
        run,
        PlainCommand {
            command: "printf '%s\\n' \"$TMPDIR\"; printf probe > \"${TMPDIR:-.}/effigy-probe\"",
            cwd: work,
            env: vec![],
            capture: true,
            timeout_ms: Some(30_000),
        },
    )
    .expect("the probe command ran");
    assert_eq!(exit.code, 0, "probe failed: {:?}", exit.stderr);
    let seen = String::from_utf8_lossy(&exit.stdout).trim().to_owned();
    // Only a run that owns a directory proves writability of it.
    let wrote = !seen.is_empty() && Path::new(&seen).join("effigy-probe").exists();
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

    let warnings =
        crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
            let mut run = begin_scratching(&target);
            let (seen, wrote) = read_temp(&mut run, work.path());
            assert_eq!(
                Path::new(&seen),
                crate::runner::scratch::path_for_tests(&config.root, &run.id()),
                "the run's temp directory is its own directory on the configured volume"
            );
            assert!(
                Path::new(&seen)
                    .starts_with(crate::runner::scratch::namespace_for_tests(&config.root)),
                "`{seen}` is not under the configured root's namespace"
            );
            assert!(wrote, "a workload writes into its own scratch");
            assert_eq!(
                seen.trim_end_matches('/').rsplit('/').next().unwrap(),
                crate::runner::scratch::name_for_tests(&run.id()),
                "the directory is named by the run"
            );
            let warnings = run.close(WorkloadOutcome::Succeeded);
            assert!(
                !Path::new(&seen).exists(),
                "the settle removed the run's scratch on success"
            );
            warnings
        });
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// Publishes the read-only state objects an older build left in a temp
/// directory: mode-0500 directories holding mode-0400 files, which a plain
/// `remove_dir_all` cannot clear.
fn publish_old_read_only_objects(temp: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let objects = temp.join("state/objects/ab");
    std::fs::create_dir_all(&objects).expect("objects");
    let object = objects.join("blob");
    std::fs::write(&object, b"immutable object").expect("object");
    std::fs::set_permissions(&object, std::fs::Permissions::from_mode(0o400)).expect("object mode");
    for dir in [&objects, &temp.join("state/objects")] {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o500)).expect("mode");
    }
}

/// A successful run whose temp tree holds state objects an older build sealed
/// read-only is released completely, and the task's next run is admitted:
/// nothing the predecessor left can refuse it.
#[test]
fn a_successful_run_releases_an_old_read_only_scratch_tree_and_its_task_runs_again() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "test", &catalog);
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);

    crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
        let mut first = begin_scratching(&target);
        let (seen, wrote) = read_temp(&mut first, work.path());
        assert!(wrote, "the run writes into its own scratch");
        publish_old_read_only_objects(Path::new(&seen));
        let warnings = first.close(WorkloadOutcome::Succeeded);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(
            !Path::new(&seen).exists(),
            "the settle removed a tree holding read-only objects"
        );

        // The same task, admitted again: the released directory left no
        // residue to settle and nothing to refuse this run.
        let mut second = begin_scratching(&target);
        let (again, wrote) = read_temp(&mut second, work.path());
        assert!(wrote);
        assert_ne!(again, seen, "a later run gets its own directory");
        let warnings = second.close(WorkloadOutcome::Succeeded);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!Path::new(&again).exists());
    });
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

    let (seen, warnings) =
        crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
            let mut run = begin_scratching(&target);
            let (seen, wrote) = read_temp(&mut run, work.path());
            assert!(wrote);
            let warnings = run.close(WorkloadOutcome::Failed { code: 1 });
            (seen, warnings)
        });
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

/// A root Effigy cannot create is an observation, never a refusal: the run
/// proceeds on the default temp directory and the operator is told once.
#[test]
fn an_unwritable_scratch_root_leaves_the_run_on_the_system_temp_directory() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "build", &catalog);
    // A file where a directory must be: nothing can be created below it.
    let blocker = root.path().join("gone-volume");
    std::fs::write(&blocker, b"not a directory").expect("blocker");
    let configured = blocker.join("scratch");

    let seen = crate::runner::scratch::with_test_scratch(Some(configured.clone()), true, || {
        let inherited = std::env::var("TMPDIR").unwrap_or_default();
        let mut run = begin_scratching(&target);
        let (seen, _) = read_temp(&mut run, work.path());
        assert!(
            run.close(WorkloadOutcome::Succeeded).is_empty(),
            "an unusable root never fails or warns the run itself"
        );
        assert_eq!(seen, inherited, "the run kept the inherited temp directory");
        seen
    });
    assert!(
        !Path::new(&seen).starts_with(&configured),
        "the run fell back to the system temp directory: {seen}"
    );
    assert!(
        !configured.exists(),
        "nothing was created below the blocker"
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

    crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
        let mut parent = begin_scratching(&target);
        let (parent_temp, _) = read_temp(&mut parent, work.path());
        let mut step = parent.step(0, "step").expect("step admitted");
        let (step_temp, _) = read_temp(&mut step, work.path());
        assert_eq!(
            step_temp, parent_temp,
            "a nested run reuses its parent's scratch"
        );
        assert!(step.close(WorkloadOutcome::Succeeded).is_empty());
        assert!(
            parent_temp.ends_with(&crate::runner::scratch::name_for_tests(&parent.id())),
            "the directory is named by the run"
        );
        assert!(parent.close(WorkloadOutcome::Succeeded).is_empty());
        assert!(!Path::new(&parent_temp).exists());
        assert!(
            std::fs::read_dir(crate::runner::scratch::namespace_for_tests(&config.root))
                .expect("namespace")
                .count()
                == 0,
            "one run, one scratch directory"
        );
    });
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

    let seen = crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
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
            workspace_admission: None,
            cwd: None,
        })
        .expect("the successor is admitted");
        assert!(successor.close(WorkloadOutcome::Succeeded).is_empty());
        seen
    });
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
fn a_successful_step_never_decides_its_parents_scratch_retention() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "qa", &catalog);
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);

    let seen = crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
        let mut parent = begin_scratching(&target);
        let (seen, _) = read_temp(&mut parent, work.path());
        let mut step = parent.step(0, "step").expect("step admitted");
        let (step_temp, _) = read_temp(&mut step, work.path());
        assert_eq!(step_temp, seen);
        // The step succeeded; the run it belongs to has not finished.
        assert!(step.close(WorkloadOutcome::Succeeded).is_empty());
        let undecided =
            std::fs::read_to_string(Path::new(&seen).join(".effigy-scratch.json")).expect("marker");
        assert!(
            !undecided.contains("keep"),
            "a step must not write the parent's verdict: {undecided}"
        );
        // The owning run dies without deciding, as a kill leaves it.
        parent.abandon_for_tests();
        let successor = TaskRun::begin(TaskRunRequest {
            target: &target,
            lock_wait_ms: 0,
            custody: false,
            scratch: false,
            managed_run_token: None,
            args: &[],
            key: None,
            admission: None,
            workspace_admission: None,
            cwd: None,
        })
        .expect("the successor is admitted");
        assert!(successor.close(WorkloadOutcome::Succeeded).is_empty());
        seen
    });
    assert!(
        Path::new(&seen).exists(),
        "the interrupted run's scratch is retained, not deleted by its step's success"
    );
    let marker =
        std::fs::read_to_string(Path::new(&seen).join(".effigy-scratch.json")).expect("marker");
    assert!(!marker.contains("keep"), "{marker}");
}

#[test]
fn a_scratch_failure_keeps_its_full_retention_from_the_failure() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "test", &catalog);
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);

    let seen = crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
        let mut run = begin_scratching(&target);
        let (seen, _) = read_temp(&mut run, work.path());
        assert!(!run.close(WorkloadOutcome::Failed { code: 1 }).is_empty());
        seen
    });
    let marker: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(Path::new(&seen).join(".effigy-scratch.json")).expect("marker"),
    )
    .expect("marker json");
    let concluded = marker["concluded_ms"]
        .as_i64()
        .expect("the marker records when the failure ended retention");
    let created = marker["created_ms"].as_i64().expect("created");
    assert_eq!(marker["keep"], serde_json::Value::Bool(true));
    assert!(
        concluded >= created,
        "retention starts when the run ended: {marker}"
    );
}

#[test]
fn a_second_keyed_run_gets_its_own_scratch_while_the_first_is_retained() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "test", &catalog);
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);
    let first_key = RunKey::parse("scratch-first").expect("key");
    let second_key = RunKey::parse("scratch-second").expect("key");

    crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
        let mut first = TaskRun::begin(TaskRunRequest {
            target: &target,
            lock_wait_ms: 0,
            custody: true,
            scratch: true,
            managed_run_token: None,
            args: &[],
            key: Some(&first_key),
            admission: None,
            workspace_admission: None,
            cwd: None,
        })
        .expect("first keyed run");
        let (first_temp, _) = read_temp(&mut first, work.path());
        assert!(!first.close(WorkloadOutcome::Failed { code: 1 }).is_empty());

        // The failed run's directory is retained; a different key still
        // allocates its own, whatever the run ids look like.
        let mut second = TaskRun::begin(TaskRunRequest {
            target: &target,
            lock_wait_ms: 0,
            custody: true,
            scratch: true,
            managed_run_token: None,
            args: &[],
            key: Some(&second_key),
            admission: None,
            workspace_admission: None,
            cwd: None,
        })
        .expect("second keyed run");
        let (second_temp, _) = read_temp(&mut second, work.path());
        assert_ne!(first_temp, second_temp);
        assert!(
            Path::new(&first_temp).exists(),
            "the failure stays retained"
        );
        assert!(Path::new(&second_temp).starts_with(&config.root));
        assert!(second.close(WorkloadOutcome::Succeeded).is_empty());
        assert!(!Path::new(&second_temp).exists());
    });
}

#[test]
fn the_builtin_test_routes_run_holds_a_sentinel_so_a_kill_still_settles_it() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);

    crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
        // The route's run owns host suites and the scratch directory, so it
        // must have a sentinel: a killed owner is settled from the journal,
        // the directory is retained, and the sweep can expire it. Without
        // one the row stays `starting` with no cleanup and nothing expires.
        let mut run = TaskRun::standalone_scratching(root.path(), "effigy test")
            .expect("the built-in run is admitted");
        let id = run.id();
        assert!(
            run.run_mut().expect("durable test run").owns_sentinel(),
            "the built-in test route's run must have a sentinel"
        );
        let env = crate::runner::scratch::temp_env();
        assert_eq!(env.len(), 3, "the run owns the scratch directory");
        assert!(
            run.close(WorkloadOutcome::Succeeded).is_empty(),
            "the route's settle reported no residue"
        );
        // Nothing is left for the sweep: the route's own settle released it.
        assert!(
            !crate::runner::scratch::path_for_tests(root.path(), &id).exists(),
            "the settled route released its directory"
        );
    });
}

#[test]
fn a_built_in_suite_settles_its_process_tree_before_the_scratch_is_released() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let _ = catalog;
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);
    let seen = work.path().join("suite-tmpdir.txt");
    let late = work.path().join("descendant.txt");

    crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
        let mut parent = TaskRun::standalone_scratching(root.path(), "effigy test")
            .expect("the built-in run is admitted");
        let command = format!(
            "printf '%s' \"$TMPDIR\" > {}; ( sleep 1; printf late > {} ) & exit 0",
            seen.display(),
            late.display()
        );
        {
            use effigy_builtin::BuiltinRuntimePorts;
            let ports =
                crate::runner::builtin_ports::RunnerBuiltinPorts::with_suite_run(&mut parent);
            assert!(ports.custody_holds_host_suites());
            // Exactly what the runner hands the built-in: the run's temp
            // environment.
            let env = ports.host_scratch_env();
            let mut suite_env = std::collections::BTreeMap::new();
            for (key, value) in env {
                suite_env.insert(key, value.to_string_lossy().into_owned());
            }
            let mut custody = ports
                .host_suite_custody("suite")
                .expect("custody available")
                .expect("a held run owns its suites");
            let outcome = custody
                .run(&command, work.path(), &suite_env, false)
                .expect("the suite ran");
            assert_eq!(outcome.code, Some(0));
            // The attempt's own outcome closes its child run, which stops the
            // suite's whole process group.
            custody.settle(&outcome);
        }
        assert!(parent.close(WorkloadOutcome::Succeeded).is_empty());
    });

    let suite_temp = std::fs::read_to_string(&seen).expect("the suite saw its temp dir");
    assert!(Path::new(&suite_temp).starts_with(&config.root));
    // The backgrounded descendant would write after a second; the suite's
    // settle stopped its group first, so it never gets there.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(
        !late.exists(),
        "a suite's descendant must not outlive the run that owns it"
    );
}

#[test]
fn the_builtin_test_route_holds_and_settles_the_configured_scratch() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let catalog = catalog(root.path(), "", "root", 0);
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);

    // The built-in `test` route takes this exact run before the runner hands
    // its env to the suites: one directory, settled with the built-in's own
    // outcome.
    let (seen, warnings) =
        crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
            let run = TaskRun::standalone_scratching(root.path(), "effigy test")
                .expect("the built-in run is admitted");
            let env = crate::runner::scratch::temp_env();
            assert!(!env.is_empty(), "the built-in test route owns a scratch");
            let seen = env
                .iter()
                .find(|(key, _)| *key == "TMPDIR")
                .map(|(_, value)| value.display().to_string())
                .expect("TMPDIR");
            assert!(Path::new(&seen).starts_with(&config.root));
            assert_eq!(
                Path::new(&seen).file_name().and_then(|name| name.to_str()),
                Some(crate::runner::scratch::name_for_tests(&run.id()).as_str())
            );
            // Every host suite of that run reads this through the ports.
            let ports = crate::runner::builtin_ports::RunnerBuiltinPorts::new();
            use effigy_builtin::BuiltinRuntimePorts;
            let port_env = ports.host_scratch_env();
            assert!(
                port_env
                    .iter()
                    .any(|(key, value)| key == "TMPDIR" && Path::new(value) == Path::new(&seen)),
                "the suite port carries the run's scratch: {port_env:?}"
            );
            (seen, run.close(WorkloadOutcome::Succeeded))
        });
    let _ = &catalog;
    assert!(
        !Path::new(&seen).exists(),
        "the built-in run's settle removes its scratch on success"
    );
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_nested_run_that_opts_out_hides_its_parents_scratch() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let parent_target = TaskTarget::for_catalog(root.path(), "parent", &catalog);
    let child_target = TaskTarget::for_catalog(root.path(), "child", &catalog);
    let scratch_root = root.path().join("scratch-vol");
    std::fs::create_dir(&scratch_root).expect("scratch root");
    let config = scratch_config(&scratch_root);

    crate::runner::scratch::with_test_scratch(Some(config.root.clone()), true, || {
        let mut parent = begin_scratching(&parent_target);
        let (parent_temp, _) = read_temp(&mut parent, work.path());
        assert!(Path::new(&parent_temp).starts_with(&config.root));

        // A nested task reference on this thread: the child declares
        // `scratch = false`, so its host processes see the inherited temp
        // environment, not its parent's directory.
        let _parent_scope = parent.enter();
        let mut child = TaskRun::begin(TaskRunRequest {
            target: &child_target,
            lock_wait_ms: 0,
            custody: true,
            scratch: false,
            managed_run_token: None,
            args: &[],
            key: None,
            admission: None,
            workspace_admission: None,
            cwd: None,
        })
        .expect("the nested run is admitted");
        assert!(
            crate::runner::scratch::temp_env().is_empty(),
            "an opted-out nested run hides its parent's scratch"
        );
        let (child_temp, _) = read_temp(&mut child, work.path());
        assert_ne!(child_temp, parent_temp);
        assert!(!Path::new(&child_temp).starts_with(&config.root));
        assert!(child.close(WorkloadOutcome::Succeeded).is_empty());
        drop(_parent_scope);

        // The parent's scope is back once the child ends.
        let (restored, _) = read_temp(&mut parent, work.path());
        assert_eq!(restored, parent_temp);
        assert!(parent.close(WorkloadOutcome::Succeeded).is_empty());
        assert!(!Path::new(&parent_temp).exists());
    });
}

#[test]
fn without_a_configured_root_a_host_run_sees_the_inherited_temp_environment() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("root");
    let work = tempfile::tempdir().expect("work");
    let catalog = catalog(root.path(), "", "root", 0);
    let target = TaskTarget::for_catalog(root.path(), "build", &catalog);

    crate::runner::scratch::with_test_scratch(None, true, || {
        let mut run = begin_scratching(&target);
        let inherited = std::env::var("TMPDIR").unwrap_or_default();
        let (seen, _) = read_temp(&mut run, work.path());
        assert_eq!(seen, inherited, "no config changes nothing about the env");
        assert!(run.close(WorkloadOutcome::Succeeded).is_empty());
    });
}

#[test]
fn checkout_journals_are_independent_while_another_checkout_writer_is_busy() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let a = TaskTarget::for_catalog(first.path(), "build", &catalog(first.path(), "", "", 0));
    let b = TaskTarget::for_catalog(second.path(), "build", &catalog(second.path(), "", "", 0));
    let run_a = begin(&a).unwrap();
    let state_a = super::kernel::effects_root(first.path()).unwrap();
    let cx = super::kernel::effects();
    let mut writer = effigy_kernel::store::Store::open(&cx, state_a).unwrap();
    writer
        .txn(&cx, |_| {
            std::thread::spawn(move || {
                let run_b =
                    begin(&b).expect("a peer checkout's writer is not part of this admission");
                assert!(second.path().join(".effigy/state/kernel.db").is_file());
                assert!(!super::kernel::test_state_home()
                    .join(".effigy/state/kernel.db")
                    .exists());
                let runs = super::control::list(Some(second.path()), 20).unwrap();
                assert_eq!(runs.runs.len(), 1);
                assert_eq!(runs.runs[0].id, run_b.id());
                assert!(run_b.close(WorkloadOutcome::Succeeded).is_empty());
            })
            .join()
            .unwrap();
            Ok::<_, effigy_kernel::store::StoreError>(())
        })
        .unwrap();
    assert!(run_a.close(WorkloadOutcome::Succeeded).is_empty());
}

#[test]
fn a_nested_catalog_directory_opens_the_enclosing_checkouts_state_root() {
    // Contract 052 §7.1: durable checkout state belongs to the resolved
    // invocation root, so a nested catalog directory with no Git metadata of
    // its own declares no second checkout. The state root and the Git
    // directory the kernel binds must agree, or every run in a subdirectory
    // refuses as untrusted state.
    let fixture = tempfile::tempdir().unwrap();
    std::fs::create_dir(fixture.path().join(".git")).unwrap();
    // The state root is opened through a directory descriptor, which
    // canonicalizes: macOS `tempfile` hands back `/var/...` for
    // `/private/var/...`, so both sides of every comparison below are the
    // resolved checkout.
    let checkout = std::fs::canonicalize(fixture.path()).unwrap();
    let nested = checkout.join("tests/fixtures/nested-catalog");
    std::fs::create_dir_all(&nested).unwrap();

    let enclosing = super::kernel::effects_root(&checkout).unwrap();
    let from_nested = super::kernel::effects_root(&nested).unwrap();
    assert_eq!(
        from_nested.state_directory().path(),
        enclosing.state_directory().path(),
        "a nested catalog shares the enclosing checkout's journal"
    );
    assert_eq!(
        from_nested.owning_directory().path(),
        checkout.as_path(),
        "the journal is owned by the checkout, not the nested catalog's cwd"
    );
    assert!(
        enclosing
            .state_directory()
            .path()
            .starts_with(checkout.join(".effigy")),
        "the checkout keeps its own state: {}",
        enclosing.state_directory().path().display()
    );
}

/// Q-015: the focused task's default wait applies only when neither the flag
/// nor the environment names a wait, and an explicit `0` still fails fast.
#[test]
fn only_an_unset_wait_takes_the_argument_scoped_default() {
    let _guard = crate::contract_test_support::lock_test();
    std::env::remove_var(super::run::LOCK_WAIT_ENV);
    let default = super::identity::ARGUMENT_SCOPED_LOCK_WAIT_MS;
    assert_eq!(
        resolve_lock_wait_ms_or(None, default).expect("default"),
        default
    );
    assert_eq!(
        resolve_lock_wait_ms_or(Some(0), default).expect("explicit zero"),
        0,
        "an explicit `--lock-wait-ms 0` fails fast"
    );
    assert_eq!(
        resolve_lock_wait_ms_or(Some(250), default).expect("explicit wait"),
        250
    );

    std::env::set_var(super::run::LOCK_WAIT_ENV, "0");
    assert_eq!(
        resolve_lock_wait_ms_or(None, default).expect("environment zero"),
        0,
        "an explicit `EFFIGY_LOCK_WAIT_MS=0` fails fast"
    );
    std::env::set_var(super::run::LOCK_WAIT_ENV, "5000");
    assert_eq!(
        resolve_lock_wait_ms_or(None, default).expect("environment wait"),
        5000
    );
    std::env::remove_var(super::run::LOCK_WAIT_ENV);

    assert_eq!(
        resolve_lock_wait_ms_or(None, 0).expect("ordinary selector"),
        0,
        "every other selector still fails fast by default"
    );
}

/// The reservation a host dispatcher writes before an engine admits: the exact
/// run, session and request key an `AdmissionTicket` names.
fn reserve_ticket(
    root: &Path,
    session: effigy_kernel::run::Session,
    marker: u8,
    digest: &[u8],
) -> effigy_kernel::run::Reservation {
    use effigy_kernel::id::{Digest, RunId};
    use effigy_kernel::run::{AdmissionKey, RequestId};

    let cx = kernel::effects();
    let state = kernel::effects_root(root).expect("effects root");
    let run = RunId::new(1_760_000_000_000, [marker; 10]).expect("run id");
    effigy_kernel::run::reserve(
        &cx,
        state,
        AdmissionKey {
            session,
            request: RequestId::new([marker; 16]),
        },
        Digest::of(digest),
        run,
    )
    .expect("the host records this request")
}

fn ticketed(
    target: &TaskTarget,
    ticket: effigy_kernel::run::AdmissionTicket,
) -> Result<TaskRun, RunnerError> {
    TaskRun::begin(TaskRunRequest {
        target,
        lock_wait_ms: 0,
        custody: false,
        scratch: false,
        managed_run_token: None,
        args: &[],
        key: None,
        admission: None,
        workspace_admission: Some(ticket),
        cwd: None,
    })
}

#[test]
fn a_workspace_ticket_admits_exactly_its_reserved_run_and_never_mints_one() {
    use effigy_kernel::run::{Recovered, Reservation, Session};

    let root = tempfile::tempdir().expect("root");
    let root_catalog = catalog(root.path(), "", "", 0);
    // A live workspace session run: a reservation names an admitted session,
    // never caller-chosen identity bytes.
    let session_run = begin(&TaskTarget::for_catalog(
        root.path(),
        "session",
        &root_catalog,
    ))
    .expect("a live workspace session");
    let session = Session {
        run: session_run.id(),
        generation: effigy_kernel::id::Generation::new(1),
    };
    let target = TaskTarget::for_catalog(root.path(), "build", &root_catalog);
    let ticket = match reserve_ticket(root.path(), session, 7, b"guest request") {
        Reservation::Fresh(ticket) => ticket,
        other => panic!("a fresh key is reserved once: {other:?}"),
    };

    let run = ticketed(&target, ticket.clone()).expect("the reserved run is admitted");
    assert_eq!(run.id(), ticket.run(), "the engine admits the ticket's run");
    assert!(run.close(WorkloadOutcome::Succeeded).is_empty());

    // Replay: the same key reads the host's own record instead of admitting a
    // second run, and the spent ticket can never admit again.
    match reserve_ticket(root.path(), session, 7, b"guest request") {
        Reservation::Recorded(Recovered::Completed { run, .. })
        | Reservation::Recorded(Recovered::Admitted { run, .. }) => {
            assert_eq!(run, ticket.run(), "the replay names the admitted run")
        }
        other => panic!("a completed request is answered from its record: {other:?}"),
    }
    let error = ticketed(&target, ticket).expect_err("an admitted ticket cannot admit twice");
    assert!(!error.to_string().is_empty(), "{error}");
    assert!(session_run.close(WorkloadOutcome::Succeeded).is_empty());
}

#[test]
fn a_workspace_ticket_refuses_every_shape_that_would_own_another_run() {
    use super::run::workspace_admission_refusal;

    assert!(workspace_admission_refusal(true, false, false, false).is_none());
    for (task_gate, nested, keyed, shared) in [
        (false, false, false, false),
        (true, true, false, false),
        (true, false, true, false),
        (true, false, false, true),
    ] {
        let refusal = workspace_admission_refusal(task_gate, nested, keyed, shared)
            .expect("this shape cannot honour an exact ticket");
        assert!(
            refusal.to_string().contains("workspace-admitted"),
            "{refusal}"
        );
    }
}

/// The reserved path must pass the secret reader to the actual spawned helper.
#[test]
#[cfg(feature = "workspace-serving-fixture")]
#[ignore = "build the current Effigy binary with test:workspace:effigy-build before this process proof"]
fn reserved_engine_refuses_changed_task_before_any_fallback_effect() {
    use effigy_kernel::run::{Reservation, Session};
    use effigy_kernel::supervise::Executable;
    use effigy_kernel::workspace_channel::{
        InvocationBody, OutputMode, Resolver, Selector, Target,
    };
    let _lock = crate::contract_test_support::lock_test();
    let fixture = tempfile::tempdir().expect("fixture");
    let checkout = fixture.path().join("checkout");
    std::fs::create_dir(&checkout).unwrap();
    let manifest = checkout.join("effigy.toml");
    std::fs::write(
        &manifest,
        "[tasks.build]\nmode = 'tui'\nconcurrent = [{ name = 'one', run = 'echo original' }]\n",
    )
    .unwrap();
    let cx = kernel::effects();
    let state = kernel::effects_root(&checkout).unwrap();
    let identity = state.owning_directory().identity().unwrap();
    let catalog = catalog(&checkout, "", "", 0);
    let session_run = begin(&TaskTarget::for_catalog(&checkout, "session", &catalog)).unwrap();
    let session = Session {
        run: session_run.id(),
        generation: effigy_kernel::id::Generation::new(1),
    };
    let ticket = match reserve_ticket(&checkout, session, 79, b"workspace drift") {
        Reservation::Fresh(ticket) => ticket,
        other => panic!("fresh reservation: {other:?}"),
    };
    let mut dispatch = crate::runner::workspace_client::host_dispatch::HostDispatch::new(
        checkout.clone(),
        state.clone(),
    );
    let body = InvocationBody {
        selector: Selector::Task("build".into()),
        cwd: ".".into(),
        target: Target::Host,
        args: vec![],
        output: OutputMode::Text,
    };
    let resolved = dispatch.resolve(&cx, &body).unwrap();
    let resolution = crate::runner::managed_run::WorkspaceResolution {
        invocation: serde_json::to_vec(&resolved).unwrap(),
        checkout: checkout.clone(),
        identity,
    };
    let marker = checkout.join("unexpected-effect");
    std::fs::write(
        &manifest,
        format!("[tasks]\nbuild = 'touch {}'\n", marker.display()),
    )
    .unwrap();
    let binary = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/effigy");
    let executable = Executable::capture(&binary)
        .expect("build current Effigy with test:workspace:effigy-build first");
    let invocation = effigy_cli::TaskInvocation {
        name: "build".into(),
        args: vec![],
    };
    let result = crate::runner::managed_run::start_reserved_with_executable(
        &checkout,
        &invocation,
        checkout.clone(),
        ticket,
        Some(resolution),
        &executable,
    );
    let refused = result.is_err();
    drop(result);
    let effects_absent = !marker.exists();
    let warnings = session_run.close(WorkloadOutcome::Succeeded);
    assert!(refused, "a changed task must refuse before admission");
    assert!(
        effects_absent,
        "a changed standard-task fallback ran without the ticket"
    );
    assert!(warnings.is_empty(), "fixture session cleanup: {warnings:?}");
}

#[cfg(feature = "workspace-serving-fixture")]
#[test]
#[ignore = "requires a current binary from test:workspace:effigy-build"]
fn reserved_engine_admits_default_and_named_profile_under_the_reserved_id() {
    use effigy_kernel::run::{Reservation, Session};
    use effigy_kernel::supervise::Executable;
    use effigy_kernel::workspace_channel::{
        InvocationBody, OutputMode, Resolver, Selector, Target,
    };
    let _lock = crate::contract_test_support::lock_test();
    let fixture = tempfile::tempdir().unwrap();
    let checkout = fixture.path().join("checkout");
    std::fs::create_dir(&checkout).unwrap();
    let default_marker = checkout.join("default-ran");
    let admin_marker = checkout.join("admin-ran");
    std::fs::write(checkout.join("effigy.toml"), format!(
        "[tasks.build]\nmode = 'tui'\nconcurrent = [{{ name = 'one', run = 'touch {} ; sleep 2' }}]\n[tasks.build.profiles.admin]\nconcurrent = [{{ name = 'two', run = 'touch {} ; sleep 2' }}]\n", default_marker.display(), admin_marker.display(),
    )).unwrap();
    let cx = kernel::effects();
    let state = kernel::effects_root(&checkout).unwrap();
    let identity = state.owning_directory().identity().unwrap();
    let session_run = begin(&TaskTarget::for_catalog(
        &checkout,
        "session",
        &catalog(&checkout, "", "", 0),
    ))
    .unwrap();
    let session = Session {
        run: session_run.id(),
        generation: effigy_kernel::id::Generation::new(1),
    };
    let binary = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/effigy");
    let executable = Executable::capture(&binary)
        .expect("build current Effigy with test:workspace:effigy-build first");
    let mut dispatch = crate::runner::workspace_client::host_dispatch::HostDispatch::new(
        checkout.clone(),
        state.clone(),
    );
    let mut failures = Vec::new();
    for (request_id, args, profile, wanted, unwanted) in [
        (80, vec![], "default", &default_marker, &admin_marker),
        (
            81,
            vec!["admin".to_owned()],
            "admin",
            &admin_marker,
            &default_marker,
        ),
    ] {
        let ticket = match reserve_ticket(&checkout, session, request_id, b"workspace profile") {
            Reservation::Fresh(ticket) => ticket,
            other => {
                failures.push(format!("fresh reservation: {other:?}"));
                continue;
            }
        };
        let expected_run = ticket.run();
        let body = InvocationBody {
            selector: Selector::Task("build".into()),
            cwd: ".".into(),
            target: Target::Host,
            args: args.clone(),
            output: OutputMode::Text,
        };
        let resolved = match dispatch.resolve(&cx, &body) {
            Ok(resolved) => resolved,
            Err(error) => {
                failures.push(format!("resolve {profile}: {error:?}"));
                continue;
            }
        };
        let resolution = crate::runner::managed_run::WorkspaceResolution {
            invocation: serde_json::to_vec(&resolved).unwrap(),
            checkout: checkout.clone(),
            identity,
        };
        let invocation = effigy_cli::TaskInvocation {
            name: "build".into(),
            args,
        };
        let mut started = match crate::runner::managed_run::start_reserved_with_executable(
            &checkout,
            &invocation,
            checkout.clone(),
            ticket,
            Some(resolution),
            &executable,
        ) {
            Ok(started) => started,
            Err(error) => {
                failures.push(format!("start {profile}: {error}"));
                continue;
            }
        };
        if started.run != expected_run || started.client.run != expected_run {
            failures.push(format!("{profile}: wrong admitted run"));
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !wanted.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if !wanted.exists() || unwanted.exists() {
            failures.push(format!("{profile}: wrong workload marker"));
        }
        let observation = kernel::observe();
        match effigy_kernel::store::Store::read(&observation, &started.root) {
            effigy_kernel::observe::Observed::Verified(reader, _) => {
                match effigy_kernel::run::summary(&reader, expected_run) {
                    effigy_kernel::observe::Observed::Verified(summary, _)
                        if summary
                            .invocation
                            .as_ref()
                            .and_then(|inv| inv.spec().task.profile())
                            == Some(profile) => {}
                    other => {
                        failures.push(format!("{profile}: wrong journal invocation: {other:?}"))
                    }
                }
            }
            other => failures.push(format!("{profile}: cannot read journal: {other:?}")),
        }
        started.client.close_input();
        if let Err(error) = started
            .client
            .wait(cx.child(std::time::Duration::from_secs(30)).operation())
        {
            failures.push(format!("{profile}: supervisor reap: {error}"));
        }
        match effigy_kernel::store::Store::read(&observation, &started.root) {
            effigy_kernel::observe::Observed::Verified(reader, _) => {
                match effigy_kernel::run::summary(&reader, expected_run) {
                    effigy_kernel::observe::Observed::Verified(summary, _)
                        if summary.cleanup_complete == Some(true) => {}
                    other => failures.push(format!("{profile}: cleanup not complete: {other:?}")),
                }
            }
            other => failures.push(format!("{profile}: cannot read cleanup: {other:?}")),
        }
        if wanted.exists() {
            if let Err(error) = std::fs::remove_file(wanted) {
                failures.push(format!("{profile}: remove marker: {error}"));
            }
        }
        if unwanted.exists() {
            if let Err(error) = std::fs::remove_file(unwanted) {
                failures.push(format!("{profile}: remove unexpected marker: {error}"));
            }
        }
    }
    let warnings = session_run.close(WorkloadOutcome::Succeeded);
    assert!(warnings.is_empty(), "session cleanup: {warnings:?}");
    assert!(
        failures.is_empty(),
        "reserved profile proof failures: {failures:?}"
    );
}

#[test]
fn reserved_child_receives_grant_at_handoff_fd() {
    use effigy_kernel::run::{Reservation, Session};
    use effigy_kernel::supervise::Executable;
    use std::os::unix::fs::PermissionsExt;

    let _lock = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().expect("fixture");
    let helper = root.path().join("helper");
    let observed = root.path().join("observed");
    std::fs::write(&helper, format!(
        "#!/bin/sh\nif [ \"$EFFIGY_INTERNAL_SECRET_GRANT\" = 1 ]; then /bin/cat /dev/fd/11 > '{}' ; else echo absent > '{}'; fi\n",
        observed.display(), observed.display()
    )).expect("helper");
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    let executable = Executable::capture(&helper).expect("executable");
    let root_catalog = catalog(root.path(), "", "", 0);
    let session_run = begin(&TaskTarget::for_catalog(
        root.path(),
        "session",
        &root_catalog,
    ))
    .expect("session run");
    let session = Session {
        run: session_run.id(),
        generation: effigy_kernel::id::Generation::new(1),
    };
    let ticket = match reserve_ticket(root.path(), session, 77, b"child grant") {
        Reservation::Fresh(ticket) => ticket,
        other => panic!("fresh reservation: {other:?}"),
    };
    // This nextest process owns its environment and the shared test lock.
    unsafe {
        std::env::remove_var("EFFIGY_TEST_SECRETS_PASSPHRASE");
        std::env::remove_var("EFFIGY_SECRET_PASSPHRASE");
    }
    crate::runner::secret_session::forget_session_passphrase_for_tests();
    let grant = crate::runner::secret_session::activate_local_dev_secret_access(true);
    let invocation = effigy_cli::TaskInvocation {
        name: "test".into(),
        args: vec![],
    };
    let result = crate::runner::managed_run::start_reserved_with_executable(
        root.path(),
        &invocation,
        root.path().to_owned(),
        ticket,
        None,
        &executable,
    );
    let bytes = std::fs::read(&observed)
        .unwrap_or_else(|error| panic!("child did not write: {error}; launch: {:?}", result.err()));
    assert!(
        bytes.starts_with(b"EFFGRNT1"),
        "child received grant frame: {bytes:?}"
    );
    drop(grant);
    let ticket = match reserve_ticket(root.path(), session, 78, b"without grant") {
        Reservation::Fresh(ticket) => ticket,
        other => panic!("fresh reservation: {other:?}"),
    };
    let _ = crate::runner::managed_run::start_reserved_with_executable(
        root.path(),
        &invocation,
        root.path().to_owned(),
        ticket,
        None,
        &executable,
    );
    assert_eq!(
        std::fs::read(&observed).expect("child wrote absence"),
        b"absent\n"
    );
    assert!(session_run.close(WorkloadOutcome::Succeeded).is_empty());
}
