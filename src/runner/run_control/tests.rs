//! Run-control proofs on real custody runs (Q-006, papercut 910c578a).
//!
//! A follower that loses its stream before, at or after settle reattaches by
//! exact run id and byte offset: every byte arrives once and in order, the
//! run is never resubmitted, and the follower ends with the run's real
//! result. Fault points are the engine hooks `FollowRead`, `FollowSettled`
//! and `FollowDone`, armed only under the `fault-injection` feature.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_core::task_selection::TaskSurface;
use effigy_kernel::custody::Stream;

use super::*;
use crate::runner::task_engine::{
    run_plain, PlainCommand, RunKey, TaskRun, TaskRunRequest, TaskTarget,
};

#[cfg(feature = "fault-injection")]
const LINES: usize = 40;
/// Prints numbered stdout and stderr lines over about a second, then fails
/// with its own code, so a follower sees a live run and then its result.
#[cfg(feature = "fault-injection")]
const CHATTY: &str = "i=0; while [ $i -lt 40 ]; do echo out-$i; echo err-$i >&2; i=$((i+1)); \
                      sleep 0.025; done; exit 3";

#[cfg(feature = "fault-injection")]
fn expected(prefix: &str) -> Vec<u8> {
    (0..LINES)
        .map(|n| format!("{prefix}-{n}\n"))
        .collect::<String>()
        .into_bytes()
}

fn target(root: &Path, task: &str) -> TaskTarget {
    TaskTarget {
        root: root.to_path_buf(),
        catalog: String::new(),
        task: task.to_owned(),
        surface: TaskSurface::Published,
        profile: None,
        shared_lock: None,
        argument_scope: None,
        label: task.to_owned(),
    }
}

/// A unique key per test: the library tests share one state home.
fn key(root: &Path, name: &str) -> RunKey {
    let unique = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("root")
        .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
    let token =
        effigy_core::worktree_scope::prepare_checkout_token(root).expect("private fixture token");
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&token[index * 2..index * 2 + 2], 16).expect("token byte");
    }
    RunKey::parse(&format!("{name}-{unique}"))
        .expect("key")
        .scoped(effigy_kernel::id::CheckoutId::new(
            effigy_kernel::id::CheckoutKind::Primary,
            bytes,
        ))
        .expect("checkout key")
}

/// Starts `command` as a keyed custody run on another thread; joins to its
/// exit code.
fn start(root: &Path, task: &str, key: &RunKey, command: &str) -> std::thread::JoinHandle<i32> {
    let root = root.to_path_buf();
    let task = task.to_owned();
    let key = key.clone();
    let command = command.to_owned();
    std::thread::spawn(move || {
        let target = target(&root, &task);
        let mut run = TaskRun::begin(TaskRunRequest {
            target: &target,
            lock_wait_ms: 0,
            custody: true,
            scratch: false,
            managed_run_token: None,
            args: &[],
            key: Some(&key),
            admission: None,
            workspace_admission: None,
            cwd: None,
        })
        .expect("keyed run admitted");
        let exit = run_plain(
            &mut run,
            PlainCommand {
                command: &command,
                cwd: &root,
                env: Vec::new(),
                capture: true,
                timeout_ms: None,
            },
        )
        .expect("command ran");
        let warnings = run.close(exit.workload);
        assert!(warnings.is_empty(), "{warnings:?}");
        exit.code
    })
}

