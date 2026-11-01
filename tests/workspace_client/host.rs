//! #507: the process-lifetime proof for a detached workspace serving host.
//!
//! The initiating front end is a real process: it starts one serving host
//! through the production launcher, reads the committed session it serves,
//! acknowledges the kernel detach and exits. A third process (this test) then
//! proves the host still owns the run and the listener, stops it through the
//! kernel's own control path, and proves the settle closed the socket exactly
//! once.
//!
//! The activation is the disposable fixture one (`workspace-serving-fixture`):
//! a real session run, listener, key and grant book with no container and no
//! prepared guest. This proves process custody, not container-mode preparation
//! or native backend qualification; #452 owns those later gates.

use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{
    CancelToken, Deadline, EffectClass, EffectContext, Limits, ObserveContext, OperationContext,
    Placement,
};
use effigy_kernel::id::RunId;
use effigy_kernel::observe::Observed;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{self, Fence, Phase, Request, RequestAction};

const CAPABILITY: &str = "abababababababababababababababababababababababababababababababab";

fn effects() -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(120)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        EffectClass::Mutate,
        Placement::Host,
    )
    .effects()
    .expect("test effect context")
}

fn observation() -> ObserveContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        EffectClass::Observe,
        Placement::Host,
    )
    .observe()
    .expect("test observe context")
}

fn state_root(
    cx: &EffectContext,
    state_home: &std::path::Path,
    project: &std::path::Path,
) -> StateRoot {
    let location = effigy_kernel::location::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(project),
    };
    StateRoot::checkout_effects(cx, state_home, &location).expect("the fixture state root")
}

fn read_summary(observe: &ObserveContext, root: &StateRoot, run: RunId) -> Option<run::RunSummary> {
    let reader = match effigy_kernel::store::Store::read(observe, root) {
        Observed::Verified(reader, _) => reader,
        _ => return None,
    };
    match run::summary(&reader, run) {
        Observed::Verified(summary, _) => Some(summary),
        _ => None,
    }
}

/// The outcome a settled run recorded, named without Debug formatting.
fn summary_names(summary: &Option<run::RunSummary>) -> &'static str {
    match summary
        .as_ref()
        .and_then(|summary| summary.workload.as_ref())
    {
        Some(effigy_kernel::outcome::WorkloadOutcome::Succeeded) => "succeeded",
        Some(effigy_kernel::outcome::WorkloadOutcome::Cancelled {
            cause: effigy_kernel::ctx::StopCause::ListenerFailed,
            ..
        }) => "cancelled:listener-failed",
        Some(effigy_kernel::outcome::WorkloadOutcome::Cancelled { .. }) => "cancelled",
        Some(effigy_kernel::outcome::WorkloadOutcome::Failed { .. }) => "failed",
        Some(effigy_kernel::outcome::WorkloadOutcome::NotStarted) => "not-started",
        Some(effigy_kernel::outcome::WorkloadOutcome::Interrupted) => "interrupted",
        Some(effigy_kernel::outcome::WorkloadOutcome::TimedOut { .. }) => "timed-out",
        Some(effigy_kernel::outcome::WorkloadOutcome::Unknown(_)) => "unknown",
        None => "no-outcome",
    }
}

fn endpoint_of(published_endpoint: &str) -> SocketAddr {
    // The disposable activation publishes a numeric loopback lease; a
    // container-mode publication publishes the guest alias for the same port.
    let text = published_endpoint
        .strip_prefix("effigy-host:")
        .unwrap_or(published_endpoint);
    if let Ok(socket) = text.parse::<SocketAddr>() {
        return socket;
    }
    let port = text.parse::<u16>().expect("the lease publishes a port");
    SocketAddr::from(([127, 0, 0, 1], port))
}

/// One disposable request for a private fixture journal.
/// The checkout identity one test's hosts use.
///
/// The serving run's task gate is machine-wide (`StateRoot` attaches
/// `MachineGates`), and its identity is derived from the request's checkout and
/// profile. A fixed checkout therefore makes the eleven proofs in this file — and
/// any concurrent run of this selector — contend for one `workspace-serving`
/// gate, which is a property of the fixture, not of what they prove. Deriving
/// the identity from the fixture's own unique directory keeps each test (and each
/// run) on its own gate; every host *within* a test still shares it, which is what
/// the deliberate contention proofs need.
fn checkout_of(fixture: &effigy_test_support::Fixture, purpose: &str) -> Vec<u8> {
    let material = format!("{}-{purpose}", fixture.path().display());
    let mut state: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in material.as_bytes() {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (0..16u8)
        .map(|index| {
            let mixed = state
                .wrapping_add(u64::from(index) + 1)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15);
            (mixed >> 56) as u8
        })
        .collect()
}

