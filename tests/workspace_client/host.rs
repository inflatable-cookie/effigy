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
fn request(fixture: &effigy_test_support::Fixture, project: &std::path::Path) -> serde_json::Value {
    let checkout: Vec<u8> = (0u8..16).collect();
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
    let checkout: Vec<u8> = (0u8..16).collect();
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
}