fn wait_for(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn found(root: &Path, id: RunId) -> Option<RunRecord> {
    match lookup_checkout(root, id) {
        Lookup::Found(record) => Some(*record),
        _ => None,
    }
}

#[test]
fn unreadable_history_does_not_hide_verified_foreground_custody() {
    let directory = tempfile::tempdir().expect("checkout");
    let target = target(directory.path(), "live");
    let request = || TaskRunRequest {
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
    };
    let durable = TaskRun::begin(request()).expect("initialize durable layout");
    assert!(durable
        .close(effigy_kernel::outcome::WorkloadOutcome::Succeeded)
        .is_empty());
    let memory = TaskRun::begin_foreground(request()).expect("foreground admitted");
    let id = memory.id();
    // No journal handle remains open: this damages only this fixture's
    // completed history while independently held foreground gates stay valid.
    std::fs::write(
        directory.path().join(".effigy/state/kernel.db"),
        b"unreadable history",
    )
    .expect("fixture history damaged");
    let listing = list_runs(Some(directory.path()), 20).expect("partial view");
    assert_eq!(listing.runs.len(), 1);
    assert_eq!(listing.runs[0].id, id);
    assert_eq!(listing.runs[0].view.evidence_source, "live-foreground");
    assert_eq!(listing.runs[0].view.state, "running");
    assert!(listing
        .omitted
        .iter()
        .any(|reason| reason.contains("journal unavailable")));
    let observed = listing.runs[0].clone();
    let state = directory.path().join(".effigy/state");
    std::fs::rename(state.join("gates"), state.join("retired-gates"))
        .expect("replace fixture gates");
    std::fs::create_dir(state.join("gates")).expect("new fixture gates");
    assert!(
        !observed.foreground_released(),
        "a replacement path is not release of the observed inode"
    );
    assert!(matches!(
        memory
            .close(effigy_kernel::outcome::WorkloadOutcome::Succeeded)
            .as_slice(),
        []
    ));
    assert!(
        observed.foreground_released(),
        "the retained gate inode is now free"
    );
    let listing = list_runs(Some(directory.path()), 20).expect("partial history");
    assert!(
        listing.runs.is_empty(),
        "free diagnostic bodies cannot resurrect a run"
    );
    assert!(!listing.omitted.is_empty());
}

/// Collects a follower's output, asserting each chunk starts exactly where
/// the previous one ended: a gap or a duplicate fails here.
#[derive(Default)]
struct Collected {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    expired: Vec<(Stream, u64, u64)>,
    logs_gone: bool,
}

impl Collected {
    fn sink(&mut self) -> impl FnMut(Event<'_>) -> std::io::Result<()> + '_ {
        move |event| {
            match event {
                Event::Bytes {
                    stream,
                    offset,
                    bytes,
                } => {
                    let (sink, base) = match stream {
                        Stream::Stdout => (&mut self.stdout, 0),
                        Stream::Stderr => (&mut self.stderr, 0),
                    };
                    let expired: u64 = self
                        .expired
                        .iter()
                        .filter(|(s, ..)| *s == stream)
                        .map(|(_, from, to)| to - from)
                        .sum();
                    assert_eq!(
                        offset,
                        base + expired + sink.len() as u64,
                        "chunk starts exactly where delivery stopped"
                    );
                    sink.extend_from_slice(bytes);
                }
                Event::Expired { stream, from, to } => self.expired.push((stream, from, to)),
                Event::Truncated { .. } => panic!("no log write failed"),
                Event::LogsGone => self.logs_gone = true,
            }
            Ok(())
        }
    }
}

#[cfg(feature = "fault-injection")]
fn lose_at(point: &'static str, nth: usize) -> impl FnMut(&'static str) -> std::io::Result<()> {
    let mut seen = 0;
    move |name| {
        if name == point {
            seen += 1;
            if seen == nth {
                return Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
            }
        }
        Ok(())
    }
}

/// Follows with the stream lost at the `nth` crossing of `point`, then
/// reattaches from the delivered offsets. Returns the joined output and the
/// reattached ending.
#[cfg(feature = "fault-injection")]
fn follow_losing_once(
    root: &Path,
    id: RunId,
    point: &'static str,
    nth: usize,
) -> (Collected, Ending) {
    let mut collected = Collected::default();
    let mut offsets = Offsets::default();
    let lost = effigy_kernel::fault_injection::with_hook(lose_at(point, nth), || {
        follow(root, id, &mut offsets, true, &mut collected.sink())
    });
    assert!(
        matches!(lost, Err(FollowError::Lost(_))),
        "the injected loss ends the first follower: {lost:?}"
    );
    assert_eq!(
        offsets,
        Offsets {
            stdout: collected.stdout.len() as u64,
            stderr: collected.stderr.len() as u64,
        },
        "offsets hold exactly what was delivered"
    );
    let ending = follow(root, id, &mut offsets, true, &mut collected.sink()).expect("reattached");
    (collected, ending)
}

#[cfg(feature = "fault-injection")]
fn assert_single_settled_run(root: &Path, id: RunId, ending: &Ending, collected: &Collected) {
    let Ending::Settled(record) = ending else {
        panic!("a follower ends at settle");
    };
    assert_eq!(record.id, id, "bound to the exact run");
    assert_eq!(record.view.state, "failed");
    assert_eq!(
        record.exit_code(),
        3,
        "the run's real result, never unknown"
    );
    assert_eq!(
        collected.stdout,
        expected("out"),
        "stdout: no gap, no duplicate"
    );
    assert_eq!(
        collected.stderr,
        expected("err"),
        "stderr: no gap, no duplicate"
    );
    let runs = list_runs(Some(root), 50).expect("runs");
    assert_eq!(runs.runs.len(), 1, "the run was never resubmitted");
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_follower_lost_mid_run_reattaches_by_exact_offset() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "lost-mid-run");
    let run = start(root.path(), "chatty", &key, CHATTY);
    wait_for("the run", || found(root.path(), key.id()).is_some());
    let (collected, ending) = follow_losing_once(root.path(), key.id(), "FollowRead", 3);
    assert_eq!(run.join().expect("run thread"), 3);
    assert_single_settled_run(root.path(), key.id(), &ending, &collected);
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_stream_dropped_at_settle_reattaches_to_the_rest_and_the_outcome() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "lost-at-settle");
    let run = start(root.path(), "chatty", &key, CHATTY);
    wait_for("the run", || found(root.path(), key.id()).is_some());
    let (collected, ending) = follow_losing_once(root.path(), key.id(), "FollowSettled", 1);
    assert_eq!(run.join().expect("run thread"), 3);
    assert_single_settled_run(root.path(), key.id(), &ending, &collected);
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_stream_lost_after_settle_reattaches_to_the_outcome_alone() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "lost-after-settle");
    let run = start(root.path(), "chatty", &key, CHATTY);
    wait_for("the run", || found(root.path(), key.id()).is_some());
    let mut collected = Collected::default();
    let mut offsets = Offsets::default();
    let lost = effigy_kernel::fault_injection::with_hook(lose_at("FollowDone", 1), || {
        follow(
            root.path(),
            key.id(),
            &mut offsets,
            true,
            &mut collected.sink(),
        )
    });
    assert!(matches!(lost, Err(FollowError::Lost(_))));
    let delivered = collected.stdout.len() + collected.stderr.len();
    let ending = follow(
        root.path(),
        key.id(),
        &mut offsets,
        true,
        &mut collected.sink(),
    )
    .expect("reattached");
    assert_eq!(
        collected.stdout.len() + collected.stderr.len(),
        delivered,
        "everything had arrived; the reattach adds no byte"
    );
    assert_eq!(run.join().expect("run thread"), 3);
    assert_single_settled_run(root.path(), key.id(), &ending, &collected);
}