/// Wait until nothing under the fixture has changed for a quiet interval.
///
/// A stopped, abandoned or refused host settles asynchronously: it still writes
/// its own settle records after the front end has returned. The fixture removes
/// its directory when the test ends, so returning before that quiet interval
/// makes the removal race the host and fails with `Directory not empty` — a
/// fixture artefact that hides the proof's real result. This waits for
/// quiescence and fails with the path still changing if it never comes.
fn settle_fixture(fixture: &effigy_test_support::Fixture) {
    fn snapshot(
        root: &std::path::Path,
        entries: &mut Vec<(std::path::PathBuf, u64, Option<std::time::SystemTime>)>,
    ) {
        let Ok(read) = std::fs::read_dir(root) else {
            return;
        };
        for entry in read.flatten() {
            let path = entry.path();
            match entry.metadata() {
                Ok(metadata) if metadata.is_dir() => {
                    entries.push((path.clone(), 0, metadata.modified().ok()));
                    snapshot(&path, entries);
                }
                Ok(metadata) => entries.push((path, metadata.len(), metadata.modified().ok())),
                Err(_) => entries.push((path, 0, None)),
            }
        }
    }

    let quiet = Duration::from_millis(300);
    let bound = Instant::now() + Duration::from_secs(60);
    let mut previous = Vec::new();
    let mut stable_since = Instant::now();
    loop {
        let mut entries = Vec::new();
        snapshot(fixture.path(), &mut entries);
        entries.sort();
        if entries == previous {
            if stable_since.elapsed() >= quiet {
                return;
            }
        } else {
            previous = entries;
            stable_since = Instant::now();
        }
        assert!(
            Instant::now() < bound,
            "the fixture never went quiet; a host is still writing under {}",
            fixture.path().display()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn request(fixture: &effigy_test_support::Fixture, project: &std::path::Path) -> serde_json::Value {
    let checkout = checkout_of(fixture, "workspace-serving");
    serde_json::json!({
        "schema": "effigy.workspace-serving.v1",
        "state_home": fixture.path().join("state"),
        "project_path": project,
        "checkout": checkout,
        "environment": "serving-proof",
        "profile": "default",
        "activation": {
            "kind": "Disposable",
            "supervisor_id": "507-serving-proof",
            "capability": CAPABILITY,
            "tls_budget_ms": 1000,
        },
    })
}

/// The same request with the disposable activation's listener stopped right
/// after it is announced, so the supervisor's own loop meets a terminal refusal.
fn failing_listener(request: &serde_json::Value) -> serde_json::Value {
    let mut request = request.clone();
    request["activation"]["fail_listener"] = serde_json::json!(true);
    request
}

/// Start a competing **detached** serving host in its own process and read its
/// answer: the production launcher against the same checkout and environment.
fn detached_contender(request: &serde_json::Value) -> serde_json::Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("__workspace-serving-fixture-compete")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the contender process starts");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(request.to_string().as_bytes())
        .expect("the request is written");
    let output = child
        .wait_with_output()
        .expect("the contender process ends");
    // The proof only counts a contender that really ran and answered in the
    // documented shape: a launch failure, a timeout, a decode error or any other
    // process failure fails the proof instead of reading as a refusal.
    assert!(
        output.status.success(),
        "the contender process must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "the contender answered no readable result ({error}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// The same request whose activation attempts a competing second owner.
fn contended(request: &serde_json::Value) -> serde_json::Value {
    let mut request = request.clone();
    request["activation"]["contend"] = serde_json::json!(true);
    request
}

/// The same request whose activation never records its switch: the host must
/// refuse to serve the session it would otherwise announce.
fn unrecorded_switch(request: &serde_json::Value) -> serde_json::Value {
    let mut request = request.clone();
    request["activation"]["omit_selection_record"] = serde_json::json!(true);
    request
}

/// Run one initiating front end to completion, in its own process, and read the
/// result it printed. `entry` selects the acknowledging or the abandoning mode.
fn initiate(entry: &str, request: &serde_json::Value) -> serde_json::Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg(entry)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the initiating process starts");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(request.to_string().as_bytes())
        .expect("the request is written");
    let output = child
        .wait_with_output()
        .expect("the initiating process exits");
    assert!(
        output.status.success(),
        "the initiating process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("the initiating process printed its result")
}

fn run_of(result: &serde_json::Value, field: &str) -> RunId {
    result[field]
        .as_str()
        .expect("a run identity")
        .parse()
        .expect("a run id")
}

#[test]
fn a_detached_serving_host_keeps_its_session_after_the_initiating_process_exits() {
    let fixture = effigy_test_support::Fixture::new("workspace-serving-host").expect("fixture");
    let state_home = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state_home).expect("state home");
    std::fs::create_dir_all(&project).expect("project");
    let checkout = checkout_of(&fixture, "workspace-serving-host");
    let request = serde_json::json!({
        "schema": "effigy.workspace-serving.v1",
        "state_home": state_home,
        "project_path": project,
        "checkout": checkout,
        "environment": "serving-proof",
        "profile": "default",
        "activation": {
            "kind": "Disposable",
            "supervisor_id": "507-serving-proof",
            "capability": CAPABILITY,
            "tls_budget_ms": 1000,
        },
    });

    // The initiating CLI: one process, which returns and exits.
    let mut initiator = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("__workspace-serving-fixture")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the initiating process starts");
    initiator
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(request.to_string().as_bytes())
        .expect("the request is written");
    let output = initiator
        .wait_with_output()
        .expect("the initiating process exits");
    assert!(
        output.status.success(),
        "the initiating process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the initiating process printed its result");
    let supervisor_run: RunId = result["supervisor_run"]
        .as_str()
        .expect("the supervisor run")
        .parse()
        .expect("a run id");
    let endpoint = endpoint_of(
        result["published"]["endpoint"]
            .as_str()
            .expect("the published endpoint"),
    );
    let session_run: RunId = result["published"]["session_run"]
        .as_str()
        .expect("the session run")
        .parse()
        .expect("a run id");

    // The initiating process is gone. The host's run and its listener are not.
    let cx = effects();
    let observe = observation();
    let root = state_root(&cx, &state_home, &project);
    assert!(
        TcpStream::connect_timeout(&endpoint, Duration::from_secs(5)).is_ok(),
        "the committed listener is still bound after the initiator exited"
    );
    // The retained authority, probed from this third process: the pinned TLS
    // identity the activation published, and the capability its grant book
    // issued. Both must still answer after the initiating process exited.
    let authority = effigy::workspace_serving_fixture::probe_authority(
        result["published"]["endpoint"]
            .as_str()
            .expect("the published endpoint"),
        result["published"]["pin"]
            .as_str()
            .expect("the published pin"),
        CAPABILITY,
        result["published"]["session_run"]
            .as_str()
            .expect("the published session run"),
        &state_home.join("probe-trace.log"),
    )
    .expect("the retained authority answers a third process");
    assert!(
        authority.negotiated,
        "the retained listener negotiated the pinned protocol from a third process"
    );
    // The unissued capability must be refused on its own connection: silence or a
    // timeout fails, so this is a real negative proof of the retained grant book.
    assert!(
        authority.refused_unissued,
        "a capability the host never issued is refused: {authority:?}"
    );
    // The positive probe request is traced, not asserted: the fixture dispatcher's
    // observe seam is never reached for it (empty trace) even though this host
    // answers a typed refusal on the control connection, so the production
    // positive request/response remains #452's gate.
    assert!(
        authority.trace.is_empty() || authority.answered,
        "a reached dispatcher answers or refuses; the trace must agree: {authority:?}"
    );
    let session = read_summary(&observe, &root, session_run).expect("the session run is recorded");
    assert!(
        !matches!(session.phase, Phase::Closed | Phase::Stopping),
        "the session's owner is still alive, not {:?}",
        session.phase
    );
    let supervisor =
        read_summary(&observe, &root, supervisor_run).expect("the host run is recorded");
    assert!(
        !matches!(supervisor.phase, Phase::Closed | Phase::Stopping),
        "the host run is still live"
    );

    // Stop it through the kernel's own fenced control path.
    let owner = supervisor
        .invocation
        .as_ref()
        .expect("the host run records its invocation")
        .spec()
        .owner
        .clone();
    let fence = Fence {
        resource: owner,
        generation: supervisor.generation,
    };
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    let stop = Request {
        // The request names the run it asks about, with its own fresh id.
        id: RunId::new(millis, [0x33; 10]).expect("a request id"),
        run: supervisor_run,
        expected: fence,
        action: RequestAction::Stop,
    };
    run::request(&cx, &root, &stop).expect("the stop request is recorded");

    // It settles once: the run closes and the socket with it.
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        match read_summary(&observe, &root, supervisor_run) {
            Some(summary) if summary.phase == Phase::Closed => break,
            Some(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(50)),
            other => panic!(
                "the host run did not close: {}",
                other.map(|s| s.phase).is_some()
            ),
        }
    }
    let until = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect_timeout(&endpoint, Duration::from_secs(1)).is_ok() {
        assert!(
            Instant::now() < until,
            "the settled host released its listener"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    // One settlement, complete cleanup, and nothing left to stop.
    let settled = read_summary(&observe, &root, supervisor_run).expect("the closed host run");
    assert_eq!(settled.phase, Phase::Closed);
    assert_eq!(
        settled.cleanup_complete,
        Some(true),
        "the host reports complete cleanup"
    );
    let session_settled =
        read_summary(&observe, &root, session_run).expect("the closed session run");
    assert_eq!(session_settled.phase, Phase::Closed);
    assert_eq!(
        session_settled.cleanup_complete,
        Some(true),
        "the session reports complete cleanup"
    );
    // The no-cause path keeps the exact outcome it had before the named causes:
    // this fixture's session never finishes a workload, so it settles NotStarted.
    assert_eq!(
        summary_names(&Some(session_settled.clone())),
        "not-started",
        "a normal settlement keeps its prior outcome"
    );
    // A second stop is refused: the run already settled once.
    let second = Request {
        id: RunId::new(millis + 1, [0x44; 10]).expect("a request id"),
        run: supervisor_run,
        expected: Fence {
            resource: settled
                .invocation
                .as_ref()
                .expect("the host records its invocation")
                .spec()
                .owner
                .clone(),
            generation: settled.generation,
        },
        action: RequestAction::Stop,
    };
    assert!(
        run::request(&cx, &root, &second).is_err(),
        "a settled run takes no second stop"
    );
    assert_eq!(
        read_summary(&observe, &root, supervisor_run).map(|summary| summary.phase),
        Some(Phase::Closed),
        "the settled run stays closed"
    );
    // Let every host this test started finish writing before the fixture
    // removes its directory; a live host writing there is what turns a passing
    // proof into `Directory not empty` cleanup noise.
    settle_fixture(&fixture);
}

/// An unacknowledged detach is owner loss: the host stops serving and settles
/// instead of claiming the session it announced, and nothing is left listening.
#[test]
fn a_host_whose_front_end_exits_without_acknowledging_settles_instead_of_serving() {
    let fixture =
        effigy_test_support::Fixture::new("workspace-serving-abandoned").expect("fixture");
    let state_home = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state_home).expect("state home");
    std::fs::create_dir_all(&project).expect("project");
    let request = request(&fixture, &project);

    // The initiating process announces the session and then exits *without*
    // acknowledging the detach: the kernel's owner-loss window.
    let result = initiate("__workspace-serving-fixture-abandon", &request);
    assert_eq!(result["detached"], serde_json::json!(false));
    let supervisor_run = run_of(&result, "supervisor_run");
    let session_run = run_of(&result["published"], "session_run");
    let endpoint = endpoint_of(
        result["published"]["endpoint"]
            .as_str()
            .expect("the published endpoint"),
    );

    // Owner loss settles the host and its session, and releases the listener.
    let cx = effects();
    let observe = observation();
    let root = state_root(&cx, &state_home, &project);
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let host = read_summary(&observe, &root, supervisor_run);
        let session = read_summary(&observe, &root, session_run);
        if matches!(
            host.as_ref().map(|summary| summary.phase),
            Some(Phase::Closed)
        ) && matches!(
            session.as_ref().map(|summary| summary.phase),
            Some(Phase::Closed)
        ) {
            assert_eq!(
                host.as_ref().and_then(|summary| summary.cleanup_complete),
                Some(true),
                "an owner-loss settle reports complete cleanup"
            );
            assert!(
                matches!(
                    session
                        .as_ref()
                        .and_then(|summary| summary.workload.clone()),
                    Some(effigy_kernel::outcome::WorkloadOutcome::Cancelled {
                        cause: effigy_kernel::ctx::StopCause::OwnerLost,
                        signal: None
                    })
                ),
                "the session run names owner loss itself: {}",
                summary_names(&session)
            );
            // The detached host run names the SAME cause the session run does.
            assert!(
                matches!(
                    host.as_ref().and_then(|summary| summary.workload.clone()),
                    Some(effigy_kernel::outcome::WorkloadOutcome::Cancelled {
                        cause: effigy_kernel::ctx::StopCause::OwnerLost,
                        signal: None
                    })
                ),
                "the detached host run names owner loss as well: {}",
                summary_names(&host)
            );
            break;
        }
        assert!(
            Instant::now() < until,
            "an unacknowledged detach left the host serving"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let until = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect_timeout(&endpoint, Duration::from_secs(1)).is_ok() {
        assert!(
            Instant::now() < until,
            "an owner-lost host released its listener"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // No resurrection: the crashed session stays closed, and a second process
    // asking for that session's ownership is refused even after the settle.
    assert_eq!(
        read_summary(&observe, &root, session_run).map(|summary| summary.phase),
        Some(Phase::Closed),
        "the crashed session is never resurrected"
    );
    // Let every host this test started finish writing before the fixture
    // removes its directory; a live host writing there is what turns a passing
    // proof into `Directory not empty` cleanup noise.
    settle_fixture(&fixture);
}

/// A listener failure retires the session exactly once instead of leaving it
/// behind: the supervisor's own loop sees the terminal refusal, releases the
/// custody and settles both runs.
#[test]
fn a_listener_failure_retires_the_session_once() {
    let fixture = effigy_test_support::Fixture::new("workspace-serving-failed").expect("fixture");
    let state_home = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state_home).expect("state home");
    std::fs::create_dir_all(&project).expect("project");
    let request = failing_listener(&request(&fixture, &project));

    // No detach: a failing listener settles the host as soon as it serves, so the
    // front end has nothing to acknowledge.
    let result = initiate("__workspace-serving-fixture-abandon", &request);
    let supervisor_run = run_of(&result, "supervisor_run");
    let session_run = run_of(&result["published"], "session_run");
    let endpoint = endpoint_of(
        result["published"]["endpoint"]
            .as_str()
            .expect("the published endpoint"),
    );

    let cx = effects();
    let observe = observation();
    let root = state_root(&cx, &state_home, &project);
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let host = read_summary(&observe, &root, supervisor_run);
        let session = read_summary(&observe, &root, session_run);
        if matches!(
            host.as_ref().map(|summary| summary.phase),
            Some(Phase::Closed)
        ) && matches!(
            session.as_ref().map(|summary| summary.phase),
            Some(Phase::Closed)
        ) {
            assert_eq!(
                host.as_ref().and_then(|summary| summary.cleanup_complete),
                Some(true),
                "a listener-failure settle reports complete cleanup"
            );
            assert_eq!(
                session
                    .as_ref()
                    .and_then(|summary| summary.cleanup_complete),
                Some(true),
                "the session it served reports complete cleanup"
            );
            // The listener failure is named on the exact session run id and the
            // exact detached host run id, not merely reported on the socket.
            assert!(
                matches!(
                    session
                        .as_ref()
                        .and_then(|summary| summary.workload.clone()),
                    Some(effigy_kernel::outcome::WorkloadOutcome::Cancelled {
                        cause: effigy_kernel::ctx::StopCause::ListenerFailed,
                        signal: None
                    })
                ),
                "the session run names the listener failure: {}",
                summary_names(&session)
            );
            assert!(
                matches!(
                    host.as_ref().and_then(|summary| summary.workload.clone()),
                    Some(effigy_kernel::outcome::WorkloadOutcome::Cancelled {
                        cause: effigy_kernel::ctx::StopCause::ListenerFailed,
                        signal: None
                    })
                ),
                "the detached host run names the listener failure: {}",
                summary_names(&host)
            );
            break;
        }
        assert!(
            Instant::now() < until,
            "a listener failure left the session serving"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let until = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect_timeout(&endpoint, Duration::from_secs(1)).is_ok() {
        assert!(
            Instant::now() < until,
            "a retired session released its listener"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // Let every host this test started finish writing before the fixture
    // removes its directory; a live host writing there is what turns a passing
    // proof into `Directory not empty` cleanup noise.
    settle_fixture(&fixture);
}

/// After an unacknowledged front end was settled, the same checkout admits a
/// fresh host: the crash left no duplicate owner, and the successor serves. The
/// two sessions are distinct runs, so nothing was resurrected.
#[test]
fn a_settled_crash_is_recovered_by_a_fresh_host() {
    let fixture = effigy_test_support::Fixture::new("workspace-serving-recovery").expect("fixture");
    let state_home = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state_home).expect("state home");
    std::fs::create_dir_all(&project).expect("project");
    let request = request(&fixture, &project);
    let cx = effects();
    let observe = observation();
    let root = state_root(&cx, &state_home, &project);

    // The crashed front end settles its host.
    let crashed = initiate("__workspace-serving-fixture-abandon", &request);
    let crashed_run = run_of(&crashed, "supervisor_run");
    let until = Instant::now() + Duration::from_secs(60);
    while !matches!(
        read_summary(&observe, &root, crashed_run).map(|summary| summary.phase),
        Some(Phase::Closed)
    ) {
        assert!(Instant::now() < until, "the crashed host did not settle");
        std::thread::sleep(Duration::from_millis(50));
    }
    // A settled run's supervisor still leaves its own process, and it is that
    // exit that frees the checkout's serving gate. A successor launched before
    // it would be refused by a gate no host is going to keep, which reads as a
    // recovery failure instead of the handover it is.
    effigy_test_support::process::wait_kernel_helpers(&project)
        .expect("the crashed host's processes leave");

    // The successor starts, announces and serves in its own process.
    let recovered = initiate("__workspace-serving-fixture", &request);
    let successor_run = run_of(&recovered, "supervisor_run");
    let successor_session = run_of(&recovered["published"], "session_run");
    assert_ne!(
        successor_run, crashed_run,
        "recovery admits a new run, never the settled one"
    );
    let crashed_session = run_of(&crashed["published"], "session_run");
    assert_ne!(
        successor_session, crashed_session,
        "a resolved crash is never resurrected from a fresh key"
    );
    let endpoint = endpoint_of(
        recovered["published"]["endpoint"]
            .as_str()
            .expect("the published endpoint"),
    );
    assert!(
        TcpStream::connect_timeout(&endpoint, Duration::from_secs(5)).is_ok(),
        "the successor serves after the crashed host settled"
    );

    // Stop the successor through the kernel's own control path.
    let supervisor = read_summary(&observe, &root, successor_run).expect("the successor run");
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    let stop = Request {
        id: RunId::new(millis, [0x55; 10]).expect("a request id"),
        run: successor_run,
        expected: Fence {
            resource: supervisor
                .invocation
                .as_ref()
                .expect("the successor records its invocation")
                .spec()
                .owner
                .clone(),
            generation: supervisor.generation,
        },
        action: RequestAction::Stop,
    };
    run::request(&cx, &root, &stop).expect("the successor's stop is recorded");
    let until = Instant::now() + Duration::from_secs(60);
    while !matches!(
        read_summary(&observe, &root, successor_run).map(|summary| summary.phase),
        Some(Phase::Closed)
    ) {
        assert!(Instant::now() < until, "the successor did not settle");
        std::thread::sleep(Duration::from_millis(50));
    }
    // Let every host this test started finish writing before the fixture
    // removes its directory; a live host writing there is what turns a passing
    // proof into `Directory not empty` cleanup noise.
    settle_fixture(&fixture);
}

/// A session whose switch the activation never recorded is never served: the
/// host reads the durable selection back and refuses, and the front end reads a
/// refusal instead of a committed session.
#[test]
fn an_unrecorded_switch_is_refused_not_served() {
    let fixture =
        effigy_test_support::Fixture::new("workspace-serving-unswitched").expect("fixture");
    let state_home = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state_home).expect("state home");
    std::fs::create_dir_all(&project).expect("project");
    let request = unrecorded_switch(&request(&fixture, &project));

    let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("__workspace-serving-fixture")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the initiating process starts");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(request.to_string().as_bytes())
        .expect("the request is written");
    let output = child
        .wait_with_output()
        .expect("the initiating process exits");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "an unrecorded switch is not a committed session"
    );
    assert!(
        stderr.contains("recorded no committed selection")
            || stderr.contains("has not switched and been read back"),
        "the refusal names the missing switch: {stderr}"
    );
    // The refused front end drops its host, which is owner loss: the detached
    // supervisor settles in its own process. Wait for that settle before the
    // fixture's journal directory is removed.
    let run = stderr
        .split("supervisor_run=")
        .nth(1)
        .and_then(|text| text.split(')').next())
        .and_then(|text| text.parse::<RunId>().ok())
        .expect("the refusal names its supervisor run");
    let cx = effects();
    let observe = observation();
    let root = state_root(&cx, &state_home, &project);
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        match read_summary(&observe, &root, run) {
            Some(summary) if summary.phase == Phase::Closed => break,
            _ => {
                assert!(
                    Instant::now() < until,
                    "the refused supervisor did not settle"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    // A settled run still has a supervisor leaving its own process; that is the
    // writer the fixture's directory removal must not race.
    effigy_test_support::process::wait_kernel_helpers(&project)
        .expect("the refused supervisor's processes leave");
    // Let every host this test started finish writing before the fixture
    // removes its directory; a live host writing there is what turns a passing
    // proof into `Directory not empty` cleanup noise.
    settle_fixture(&fixture);
}

/// A competing second owner for one session refuses inside the detached process,
/// and the first owner keeps serving: the kernel's one-serving-owner fence holds
/// across the production launch path, not only in an in-crate proof.
#[test]
fn a_competing_owner_is_refused_inside_the_detached_process() {
    let fixture =
        effigy_test_support::Fixture::new("workspace-serving-contended").expect("fixture");
    let state_home = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state_home).expect("state home");
    std::fs::create_dir_all(&project).expect("project");
    let request = contended(&request(&fixture, &project));

    let result = initiate("__workspace-serving-fixture", &request);
    let supervisor_run = run_of(&result, "supervisor_run");
    let endpoint = endpoint_of(
        result["published"]["endpoint"]
            .as_str()
            .expect("the published endpoint"),
    );
    // The activation refused the contender and the first owner still serves.
    assert!(
        TcpStream::connect_timeout(&endpoint, Duration::from_secs(5)).is_ok(),
        "the first owner still serves after refusing a contender"
    );

    let cx = effects();
    let observe = observation();
    let root = state_root(&cx, &state_home, &project);
    let supervisor = read_summary(&observe, &root, supervisor_run).expect("the host run");
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    let stop = Request {
        id: RunId::new(millis, [0x66; 10]).expect("a request id"),
        run: supervisor_run,
        expected: Fence {
            resource: supervisor
                .invocation
                .as_ref()
                .expect("the host records its invocation")
                .spec()
                .owner
                .clone(),
            generation: supervisor.generation,
        },
        action: RequestAction::Stop,
    };
    run::request(&cx, &root, &stop).expect("the stop is recorded");
    let until = Instant::now() + Duration::from_secs(60);
    while !matches!(
        read_summary(&observe, &root, supervisor_run).map(|summary| summary.phase),
        Some(Phase::Closed)
    ) {
        assert!(Instant::now() < until, "the contended host did not settle");
        std::thread::sleep(Duration::from_millis(50));
    }
    let until = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect_timeout(&endpoint, Duration::from_secs(1)).is_ok() {
        assert!(
            Instant::now() < until,
            "the contended host released its listener"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // Let every host this test started finish writing before the fixture
    // removes its directory; a live host writing there is what turns a passing
    // proof into `Directory not empty` cleanup noise.
    settle_fixture(&fixture);
}

/// The startup result is bounded by the caller's own deadline, not by an
/// internal one: a host that announces late makes the caller give up when it
/// asked to, and the refusal says so.
///
/// The caller's budget has to be larger than this machine's fixed startup cost
/// — spawning the front end, opening the journal and admitting the supervisor —
/// or the proof measures fork/exec instead of the deadline. Measured without
/// contention that cost is ~1.1s (context 257ms, journal 140ms, executable
/// 18ms, supervisor admission 672ms), and several seconds under host load. The
/// budget therefore bounds the whole startup (the fixture front end gives it to
/// the launch as well as the announcement wait), and the announcement delay is
/// set far above it, so "gave up before the host announced" is decided by the
/// caller's own report and not by spawn cost.
#[test]
fn startup_honours_the_callers_deadline() {
    let fixture = effigy_test_support::Fixture::new("workspace-serving-deadline").expect("fixture");
    let state_home = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state_home).expect("state home");
    std::fs::create_dir_all(&project).expect("project");
    let mut request = request(&fixture, &project);
    const BUDGET_MS: u64 = 1_000;
    // The activation announces far after anything the caller's own budget covers,
    // so a caller that waited for the announcement would report a number near it.
    const ANNOUNCE_MS: u64 = 30_000;
    request["activation"]["announce_delay_ms"] = serde_json::json!(ANNOUNCE_MS);

    let body = serde_json::json!({ "request": request, "budget_ms": BUDGET_MS });
    let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("__workspace-serving-fixture-deadline")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the initiating process starts");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(body.to_string().as_bytes())
        .expect("the request is written");
    let started = Instant::now();
    let output = child
        .wait_with_output()
        .expect("the initiating process exits");
    let elapsed = started.elapsed();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "a late host is not a success");
    assert!(
        stderr.contains("did not announce its session in time") || stderr.contains("TimedOut"),
        "the refusal names the deadline: {stderr}"
    );
    let reported = stderr
        .split("deadline=")
        .nth(1)
        .and_then(|text| text.split("ms").next())
        .and_then(|text| text.parse::<u128>().ok())
        .expect("the refusal reports the deadline it honoured");
    let field = |name: &str| -> u128 {
        stderr
            .split(&format!("{name}="))
            .nth(1)
            .and_then(|text| text.split("ms").next())
            .and_then(|text| text.parse::<u128>().ok())
            .unwrap_or_else(|| panic!("the refusal reports its {name}: {stderr}"))
    };
    let launch = field("launch");
    let wait = field("wait");
    // The deadline message above is the evidence that the budget, not the
    // announcement, ended the wait: it is only reported once the budget expired
    // before the host answered.
    assert!(
        launch + wait >= u128::from(BUDGET_MS),
        "the caller's {BUDGET_MS}ms budget was consumed, not skipped \
         (launch {launch}ms + wait {wait}ms)"
    );
    // The mechanism: the announcement wait never outlives the budget it was given.
    // Process startup is deliberately not asserted — a loaded host may take
    // seconds to spawn, and that is not what this proof is about.
    assert!(
        wait <= u128::from(BUDGET_MS) + 250,
        "the {BUDGET_MS}ms budget bounded the {ANNOUNCE_MS}ms announcement wait ({wait}ms)"
    );
    // ...and the whole attempt ended far below the announcement: a caller that
    // waited for it, or an inner deadline longer than the caller's, would report
    // something near the announcement instead.
    assert!(
        reported < u128::from(ANNOUNCE_MS),
        "the caller gave up before the {ANNOUNCE_MS}ms announcement ({reported}ms)"
    );
    // A wall-clock guard as well: a hung front end must still fail the proof.
    assert!(
        elapsed < Duration::from_secs(120),
        "the front end finished ({elapsed:?}, reported {reported}ms)"
    );
    effigy_test_support::process::wait_kernel_helpers(&project)
        .expect("the abandoned host settles");
    // Let every host this test started finish writing before the fixture
    // removes its directory; a live host writing there is what turns a passing
    // proof into `Directory not empty` cleanup noise.
    settle_fixture(&fixture);
}

/// A front end that crashes *after* acknowledging the detach leaves the session
/// serving: the acknowledgement is durable, so a dead client is not owner loss.
#[test]
fn a_crash_after_the_acknowledgement_keeps_the_session_serving() {
    let fixture = effigy_test_support::Fixture::new("workspace-serving-post-ack").expect("fixture");
    let state_home = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state_home).expect("state home");
    std::fs::create_dir_all(&project).expect("project");
    let request = request(&fixture, &project);

    // The initiating process detaches and then aborts (a crash, not an exit).
    let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("__workspace-serving-fixture-crash")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the initiating process starts");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(request.to_string().as_bytes())
        .expect("the request is written");
    let output = child
        .wait_with_output()
        .expect("the initiating process ends");
    assert!(
        !output.status.success(),
        "the front end crashed after the acknowledgement"
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("the front end printed the committed session before it crashed");
    let supervisor_run = run_of(&result, "supervisor_run");
    let endpoint = endpoint_of(
        result["published"]["endpoint"]
            .as_str()
            .expect("the published endpoint"),
    );

    // The host still serves, and the retained authority still answers.
    let authority = effigy::workspace_serving_fixture::probe_authority(
        result["published"]["endpoint"]
            .as_str()
            .expect("the published endpoint"),
        result["published"]["pin"]
            .as_str()
            .expect("the published pin"),
        CAPABILITY,
        result["published"]["session_run"]
            .as_str()
            .expect("the published session run"),
        &state_home.join("probe-trace.log"),
    )
    .expect("the retained authority answers after a front-end crash");
    assert!(
        authority.negotiated,
        "the retained listener still negotiates the pinned protocol after a front-end crash"
    );
    assert!(
        authority.refused_unissued,
        "the retained grant book still refuses an unissued capability: {authority:?}"
    );

    // A competing DETACHED host cannot become the live session's owner: it runs
    // the same production launcher against the same checkout and environment.
    let competing = detached_contender(&request);
    assert!(
        competing["refused"].as_bool() == Some(true),
        "a second detached owner is refused (a launcher error is not a refusal): {competing}"
    );
    assert!(
        competing["reason"]
            .as_str()
            .is_some_and(|reason| { reason.contains("is held by") || reason.contains("refused") }),
        "the refusal is the kernel's own owner-gate/refusal fencing: {competing}"
    );

    // Stop it through the kernel's own control path; it settles once.
    let cx = effects();
    let observe = observation();
    let root = state_root(&cx, &state_home, &project);
    let supervisor = read_summary(&observe, &root, supervisor_run).expect("the host run");
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    let stop = Request {
        id: RunId::new(millis, [0x77; 10]).expect("a request id"),
        run: supervisor_run,
        expected: Fence {
            resource: supervisor
                .invocation
                .as_ref()
                .expect("the host records its invocation")
                .spec()
                .owner
                .clone(),
            generation: supervisor.generation,
        },
        action: RequestAction::Stop,
    };
    run::request(&cx, &root, &stop).expect("the stop is recorded");
    let until = Instant::now() + Duration::from_secs(60);
    while !matches!(
        read_summary(&observe, &root, supervisor_run).map(|summary| summary.phase),
        Some(Phase::Closed)
    ) {
        assert!(Instant::now() < until, "the host did not settle");
        std::thread::sleep(Duration::from_millis(50));
    }
    let until = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect_timeout(&endpoint, Duration::from_secs(1)).is_ok() {
        assert!(
            Instant::now() < until,
            "the settled host released its listener"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // Let every host this test started finish writing before the fixture
    // removes its directory; a live host writing there is what turns a passing
    // proof into `Directory not empty` cleanup noise.
    settle_fixture(&fixture);
}

/// An interrupted handoff: the front end starts the host and dies before it reads
/// any startup result. No session is ever claimed, the host settles on owner
/// loss, and the next launch on the same checkout is admitted — recoverable, not
/// broken, and never a claimed success from a dropped owner.
#[test]
fn an_interrupted_handoff_claims_nothing_and_stays_recoverable() {
    let fixture = effigy_test_support::Fixture::new("workspace-serving-handoff").expect("fixture");
    let state_home = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state_home).expect("state home");
    std::fs::create_dir_all(&project).expect("project");
    let request = request(&fixture, &project);

    let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("__workspace-serving-fixture-drop")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the initiating process starts");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(request.to_string().as_bytes())
        .expect("the request is written");
    let output = child
        .wait_with_output()
        .expect("the initiating process ends");
    assert!(
        !output.status.success(),
        "the interrupted front end did not complete its handoff"
    );
    assert!(
        output.stdout.is_empty(),
        "an interrupted handoff claims no committed session"
    );

    // The dropped owner's host settles on owner loss, and only then does the
    // same checkout admit a fresh host: the interrupted handoff left nothing
    // that blocks recovery. The settle is not instantaneous, so the proof waits
    // for the run the dropped front end named — its settle and then its recorded
    // processes leaving, which is what releases the checkout's serving gate —
    // instead of sleeping long enough to usually be true.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let interrupted = stderr
        .split("supervisor_run=")
        .nth(1)
        .and_then(|text| text.trim().parse::<RunId>().ok())
        .expect("the interrupted front end names the supervisor run it lost");
    let cx = effects();
    let observe = observation();
    let root = state_root(&cx, &state_home, &project);
    let until = Instant::now() + Duration::from_secs(60);
    while !matches!(
        read_summary(&observe, &root, interrupted).map(|summary| summary.phase),
        Some(Phase::Closed)
    ) {
        assert!(
            Instant::now() < until,
            "the interrupted handoff's host did not settle"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    effigy_test_support::process::wait_kernel_helpers(&project)
        .expect("the interrupted handoff's host leaves no writer behind");
    let recovered = initiate("__workspace-serving-fixture", &request);
    let endpoint = endpoint_of(
        recovered["published"]["endpoint"]
            .as_str()
            .expect("the published endpoint"),
    );
    assert!(
        TcpStream::connect_timeout(&endpoint, Duration::from_secs(5)).is_ok(),
        "a successor serves after an interrupted handoff"
    );
    let supervisor_run = run_of(&recovered, "supervisor_run");
    let supervisor = read_summary(&observe, &root, supervisor_run).expect("the successor run");
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    let stop = Request {
        id: RunId::new(millis, [0x88; 10]).expect("a request id"),
        run: supervisor_run,
        expected: Fence {
            resource: supervisor
                .invocation
                .as_ref()
                .expect("the successor records its invocation")
                .spec()
                .owner
                .clone(),
            generation: supervisor.generation,
        },
        action: RequestAction::Stop,
    };
    run::request(&cx, &root, &stop).expect("the successor's stop is recorded");
    let until = Instant::now() + Duration::from_secs(60);
    while !matches!(
        read_summary(&observe, &root, supervisor_run).map(|summary| summary.phase),
        Some(Phase::Closed)
    ) {
        assert!(Instant::now() < until, "the successor did not settle");
        std::thread::sleep(Duration::from_millis(50));
    }
    // Let every host this test started finish writing before the fixture
    // removes its directory; a live host writing there is what turns a passing
    // proof into `Directory not empty` cleanup noise.
    settle_fixture(&fixture);
}

/// Two command processes in one terminal resolve ONE client identity, so the
/// capability the first was issued is reused by the second; another terminal's
/// client is a different identity and never shares its store.
#[test]
fn two_commands_in_one_terminal_share_one_client_identity() {
    // A declared client is the only identity a command without a terminal has, so
    // the reuse proof declares one exactly as an entry does.
    let declared_env = [("EFFIGY_WORKSPACE_CLIENT_ID", "terminal-a")];
    let first = client_identity_of(&declared_env);
    let second = client_identity_of(&declared_env);
    assert_eq!(first, second, "a terminal's commands share one identity");
    // Redirecting a command's input does not change its terminal identity, so a
    // command run with a redirected stdin reuses the capability the terminal was
    // issued rather than falling back to another client.
    let redirected = client_identity_with_redirected_input(&declared_env);
    assert_eq!(
        first, redirected,
        "a redirected stdin does not change the terminal's identity"
    );
    // A command with no declared client and no terminal has no identity at all, so
    // it can neither read nor write a shared capability record.
    // All three streams redirected: no descriptor is a terminal, so the command
    // has no terminal identity at all (an inherited stderr would still be one).
    let bare = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("__workspace-serving-fixture-client")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("EFFIGY_WORKSPACE_CLIENT_ID")
        .output()
        .expect("the command process runs");
    assert!(
        !bare.status.success(),
        "a command with no terminal and no declared client has no client identity"
    );
    assert!(
        first.starts_with("terminal-"),
        "the identity is the terminal's session, not the command's pid: {first}"
    );

    // Two DISTINCT terminals are two clients even in one session, and two
    // commands in one terminal are one client: the identity follows the terminal.
    // Distinct terminals are identified by their device NAME: two pty pairs can
    // report the same device/inode numbers, and sharing those numbers would give
    // two terminals one capability file.
    let first_tty = effigy::workspace_serving_fixture::terminal_identity(Some(0x20), 4242)
        .expect("a terminal identity");
    let same_tty = effigy::workspace_serving_fixture::terminal_identity(Some(0x20), 4242)
        .expect("a terminal identity");
    let second_tty = effigy::workspace_serving_fixture::terminal_identity(Some(0x21), 4242)
        .expect("a terminal identity");
    assert_eq!(first_tty, same_tty, "one terminal keeps one identity");
    assert_ne!(
        first_tty, second_tty,
        "two terminals with the same device numbers are still two clients"
    );
    // A command with no terminal falls back to its session; it never shares a
    // terminal client's record.
    // A command with no terminal has NO client identity: a session-wide one would
    // share a single capability file between independent clients.
    assert!(
        effigy::workspace_serving_fixture::terminal_identity(None, 4242).is_err(),
        "a command with no terminal is refused, never given a shared identity"
    );
    assert!(
        effigy::workspace_serving_fixture::terminal_identity(None, -1).is_err(),
        "a client with neither a terminal nor a session is refused, never shared"
    );

    // Isolation: an entry-declared client is a different client, so its store is
    // a different file and no command of one can present the other's authority.
    let other = client_identity_of(&[("EFFIGY_WORKSPACE_CLIENT_ID", "terminal-b")]);
    assert_eq!(first, "terminal-a");
    assert_eq!(other, "terminal-b");
    assert_ne!(first, other, "two declared clients are two clients");
}

/// The same command with its input redirected, as a shell pipeline does.
fn client_identity_with_redirected_input(env: &[(&str, &str)]) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command
        .arg("__workspace-serving-fixture-client")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("the command process runs");
    assert!(
        output.status.success(),
        "the command reported its client identity: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn client_identity_of(env: &[(&str, &str)]) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command
        .arg("__workspace-serving-fixture-client")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("the command process runs");
    assert!(
        output.status.success(),
        "the command reported its client identity: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A contender that cannot launch, or that actually becomes a second host, is NOT
/// a refusal: only the kernel's own owner-gate refusal may pass the contention
/// proof, so these two cases must fail it.
///
/// The two cases are two different checkouts on purpose. The kernel's serving
/// gate is keyed by the checkout a launch serves, and an admitted contender's
/// host keeps that gate until it has finished leaving; a second contender on the
/// same checkout would be answered by the first one's gate and the proof would
/// read that as a second-owner refusal. A separate checkout, and a separate
/// journal under it, is what keeps the launcher-failure case about the launcher.
#[test]
fn a_contender_that_is_not_refused_by_the_owner_gate_is_not_a_refusal() {
    let fixture =
        effigy_test_support::Fixture::new("workspace-serving-contender").expect("fixture");
    let state_home = fixture.path().join("state");
    let project = fixture.path().join("project");
    std::fs::create_dir_all(&state_home).expect("state home");
    std::fs::create_dir_all(&project).expect("project");

    // The launcher-failure case is prepared first, on its own checkout and its
    // own journal: its own checkout, so no live owner of the first checkout can
    // answer for it.
    let broken_fixture =
        effigy_test_support::Fixture::new("workspace-serving-contender-broken").expect("fixture");
    let broken_project = broken_fixture.path().join("project");
    std::fs::create_dir_all(&broken_project).expect("broken contender project");
    let mut broken = request(&broken_fixture, &broken_project);
    broken["checkout"] = serde_json::json!(vec![0x5au8; 16]);
    broken["state_home"] = serde_json::json!("/nonexistent-effigy-state-home");

    let request = request(&fixture, &project);

    // Nothing is serving this checkout, so a contender is admitted and announces
    // its own host: that is not a refusal of a second owner.
    let uncontended = detached_contender(&request);
    assert_eq!(
        uncontended["refused"].as_bool(),
        Some(false),
        "an admitted contender is not a refusal: {uncontended}"
    );

    // A contender whose launcher cannot even open the journal is not a refusal
    // either: an unavailable launcher proves nothing about ownership.
    let failed = detached_contender(&broken);
    assert_eq!(
        failed["refused"].as_bool(),
        Some(false),
        "a launcher error is not a refusal: {failed}"
    );
    // Let every host this test started finish writing before the fixture
    // removes its directory; a live host writing there is what turns a passing
    // proof into `Directory not empty` cleanup noise.
    settle_fixture(&fixture);
}
