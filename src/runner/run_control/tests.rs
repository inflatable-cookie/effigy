//! Run-control proofs on real custody runs (Q-006, papercut 910c578a).
//!
//! A follower that loses its stream before, at or after settle reattaches by
//! exact run id and byte offset: every byte arrives once and in order, the
//! run is never resubmitted, and the follower ends with the run's real
//! result. Fault points are the engine hooks `FollowRead`, `FollowSettled`
//! and `FollowDone`, armed only under the `fault-injection` feature.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::Engine as _;
use effigy_core::task_selection::TaskSurface;
use effigy_kernel::custody::Stream;

use super::*;
use crate::runner::task_engine::{
    run_plain, PlainCommand, RunKey, TaskRun, TaskRunRequest, TaskTarget,
};

#[test]
fn projected_json_gap_keeps_raw_resume_offset() {
    let bytes = b"31msecret\n";
    let mut projection = PlainStream::unknown();
    let mut stream = StreamJson::new(10);
    stream.bytes.extend(projection.project(bytes));
    let json = stream.json(10 + bytes.len() as u64);
    assert_eq!(json["from"], 10);
    assert_eq!(json["next_offset"], 10 + bytes.len() as u64);
    let data = base64::engine::general_purpose::STANDARD
        .decode(json["data_base64"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        String::from_utf8(data).unwrap(),
        format!("[plain output gap: {} bytes elided]\n", bytes.len())
    );
}

#[test]
fn resumed_reader_replays_raw_prefix_across_csi_and_osc() {
    for (raw, offset, expected) in [
        (b"\x1b[31mred\x1b[0m".as_slice(), 3, b"red".as_slice()),
        (
            b"\x1b]title\x1b\\visible".as_slice(),
            5,
            b"visible".as_slice(),
        ),
    ] {
        let mut projection = resume_with(offset as u64, |start, length| {
            Some(effigy_kernel::run::LogRead {
                retained_from: 0,
                from: start,
                bytes: raw[start as usize..start as usize + length].to_vec(),
                end: raw.len() as u64,
                truncated: false,
            })
        });
        assert_eq!(projection.project(&raw[offset..]), expected);
    }
    let mut projection = resume_with(12, |_, _| None);
    assert_eq!(
        projection.project(b"partial\n"),
        b"[plain output gap: 8 bytes elided]\n"
    );
}

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
                    ..
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