#[test]
fn a_terminal_run_without_logs_still_reports_its_settled_outcome() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "logs-gone");
    assert_eq!(
        start(root.path(), "quick", &key, "echo hi; exit 5")
            .join()
            .expect("run thread"),
        5
    );
    // Retention removes a settled run's logs before its row.
    let logs: PathBuf = root
        .path()
        .join(".effigy/state/runs")
        .join(key.id().to_string());
    std::fs::remove_dir_all(&logs).expect("remove the run's logs");
    let mut collected = Collected::default();
    let ending = follow(
        root.path(),
        key.id(),
        &mut Offsets::default(),
        true,
        &mut collected.sink(),
    )
    .expect("journal still answers");
    let Ending::Settled(record) = ending else {
        panic!("terminal run");
    };
    assert!(collected.logs_gone, "the missing log is reported");
    assert_eq!(
        record.exit_code(),
        5,
        "the real result comes from the journal"
    );
}

#[test]
fn expired_log_bytes_are_reported_never_skipped_silently() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "expired");
    // Six 1 MiB segments; the log keeps the newest four.
    let six_mib = "head -c 6291456 /dev/zero | tr '\\0' x";
    assert_eq!(
        start(root.path(), "flood", &key, six_mib)
            .join()
            .expect("run thread"),
        0
    );
    let mut collected = Collected::default();
    let mut offsets = Offsets::default();
    follow(
        root.path(),
        key.id(),
        &mut offsets,
        true,
        &mut collected.sink(),
    )
    .expect("followed");
    assert_eq!(
        collected.expired,
        vec![(Stream::Stdout, 0, 2 << 20)],
        "the two oldest segments are reported as expired"
    );
    assert_eq!(collected.stdout.len(), 4 << 20);
    assert_eq!(offsets.stdout, 6 << 20, "the next offset is the log's end");
}

#[test]
fn keyed_contender_observes_the_run_published_after_its_lookup() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "published-race");
    let (looked_up, observed) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let contender_root = root.path().to_path_buf();
    let contender_key = key.clone();
    let contender = std::thread::spawn(move || {
        crate::runner::task_engine::after_key_lookup_for_test(move || {
            looked_up.send(()).expect("report absent lookup");
            released.recv().expect("release contender");
        });
        TaskRun::begin(TaskRunRequest {
            target: &target(&contender_root, "build"),
            lock_wait_ms: 0,
            custody: false,
            scratch: false,
            managed_run_token: None,
            args: &[],
            key: Some(&contender_key),
            admission: None,
            workspace_admission: None,
            cwd: None,
        })
    });
    observed.recv().expect("contender looked up absent key");
    let winner = TaskRun::begin(TaskRunRequest {
        target: &target(root.path(), "build"),
        lock_wait_ms: 0,
        custody: false,
        scratch: false,
        managed_run_token: None,
        args: &[],
        key: Some(&key),
        admission: None,
        workspace_admission: None,
        cwd: None,
    })
    .expect("winner publishes while holding its gate");
    assert_eq!(winner.id(), key.id());
    release.send(()).expect("release contender");
    let loser = contender.join().expect("contender thread");
    assert!(
        matches!(loser, Err(RunnerError::RunKeyExists { ref run }) if *run == key.id().to_string()),
        "contender must name the published run: {loser:?}"
    );
    assert_eq!(
        list_runs(Some(root.path()), 50).expect("runs").runs.len(),
        1
    );
    winner.close(effigy_kernel::outcome::WorkloadOutcome::Succeeded);
}

#[test]
fn different_task_contender_refuses_the_key_published_after_its_lookup() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "foreign-race");
    let (looked_up, observed) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let contender_root = root.path().to_path_buf();
    let contender_key = key.clone();
    let contender = std::thread::spawn(move || {
        crate::runner::task_engine::after_key_lookup_for_test(move || {
            looked_up.send(()).expect("report absent lookup");
            released.recv().expect("release contender");
        });
        TaskRun::begin(TaskRunRequest {
            target: &target(&contender_root, "deploy"),
            lock_wait_ms: 0,
            custody: false,
            scratch: false,
            managed_run_token: None,
            args: &[],
            key: Some(&contender_key),
            admission: None,
            workspace_admission: None,
            cwd: None,
        })
    });
    observed.recv().expect("contender looked up absent key");
    let winner = TaskRun::begin(TaskRunRequest {
        target: &target(root.path(), "build"),
        lock_wait_ms: 0,
        custody: false,
        scratch: false,
        managed_run_token: None,
        args: &[],
        key: Some(&key),
        admission: None,
        workspace_admission: None,
        cwd: None,
    })
    .expect("winner publishes while holding its gate");
    release.send(()).expect("release contender");
    let error = contender
        .join()
        .expect("contender thread")
        .expect_err("different task must refuse");
    assert!(
        error.to_string().contains("already names run"),
        "foreign key must refuse, got {error:?}"
    );
    assert_eq!(
        list_runs(Some(root.path()), 50).expect("runs").runs.len(),
        1
    );
    winner.close(effigy_kernel::outcome::WorkloadOutcome::Succeeded);
}

#[test]
fn keyed_contention_before_publication_refuses_without_attaching() {
    use effigy_kernel::gate::{Gate, Wait};
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "unpublished");
    let target = target(root.path(), "build");
    let checkout = crate::runner::task_engine::checkout::mint(root.path()).expect("checkout");
    let state = crate::runner::task_engine::checkout_effects_root(root.path()).expect("state");
    let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::Shell);
    let owner = target.owner(checkout).expect("owner");
    let gate = Gate::acquire(&cx, state, &[owner], Wait::FailFast).expect("hold task gate");
    let result = TaskRun::begin(TaskRunRequest {
        target: &target,
        lock_wait_ms: 0,
        custody: false,
        scratch: false,
        managed_run_token: None,
        args: &[],
        key: Some(&key),
        admission: None,
        workspace_admission: None,
        cwd: None,
    });
    assert!(
        matches!(result, Err(ref error) if !matches!(error, RunnerError::RunKeyExists { .. })),
        "unpublished gate is not an attachment: {result:?}"
    );
    assert!(list_runs(Some(root.path()), 50)
        .expect("runs")
        .runs
        .is_empty());
    drop(gate);
}

#[test]
fn unreadable_key_journal_refuses_without_attaching() {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "unreadable");
    let state = crate::runner::task_engine::checkout_effects_root(root.path()).expect("state");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(state.path().join(effigy_kernel::store::DB_NAME))
        .expect("owned journal");
    file.write_all(b"not a sqlite journal")
        .expect("write corrupt journal");
    drop(file);
    let result = TaskRun::begin(TaskRunRequest {
        target: &target(root.path(), "build"),
        lock_wait_ms: 0,
        custody: false,
        scratch: false,
        managed_run_token: None,
        args: &[],
        key: Some(&key),
        admission: None,
        workspace_admission: None,
        cwd: None,
    });
    let error = result.expect_err("unreadable journal refuses");
    assert!(error.to_string().contains("cannot be checked"), "{error}");
}

#[test]
fn a_run_key_names_one_run_under_racing_invocations_and_repeats() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "race");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let racers = (0..2)
        .map(|_| {
            let (root, key, barrier) = (root.path().to_path_buf(), key.clone(), barrier.clone());
            std::thread::spawn(move || {
                let target = target(&root, "build");
                barrier.wait();
                TaskRun::begin(TaskRunRequest {
                    target: &target,
                    lock_wait_ms: 0,
                    custody: false,
                    scratch: false,
                    managed_run_token: None,
                    args: &[],
                    key: Some(&key),
                    admission: None,
                    workspace_admission: None,
                    cwd: None,
                })
            })
        })
        .collect::<Vec<_>>();
    let results = racers
        .into_iter()
        .map(|racer| racer.join().expect("racer"))
        .collect::<Vec<_>>();
    let started = results.iter().filter(|result| result.is_ok()).count();
    assert_eq!(started, 1, "exactly one racer starts the run");
    for result in results {
        match result {
            Ok(run) => {
                assert_eq!(run.id(), key.id(), "the key names the run");
                run.close(effigy_kernel::outcome::WorkloadOutcome::Succeeded);
            }
            Err(RunnerError::RunKeyExists { run }) => assert_eq!(run, key.id().to_string()),
            Err(other) => panic!("the other racer attaches, got {other}"),
        }
    }
    let repeat = TaskRun::begin(TaskRunRequest {
        target: &target(root.path(), "build"),
        lock_wait_ms: 0,
        custody: false,
        scratch: false,
        managed_run_token: None,
        args: &[],
        key: Some(&key),
        admission: None,
        workspace_admission: None,
        cwd: None,
    });
    assert!(
        matches!(repeat, Err(RunnerError::RunKeyExists { .. })),
        "a repeat after settle reports the run, never a second one"
    );
    let other = TaskRun::begin(TaskRunRequest {
        target: &target(root.path(), "deploy"),
        lock_wait_ms: 0,
        custody: false,
        scratch: false,
        managed_run_token: None,
        args: &[],
        key: Some(&key),
        admission: None,
        workspace_admission: None,
        cwd: None,
    })
    .expect_err("a key never binds a second task");
    assert!(other.to_string().contains("already names run"), "{other}");
    assert_eq!(
        list_runs(Some(root.path()), 50).expect("runs").runs.len(),
        1
    );
}

#[test]
fn run_keys_are_bounded_and_derive_one_stable_id() {
    let checkout =
        effigy_kernel::id::CheckoutId::new(effigy_kernel::id::CheckoutKind::Primary, [1; 16]);
    let other =
        effigy_kernel::id::CheckoutId::new(effigy_kernel::id::CheckoutKind::Primary, [2; 16]);
    assert_eq!(
        RunKey::parse("ci-1234").expect("key").id(),
        RunKey::parse("ci-1234").expect("key").id()
    );
    assert_ne!(
        RunKey::parse("ci-1234").expect("key").id(),
        RunKey::parse("ci-1235").expect("key").id()
    );
    for bad in ["", "has space", "slash/key", &"k".repeat(RunKey::MAX + 1)] {
        assert!(RunKey::parse(bad).is_err(), "{bad:?} is refused");
    }
    let key = RunKey::parse("same").expect("key");
    assert_eq!(key.scoped(checkout).unwrap(), key.scoped(checkout).unwrap());
    assert_ne!(
        key.scoped(checkout).unwrap().id(),
        key.scoped(other).unwrap().id()
    );
    assert_ne!(
        key.id(),
        key.scoped(checkout).unwrap().id(),
        "legacy ids have a separate domain"
    );
}

#[test]
fn run_stop_settles_a_running_task_and_its_children() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "stop");
    let run = start(
        root.path(),
        "hold",
        &key,
        "sleep 300 & echo $! > child.pid; wait",
    );
    let child = root.path().join("child.pid");
    wait_for("the child", || {
        std::fs::read_to_string(&child).is_ok_and(|pid| pid.ends_with('\n'))
    });
    let child: u32 = std::fs::read_to_string(&child)
        .expect("pid")
        .trim()
        .parse()
        .expect("pid");
    let record = found(root.path(), key.id()).expect("live run");
    assert_eq!(record.view.state, "running");
    assert_eq!(
        request_stop(&record).expect("request"),
        StopStarted::Requested
    );
    assert_eq!(
        run.join().expect("run thread"),
        1,
        "a requested stop records `cancelled`, exit 1"
    );
    let settled = found(root.path(), key.id()).expect("settled run");
    assert_eq!(settled.view.state, "cancelled");
    assert_eq!(settled.view.cleanup, Some("complete"));
    assert!(
        !matches!(
            effigy_kernel::ident::process_identity(child),
            Observed::Verified(..)
        ),
        "the task's background child was stopped with its group"
    );
    assert_eq!(
        request_stop(&settled).expect("idempotent"),
        StopStarted::AlreadyClosed
    );
}

// Honest unknown (KTE-02/03): a follow cannot hang on an observation that
// stays unverifiable, and cannot report bytes it could not read as drained.
// Each fixture damages exactly one observation — a journal event payload for
// the workload read, or stdout's segment files for one stream's read — while
// the journal lookup keeps answering on every poll, so each deadline is
// proven independently of a successful lookup.

use std::os::unix::fs::PermissionsExt;

/// A workload that stays open until the fixture releases it, with no output.
#[cfg(feature = "fault-injection")]
const HOLD_UNTIL: &str = "while [ ! -f release.txt ]; do sleep 0.05; done";
/// Produces stdout lines for about a second, then settles cleanly.
#[cfg(feature = "fault-injection")]
const DRIPS: &str =
    "i=0; while [ $i -lt 20 ]; do echo out-$i; i=$((i+1)); sleep 0.05; done; exit 0";

#[cfg(feature = "fault-injection")]
fn drip_lines() -> Vec<u8> {
    (0..20)
        .map(|n| format!("out-{n}\n"))
        .collect::<String>()
        .into_bytes()
}

/// A run's retained log directory.
fn log_dir(root: &Path, id: RunId) -> PathBuf {
    root.join(".effigy/state/runs").join(id.to_string())
}

/// Shrinks the follow's retry budget for this test process. nextest runs
/// each test in its own process, so no test sees another's budget; the
/// shipped budget stays the full 30 s.
fn arm_unknown_budget(budget: Duration) {
    *TEST_UNKNOWN_BUDGET.lock().expect("budget lock") = Some(budget);
}

#[cfg(feature = "fault-injection")]
fn wait_for_workload(root: &Path, id: RunId) {
    // Steady state: every launch apply has been applied and verified, and
    // the custodian reads the run's effects again only at settle. Fixtures
    // damage evidence only inside this window.
    wait_for("the running workload", || {
        found(root, id).is_some_and(|record| record.view.state == "running")
    });
}

#[cfg(feature = "fault-injection")]
/// Refuses the first `count` crossings of `point`, then lets them through:
/// `usize::MAX` never recovers. Scoped by `with_hook`, so only the caller's
/// own reads fault; the fixture run's custody reads stay clean.
#[cfg(feature = "fault-injection")]
fn refuse_first(
    point: &'static str,
    count: usize,
) -> impl FnMut(&'static str) -> std::io::Result<()> {
    let mut seen = 0usize;
    move |name| {
        if name == point {
            seen += 1;
            if seen > count {
                return Ok(());
            }
            return Err(std::io::Error::other("injected"));
        }
        Ok(())
    }
}

/// Makes every stdout segment unreadable, so reading that one stream fails
/// until the same files are restored; stderr stays readable throughout.
fn hide_stdout_segments(dir: &Path) -> Vec<PathBuf> {
    let mut hidden = Vec::new();
    for entry in std::fs::read_dir(dir).expect("log directory") {
        let path = entry.expect("entry").path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains(".stdout.") && name.ends_with(".log"))
        {
            let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
            permissions.set_mode(0o000);
            std::fs::set_permissions(&path, permissions).expect("hide segment");
            hidden.push(path);
        }
    }
    assert!(
        !hidden.is_empty(),
        "the fixture run captured stdout segments"
    );
    hidden
}

fn restore_segments(hidden: &[PathBuf]) {
    for path in hidden {
        let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions).expect("restore segment");
    }
}

/// A workload observation that never verifies again ends the follow with the
/// reason it saw, inside the budget, although the journal lookup itself
/// answers on every poll. Nothing is reported drained, and the run survives.
#[test]
#[cfg(feature = "fault-injection")]
fn a_run_whose_workload_stays_unverifiable_ends_unknown_within_the_budget() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "workload-unknown");
    let run = start(root.path(), "hold-quiet", &key, HOLD_UNTIL);
    // Steady state: the launch is complete, so the fault affects only this
    // follower's own reads.
    wait_for_workload(root.path(), key.id());
    arm_unknown_budget(Duration::from_millis(600));
    let mut collected = Collected::default();
    let mut offsets = Offsets::default();
    let started = Instant::now();
    let outcome =
        effigy_kernel::fault_injection::with_hook(refuse_first("RunEffects", usize::MAX), || {
            follow(
                root.path(),
                key.id(),
                &mut offsets,
                true,
                &mut collected.sink(),
            )
        });
    let elapsed = started.elapsed();
    std::fs::write(root.path().join("release.txt"), b"").expect("release the run");
    assert_eq!(run.join().expect("run thread"), 0);
    let Err(FollowError::Unknown(reason)) = outcome else {
        panic!("an unverifiable workload ends unknown: {outcome:?}");
    };
    assert!(reason.contains("could not tell"), "{reason}");
    assert!(
        elapsed >= Duration::from_millis(600),
        "the deadline bounds the retries: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the budget ends the follow, not a hang: {elapsed:?}"
    );
    assert_eq!(
        offsets,
        Offsets::default(),
        "nothing was reported as delivered"
    );
    let record = found(root.path(), key.id()).expect("the run is untouched");
    assert!(
        record.closed(),
        "the released run still settles: {record:?}"
    );
    assert_eq!(record.exit_code(), 0);
}

/// A workload observation that fails a few times and then verifies recovers:
/// the follow completes with every byte once, in order, and the run's real
/// result.
#[test]
#[cfg(feature = "fault-injection")]
fn a_briefly_unverifiable_workload_read_recovers_without_loss_or_duplicate() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "workload-transient");
    let run = start(root.path(), "drips", &key, DRIPS);
    wait_for_workload(root.path(), key.id());
    arm_unknown_budget(Duration::from_secs(5));
    let mut collected = Collected::default();
    let mut offsets = Offsets::default();
    let ending = effigy_kernel::fault_injection::with_hook(refuse_first("RunEffects", 3), || {
        follow(
            root.path(),
            key.id(),
            &mut offsets,
            true,
            &mut collected.sink(),
        )
    })
    .expect("the transient failure recovers within the budget");
    assert_eq!(run.join().expect("run thread"), 0);
    let Ending::Settled(record) = ending else {
        panic!("the run settles while followed");
    };
    assert_eq!(record.exit_code(), 0, "the run's real result");
    assert_eq!(
        collected.stdout,
        drip_lines(),
        "every byte arrived once, in order"
    );
    assert_eq!(offsets.stdout, drip_lines().len() as u64);
}

/// A terminal run whose workload evidence stays unverifiable is not reported
/// settled either: the journal lookup keeps answering, the workload
/// observation does not, and the follow ends unknown within the budget.
#[cfg(feature = "fault-injection")]
#[test]
fn a_terminal_run_with_unverifiable_workload_evidence_is_not_reported_settled() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "terminal-workload-unknown");
    assert_eq!(
        start(root.path(), "quick", &key, "echo hi; exit 5")
            .join()
            .expect("run thread"),
        5
    );
    arm_unknown_budget(Duration::from_millis(600));
    let mut collected = Collected::default();
    let mut offsets = Offsets::default();
    let outcome =
        effigy_kernel::fault_injection::with_hook(refuse_first("RunEffects", usize::MAX), || {
            follow(
                root.path(),
                key.id(),
                &mut offsets,
                true,
                &mut collected.sink(),
            )
        });
    assert!(
        matches!(&outcome, Err(FollowError::Unknown(_))),
        "a terminal run with unverifiable workload evidence cannot be settled success: {outcome:?}"
    );
    assert_eq!(offsets, Offsets::default(), "nothing was reported drained");
    let mut collected = Collected::default();
    let ending = follow(
        root.path(),
        key.id(),
        &mut Offsets::default(),
        true,
        &mut collected.sink(),
    )
    .expect("readable without the fault");
    let Ending::Settled(record) = ending else {
        panic!("terminal run");
    };
    assert_eq!(record.exit_code(), 5);
    assert_eq!(collected.stdout, b"hi\n" as &[u8]);
}

/// A terminal run whose one stream cannot be read is not reported settled or
/// drained; once the stream is readable again the same run reports its real
/// result and output.
#[test]
fn a_terminal_run_with_an_unreadable_log_is_not_reported_settled_or_drained() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "unreadable-log");
    assert_eq!(
        start(root.path(), "quick", &key, "echo hi; exit 5")
            .join()
            .expect("run thread"),
        5
    );
    let hidden = hide_stdout_segments(&log_dir(root.path(), key.id()));
    arm_unknown_budget(Duration::from_millis(600));
    let mut collected = Collected::default();
    let mut offsets = Offsets::default();
    let outcome = follow(
        root.path(),
        key.id(),
        &mut offsets,
        true,
        &mut collected.sink(),
    );
    restore_segments(&hidden);
    assert!(
        matches!(&outcome, Err(FollowError::Unknown(reason)) if reason.contains("could not tell")),
        "an unread log cannot become settled success: {outcome:?}"
    );
    assert!(!collected.logs_gone, "a hidden log is not an absent log");
    assert!(
        collected.stdout.is_empty(),
        "no byte was reported delivered"
    );
    assert_eq!(offsets.stdout, 0, "nothing was reported drained");
    let mut collected = Collected::default();
    let ending = follow(
        root.path(),
        key.id(),
        &mut Offsets::default(),
        true,
        &mut collected.sink(),
    )
    .expect("readable again");
    let Ending::Settled(record) = ending else {
        panic!("terminal run");
    };
    assert_eq!(record.exit_code(), 5, "the real result is still there");
    assert_eq!(collected.stdout, b"hi\n" as &[u8]);
}

/// A stream that briefly cannot be read delays the snapshot and then
/// completes it: one exact read of both streams, no gap and no duplicate,
/// ending settled with the run's own result.
#[test]
fn a_briefly_unreadable_stream_recovers_to_one_exact_read() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "readable-again");
    assert_eq!(
        start(root.path(), "quick", &key, "echo hi; echo err >&2; exit 5")
            .join()
            .expect("run thread"),
        5
    );
    let hidden = hide_stdout_segments(&log_dir(root.path(), key.id()));
    let repair = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        restore_segments(&hidden);
    });
    arm_unknown_budget(Duration::from_secs(5));
    let mut collected = Collected::default();
    let mut offsets = Offsets::default();
    let ending = follow(
        root.path(),
        key.id(),
        &mut offsets,
        false,
        &mut collected.sink(),
    )
    .expect("the transient failure recovers within the budget");
    repair.join().expect("repair thread");
    let Ending::Settled(record) = ending else {
        panic!("terminal run");
    };
    assert_eq!(record.exit_code(), 5);
    assert_eq!(collected.stdout, b"hi\n" as &[u8]);
    assert_eq!(collected.stderr, b"err\n" as &[u8]);
    assert_eq!(offsets.stdout, 3);
    assert_eq!(offsets.stderr, 4);
}

/// An open run's snapshot cannot pass while its workload evidence cannot be
/// verified: the single pass ends unknown within the budget instead of
/// returning an incomplete read as success.
#[test]
#[cfg(feature = "fault-injection")]
fn an_open_runs_snapshot_cannot_pass_with_unverifiable_workload_evidence() {
    let root = tempfile::tempdir().expect("root");
    let key = key(root.path(), "open-unread");
    let run = start(root.path(), "hold-quiet", &key, HOLD_UNTIL);
    wait_for_workload(root.path(), key.id());
    // Steady state only: launch applies read the run's effects back, so
    // damaging earlier would break the fixture run itself.
    arm_unknown_budget(Duration::from_millis(600));
    let mut collected = Collected::default();
    let mut offsets = Offsets::default();
    let outcome =
        effigy_kernel::fault_injection::with_hook(refuse_first("RunEffects", usize::MAX), || {
            follow(
                root.path(),
                key.id(),
                &mut offsets,
                false,
                &mut collected.sink(),
            )
        });
    std::fs::write(root.path().join("release.txt"), b"").expect("release the run");
    assert_eq!(run.join().expect("run thread"), 0);
    assert!(
        matches!(outcome, Err(FollowError::Unknown(_))),
        "a snapshot that could not read its stream is not a success: {outcome:?}"
    );
    assert_eq!(offsets, Offsets::default());
}

/// Every observation owns its deadline: another stage's success never
/// retires it, its own success does, and expiry uses the full shipped
/// budget of fake-clock time (the proofs above shrink only their own
/// process's budget).
#[test]
fn each_observation_owns_its_deadline_and_no_other_resets_it() {
    let now = Instant::now();
    let spent = now - UNKNOWN_BUDGET - Duration::from_secs(1);
    let mut deadlines = RetryDeadlines::default();
    deadlines.0[Observation::Workload.slot()] = Some(spent);
    deadlines.0[Observation::Log(Stream::Stdout).slot()] = Some(spent);
    deadlines.observed(Observation::Journal);
    deadlines.observed(Observation::Log(Stream::Stderr));
    assert_eq!(
        deadlines.0[Observation::Workload.slot()],
        Some(spent),
        "another stage's success does not retire a failing stage's deadline"
    );
    assert_eq!(
        deadlines.0[Observation::Log(Stream::Stdout).slot()],
        Some(spent),
        "the other stream's success does not retire this stream's deadline"
    );
    let outcome = deadlines.retrying(Observation::Workload, "the reason".to_owned());
    assert!(
        matches!(outcome, Err(FollowError::Unknown(reason)) if reason == "the reason"),
        "past the budget the follow ends with that observation's reason"
    );
    deadlines.0[Observation::Workload.slot()] =
        Some(now - (UNKNOWN_BUDGET - Duration::from_secs(1)));
    deadlines
        .retrying(Observation::Workload, "again".to_owned())
        .expect("within the budget it keeps retrying");
    deadlines.observed(Observation::Workload);
    assert_eq!(
        deadlines.0[Observation::Workload.slot()],
        None,
        "its own success retires the deadline"
    );
    assert_eq!(
        deadlines.0[Observation::Log(Stream::Stdout).slot()],
        Some(spent),
        "and still nothing else's"
    );
}

/// The typed unknown reaches callers through the existing envelope, with the
/// original reason, under the run schema — no invented error schema.
#[test]
fn an_unknown_follow_maps_to_the_existing_unknown_envelope() {
    let error = follow_error(
        "run-9",
        FollowError::Unknown("the store was busy".to_owned()),
    );
    let RunnerError::RunControl {
        detail,
        details_json,
        ..
    } = error
    else {
        panic!("the typed envelope: {error:?}");
    };
    assert!(detail.contains("outcome unknown"), "{detail}");
    assert!(detail.contains("the store was busy"), "{detail}");
    let envelope: serde_json::Value = serde_json::from_str(&details_json).expect("envelope");
    assert_eq!(envelope["schema"], "effigy.run.v1");
    assert_eq!(envelope["state"], "unknown");
    assert_eq!(envelope["reason"], "the store was busy");
}
