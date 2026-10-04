use crate::transport::{
    is_attach_idle_read_error, parse_settlement, parse_test_frame, ReconnectTimer,
};
use crate::{
    canonical_start_identity, container_started_fact, start_identity_matches, AttachEvent,
    Authority, ClientError, Clock, HostRunClient, HostRunRoot, IdentityProvider, OutputStream,
    TokenKeys,
};
use base64::Engine;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Cursor, ErrorKind, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct FixedClock(DateTime<Utc>);
impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

struct FixedIdentity(&'static str);
impl IdentityProvider for FixedIdentity {
    fn start_identity(&self, _pid: u32) -> Option<String> {
        Some(self.0.into())
    }
}

#[derive(Default)]
struct FakeReconnectTimer(AtomicU64);

impl ReconnectTimer for FakeReconnectTimer {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::Relaxed))
    }

    fn sleep(&self, duration: Duration) {
        self.0.fetch_add(
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }
}

struct Fixture {
    _temp: TempDir,
    root_path: PathBuf,
    socket_path: PathBuf,
    listener: UnixListener,
    authority: Authority,
}

fn make_fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root_path = temp.path().join("host-run");
    std::fs::create_dir(&root_path).unwrap();
    let root_path = std::fs::canonicalize(root_path).unwrap();
    std::fs::set_permissions(&root_path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let run_path = root_path.join("run");
    std::fs::create_dir(&run_path).unwrap();
    std::fs::set_permissions(&run_path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket_path = run_path.join("scheduler.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();
    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let pid = std::process::id();
    let authority = Authority {
        format: "host.run.authority".into(),
        version: 1,
        holder: "queue".into(),
        endpoint: socket_path.clone(),
        epoch: 3,
        pid,
        start_identity: canonical_start_identity(pid).expect("test process identity"),
    };
    write_authority(&root_path, &authority);
    write_token_keys(&root_path, 3, [5u8; 32], None);
    Fixture {
        _temp: temp,
        root_path,
        socket_path,
        listener,
        authority,
    }
}

fn write_token_keys(
    root: &Path,
    current_epoch: u64,
    current: [u8; 32],
    previous: Option<(u64, [u8; 32])>,
) {
    let mut content = json!({
        "format":"host.run.keys",
        "version":1,
        "current":{"epoch":current_epoch,"keyB64":base64::engine::general_purpose::STANDARD.encode(current)}
    });
    if let Some((epoch, key)) = previous {
        content["previous"] = json!({
            "epoch":epoch,
            "keyB64":base64::engine::general_purpose::STANDARD.encode(key)
        });
    }
    let path = root.join("token.key");
    std::fs::write(&path, serde_json::to_vec(&content).unwrap()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

fn write_authority(root: &Path, authority: &Authority) {
    let next = root.join("authority.next");
    std::fs::write(&next, serde_json::to_vec(authority).unwrap()).unwrap();
    std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::rename(next, root.join("authority.json")).unwrap();
}

fn client(fixture: &Fixture) -> HostRunClient {
    let (root, authority) = HostRunRoot::open(&fixture.root_path).expect("trusted fixture root");
    HostRunClient::open(root, authority)
}

fn live_discover_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state/host-run"))
}

fn assert_private_host_run_fixture(fixture: &Fixture) {
    assert_eq!(
        fixture.authority.pid,
        std::process::id(),
        "fixture authority pid is the test process, not a live scheduler"
    );
    assert_eq!(
        fixture.authority.endpoint,
        fixture.root_path.join("run/scheduler.sock")
    );
    assert_eq!(fixture.socket_path, fixture.authority.endpoint);
    let identity = canonical_start_identity(fixture.authority.pid).expect("test process identity");
    assert_eq!(fixture.authority.start_identity, identity);
    assert!(
        !identity.contains(".local/state/host-run"),
        "start identity is pid/kernel scoped, not a host-run path: {identity}"
    );
    if let Some(live) = live_discover_root() {
        assert_ne!(
            fixture.root_path, live,
            "peer-trust fixture must not use HostRunRoot::discover live path"
        );
        assert!(
            !fixture.root_path.starts_with(&live),
            "fixture root {:?} must not nest under {:?}",
            fixture.root_path,
            live
        );
        assert!(
            !fixture.socket_path.starts_with(&live),
            "fixture socket {:?} must not nest under {:?}",
            fixture.socket_path,
            live
        );
    }
}

const PEER_FIXTURE_BOUND: Duration = Duration::from_secs(5);

struct HeldAcceptedPeer {
    stop: Arc<AtomicBool>,
    accepted: Option<mpsc::Receiver<UnixStream>>,
    stream: Option<UnixStream>,
    server: Option<thread::JoinHandle<()>>,
}

impl HeldAcceptedPeer {
    fn spawn(listener: UnixListener) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + PEER_FIXTURE_BOUND;
            let stream = loop {
                if stop_flag.load(Ordering::Relaxed) || Instant::now() >= deadline {
                    return;
                }
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => return,
                }
            };
            let clone = match stream.try_clone() {
                Ok(clone) => clone,
                Err(_) => return,
            };
            if accepted_tx.send(clone).is_err() {
                return;
            }
            while !stop_flag.load(Ordering::Relaxed) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            drop(stream);
            drop(listener);
        });
        Self {
            stop,
            accepted: Some(accepted_rx),
            stream: None,
            server: Some(server),
        }
    }

    fn assert_no_request_dispatched(&mut self, context: &str) {
        let mut stream = self.held_stream(context);
        let mut buf = [0u8; 32];
        match stream.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => panic!("{context}: {n} request byte(s) arrived before trust rejection"),
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => panic!("{context}: held peer read failed: {error}"),
        }
    }

    fn held_stream(&mut self, context: &str) -> &UnixStream {
        if self.stream.is_none() {
            let rx = self
                .accepted
                .take()
                .expect("accepted channel still available");
            let stream = match rx.recv_timeout(PEER_FIXTURE_BOUND) {
                Ok(stream) => stream,
                Err(RecvTimeoutError::Timeout) => {
                    panic!("{context}: fixture listener did not accept a live peer")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("{context}: fixture accept thread ended before a live peer")
                }
            };
            stream.set_nonblocking(true).unwrap();
            self.stream = Some(stream);
        }
        self.stream.as_ref().unwrap()
    }
}

impl Drop for HeldAcceptedPeer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

struct ImmediateDropPeer {
    stop: Arc<AtomicBool>,
    accepted: Option<mpsc::Receiver<()>>,
    server: Option<thread::JoinHandle<()>>,
}

impl ImmediateDropPeer {
    fn spawn(listener: UnixListener) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + PEER_FIXTURE_BOUND;
            while Instant::now() < deadline && !stop_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        drop(stream);
                        let _ = accepted_tx.send(());
                        return;
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => return,
                }
            }
        });
        Self {
            stop,
            accepted: Some(accepted_rx),
            server: Some(server),
        }
    }

    fn assert_accepted_and_dropped(&mut self, context: &str) {
        let rx = self
            .accepted
            .take()
            .expect("accepted channel still available");
        match rx.recv_timeout(PEER_FIXTURE_BOUND) {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout) => {
                panic!("{context}: fixture did not accept and drop a peer")
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("{context}: fixture accept thread ended without accepting a peer")
            }
        }
    }
}

impl Drop for ImmediateDropPeer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

struct BoundedServer(Option<thread::JoinHandle<()>>);

impl Drop for BoundedServer {
    fn drop(&mut self) {
        if let Some(server) = self.0.take() {
            let _ = server.join();
        }
    }
}

fn assert_scheduler_unreachable(error: ClientError, context: &str) {
    assert!(
        matches!(error, ClientError::SchedulerUnreachable),
        "{context}: expected SchedulerUnreachable, got {error:?}"
    );
}

fn assert_closed_peer_fail_closed(error: ClientError, context: &str) {
    match error {
        ClientError::Io(ref io) => {
            eprintln!("{context}: fail-closed as Io({:?}): {io}", io.kind());
        }
        ClientError::SchedulerUnreachable => {
            eprintln!(
                "{context}: fail-closed as SchedulerUnreachable (peer credentials still readable after close on this platform); not a live-held identity proof"
            );
        }
        other => panic!(
            "{context}: closed peer must fail closed with Io or SchedulerUnreachable; got {other:?}"
        ),
    }
}

fn read_request(stream: &mut UnixStream) -> Value {
    let mut line = String::new();
    BufReader::new(&mut *stream).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

fn send_response(stream: &mut UnixStream, request: &Value, body: Value) {
    let response = json!({"v":1,"id":request["id"],"ok":true,"body":body});
    serde_json::to_writer(&mut *stream, &response).unwrap();
    stream.write_all(b"\n").unwrap();
}

fn send_error(stream: &mut UnixStream, request: &Value, code: &str) {
    let response =
        json!({"v":1,"id":request["id"],"ok":false,"error":{"code":code,"message":"fixture"}});
    serde_json::to_writer(&mut *stream, &response).unwrap();
    stream.write_all(b"\n").unwrap();
}

fn ok_settlement(run_id: &str) -> Value {
    json!({
        "format":"host.run.settlement", "version":1,"runId":run_id,"outcome":"cancelled",
        "launched":false,"settledAt":"2026-10-01T14:00:00Z","result":null,"containers":[]
    })
}

fn passed_settlement(run_id: &str) -> Value {
    json!({
        "format":"host.run.settlement", "version":1,"runId":run_id,"outcome":"passed",
        "launched":true,"settledAt":"2026-10-01T14:00:00Z",
        "result":{
            "format":"host.run.result","version":1,"runId":run_id,"epoch":3,"pgid":42,
            "startIdentity":"42@boot:1","exitCode":0,"signal":null,
            "startedAt":"2026-10-01T13:59:00Z","endedAt":"2026-10-01T14:00:00Z",
            "wallMs":60000,"cpuMs":null,"escapedDescendants":[]
        },
        "containers":[]
    })
}

const TEST_ATTACH_READ_TIMEOUT: Duration = Duration::from_millis(40);

#[test]
fn attach_idle_follow_recognizes_socket_read_timeout_kinds() {
    assert!(is_attach_idle_read_error(&std::io::Error::from(
        std::io::ErrorKind::WouldBlock
    )));
    assert!(is_attach_idle_read_error(&std::io::Error::from(
        std::io::ErrorKind::TimedOut
    )));

    let (_writer, reader_stream) = UnixStream::pair().unwrap();
    reader_stream
        .set_read_timeout(Some(TEST_ATTACH_READ_TIMEOUT))
        .unwrap();
    let mut reader = BufReader::new(reader_stream);
    let error = reader.fill_buf().unwrap_err();
    assert!(
        is_attach_idle_read_error(&error),
        "platform socket read timeout was {:?}",
        error.kind()
    );
}

#[test]
fn attach_idle_follow_keeps_queued_run_and_partial_frame_until_settlement() {
    let fixture = make_fixture();
    let mut client = client(&fixture).with_attach_read_timeout(TEST_ATTACH_READ_TIMEOUT);
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert_eq!(request["method"], "attach");
        assert_eq!(request["body"]["runId"], "r-idle-queued");
        send_response(
            &mut stream,
            &request,
            json!({"event":"state","state":"queued","position":1}),
        );
        thread::sleep(TEST_ATTACH_READ_TIMEOUT * 4);

        send_response(
            &mut stream,
            &request,
            json!({"event":"state","state":"running"}),
        );

        let output = json!({
            "v":1,"id":request["id"],"ok":true,
            "body":{"event":"output","stream":"stdout","offset":0,
                "dataB64":base64::engine::general_purpose::STANDARD.encode(b"ready\n")}
        });
        let mut frame = serde_json::to_vec(&output).unwrap();
        frame.push(b'\n');
        let split = frame.len() / 2;
        stream.write_all(&frame[..split]).unwrap();
        thread::sleep(TEST_ATTACH_READ_TIMEOUT * 4);
        stream.write_all(&frame[split..]).unwrap();
        thread::sleep(TEST_ATTACH_READ_TIMEOUT * 4);

        send_response(
            &mut stream,
            &request,
            json!({"event":"settled","settlement":passed_settlement("r-idle-queued")}),
        );
    });

    let mut events = Vec::new();
    let settlement = client
        .attach_stream("r-idle-queued", 0, 0, |event| events.push(event))
        .unwrap();
    assert_eq!(settlement.outcome, crate::SettlementOutcome::Passed);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AttachEvent::Output { .. }))
            .count(),
        1
    );
    assert!(events.iter().any(|event| matches!(
        event,
        AttachEvent::State { state, position: Some(1) } if state == "queued"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        AttachEvent::State { state, .. } if state == "running"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        AttachEvent::Output { stream: OutputStream::Stdout, offset: 0, data }
            if data == b"ready\n"
    )));
    server.join().unwrap();
}

#[test]
fn attach_idle_follow_keeps_silent_running_job_until_output_and_settlement() {
    let fixture = make_fixture();
    let mut client = client(&fixture).with_attach_read_timeout(TEST_ATTACH_READ_TIMEOUT);
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert_eq!(request["method"], "attach");
        send_response(
            &mut stream,
            &request,
            json!({"event":"state","state":"running"}),
        );
        thread::sleep(TEST_ATTACH_READ_TIMEOUT * 4);
        send_response(
            &mut stream,
            &request,
            json!({
                "event":"output","stream":"stderr","offset":0,
                "dataB64":base64::engine::general_purpose::STANDARD.encode(b"still alive")
            }),
        );
        thread::sleep(TEST_ATTACH_READ_TIMEOUT * 4);
        send_response(
            &mut stream,
            &request,
            json!({"event":"settled","settlement":passed_settlement("r-idle-running")}),
        );
    });

    let mut events = Vec::new();
    let settlement = client
        .attach_stream("r-idle-running", 0, 0, |event| events.push(event))
        .unwrap();
    assert_eq!(settlement.outcome, crate::SettlementOutcome::Passed);
    assert!(events.iter().any(|event| matches!(
        event,
        AttachEvent::Output { stream: OutputStream::Stderr, offset: 0, data }
            if data == b"still alive"
    )));
    server.join().unwrap();
}

#[test]
fn attach_idle_follow_still_accepts_cancel_settlement_while_waiting() {
    let fixture = make_fixture();
    let mut follower = client(&fixture).with_attach_read_timeout(TEST_ATTACH_READ_TIMEOUT);
    let mut canceller = client(&fixture);
    let listener = fixture.listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut attach, _) = listener.accept().unwrap();
        let attach_request = read_request(&mut attach);
        assert_eq!(attach_request["method"], "attach");
        send_response(
            &mut attach,
            &attach_request,
            json!({"event":"state","state":"running"}),
        );

        let (mut cancel, _) = listener.accept().unwrap();
        let cancel_request = read_request(&mut cancel);
        assert_eq!(cancel_request["method"], "cancel");
        assert_eq!(cancel_request["body"]["runId"], "r-idle-cancel");
        send_response(&mut cancel, &cancel_request, json!({"stopping":true}));
        send_response(
            &mut attach,
            &attach_request,
            json!({"event":"settled","settlement":ok_settlement("r-idle-cancel")}),
        );
    });
    let follower_thread = thread::spawn(move || {
        follower
            .attach_stream("r-idle-cancel", 0, 0, |_| {})
            .unwrap()
    });

    thread::sleep(TEST_ATTACH_READ_TIMEOUT * 4);
    assert_eq!(
        canceller.cancel("r-idle-cancel", "interrupted").unwrap()["stopping"],
        true
    );
    assert_eq!(
        follower_thread.join().unwrap().outcome,
        crate::SettlementOutcome::Cancelled
    );
    server.join().unwrap();
}

#[test]
fn authority_and_socket_are_owned_private_and_peer_identity_must_match() {
    {
        let fixture = make_fixture();
        assert_private_host_run_fixture(&fixture);
        let (root, opened) = HostRunRoot::open(&fixture.root_path).unwrap();
        assert_eq!(root.path(), fixture.root_path.as_path());
        assert_eq!(opened.endpoint, fixture.socket_path);
        let mut client = HostRunClient::open(root, opened)
            .with_identity_provider(Arc::new(FixedIdentity("wrong-generation")));
        let mut held = HeldAcceptedPeer::spawn(fixture.listener);
        let error = client.status(&json!({})).unwrap_err();
        held.assert_no_request_dispatched("wrong-generation live mismatch");
        assert_scheduler_unreachable(
            error,
            "live generation mismatch on a held private peer must be refused before dispatch",
        );
    }

    {
        let fixture = make_fixture();
        assert_private_host_run_fixture(&fixture);
        let (root, opened) = HostRunRoot::open(&fixture.root_path).unwrap();
        assert_eq!(root.path(), fixture.root_path.as_path());
        let wrong_pid = Authority {
            pid: opened.pid.saturating_add(1),
            ..opened
        };
        let mut client = HostRunClient::open(root, wrong_pid);
        let mut held = HeldAcceptedPeer::spawn(fixture.listener);
        let error = client.status(&json!({})).unwrap_err();
        held.assert_no_request_dispatched("wrong-pid live mismatch");
        assert_scheduler_unreachable(
            error,
            "live pid mismatch on a held private peer must be refused before dispatch",
        );
    }

    let fixture = make_fixture();
    std::fs::set_permissions(&fixture.socket_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(HostRunRoot::open(&fixture.root_path).is_err());

    let fixture = make_fixture();
    let mut unsupported = fixture.authority.clone();
    unsupported.version = 2;
    write_authority(&fixture.root_path, &unsupported);
    assert!(HostRunRoot::open(&fixture.root_path).is_err());

    let fixture = make_fixture();
    std::fs::set_permissions(
        fixture.root_path.join("authority.json"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(HostRunRoot::open(&fixture.root_path).is_err());

    let fixture = make_fixture();
    std::fs::set_permissions(
        fixture.root_path.join("run"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(HostRunRoot::open(&fixture.root_path).is_err());

    let fixture = make_fixture();
    std::fs::set_permissions(&fixture.root_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(HostRunRoot::open(&fixture.root_path).is_err());
}

#[test]
fn valid_peer_identity_is_accepted_on_private_fixture() {
    let fixture = make_fixture();
    assert_private_host_run_fixture(&fixture);
    let mut client = client(&fixture);
    let listener = fixture.listener;
    let _server = BoundedServer(Some(thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + PEER_FIXTURE_BOUND;
        let mut stream = loop {
            if Instant::now() >= deadline {
                panic!("valid-peer fixture listener did not accept");
            }
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("valid-peer accept failed: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(PEER_FIXTURE_BOUND)).unwrap();
        let request = read_request(&mut stream);
        assert_eq!(request["method"], "status");
        send_response(
            &mut stream,
            &request,
            json!({"runId":"peer-trust-positive","state":"running","epoch":3}),
        );
    })));
    let status = client.status(&json!({})).expect("valid private peer");
    assert_eq!(status["runId"], "peer-trust-positive");
    assert_eq!(status["epoch"], 3);
}

#[test]
fn closed_peer_fails_closed_and_is_not_a_trust_match() {
    {
        let fixture = make_fixture();
        assert_private_host_run_fixture(&fixture);
        let root = HostRunRoot::open(&fixture.root_path).unwrap().0;
        let wrong_pid = Authority {
            pid: fixture.authority.pid.saturating_add(1),
            ..fixture.authority.clone()
        };
        let mut client = HostRunClient::open(root, wrong_pid);
        let mut dropped = ImmediateDropPeer::spawn(fixture.listener);
        let result = client.status(&json!({}));
        dropped.assert_accepted_and_dropped(
            "old wrong-pid fixture that drops the accepted stream immediately",
        );
        let error = result.expect_err(
            "premature close of a mismatched peer must fail closed; success would be a false trust proof",
        );
        assert_closed_peer_fail_closed(
            error,
            "old wrong-pid fixture that drops the accepted stream immediately",
        );
    }

    {
        let fixture = make_fixture();
        assert_private_host_run_fixture(&fixture);
        let mut client = client(&fixture);
        let mut dropped = ImmediateDropPeer::spawn(fixture.listener);
        let result = client.status(&json!({}));
        dropped.assert_accepted_and_dropped("valid-identity peer dropped immediately after accept");
        let error = result
            .expect_err("closed valid peer must fail closed; success would be a false trust proof");
        assert_closed_peer_fail_closed(
            error,
            "valid-identity peer dropped immediately after accept",
        );
    }
}

#[test]
fn token_key_requires_private_regular_file() {
    let fixture = make_fixture();
    let path = fixture.root_path.join("token.key");
    let content = json!({"format":"host.run.keys","version":1,"current":{"epoch":3,"keyB64":base64::engine::general_purpose::STANDARD.encode([5u8;32])}});
    std::fs::write(&path, serde_json::to_vec(&content).unwrap()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let root = HostRunRoot::open(&fixture.root_path).unwrap().0;
    assert!(TokenKeys::from_root(&root).is_ok());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(TokenKeys::from_root(&root).is_err());
}

#[test]
fn present_invalid_parent_token_is_exit_77_and_never_submits() {
    let fixture = make_fixture();
    let path = fixture.root_path.join("token.key");
    let content = json!({"format":"host.run.keys","version":1,"current":{"epoch":3,"keyB64":base64::engine::general_purpose::STANDARD.encode([5u8;32])}});
    std::fs::write(&path, serde_json::to_vec(&content).unwrap()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut client = client(&fixture);
    let error = client
        .validate_parent_token(Some("not-a-valid-token"), &fixture.root_path)
        .unwrap_err();
    assert_eq!(error.exit_code(), Some(77));
    fixture.listener.set_nonblocking(true).unwrap();
    assert!(fixture.listener.accept().is_err());
}

#[test]
fn symlinked_authority_and_ancestor_escape_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("host-run");
    let outside = temp.path().join("outside");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let outside = std::fs::canonicalize(outside).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o700)).unwrap();
    let target = outside.join("authority.json");
    let fake = Authority {
        format: "host.run.authority".into(),
        version: 1,
        holder: "q".into(),
        endpoint: root.join("run/scheduler.sock"),
        epoch: 1,
        pid: 1,
        start_identity: "x".into(),
    };
    std::fs::write(&target, serde_json::to_vec(&fake).unwrap()).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::os::unix::fs::symlink(&target, root.join("authority.json")).unwrap();
    assert!(HostRunRoot::open(&root).is_err());

    std::fs::remove_file(root.join("authority.json")).unwrap();
    std::fs::create_dir(root.join("real-run")).unwrap();
    std::fs::set_permissions(
        root.join("real-run"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside, root.join("run")).unwrap();
    let fake = Authority {
        endpoint: root.join("run/scheduler.sock"),
        ..fake
    };
    let authority_path = root.join("authority.json");
    std::fs::write(&authority_path, serde_json::to_vec(&fake).unwrap()).unwrap();
    std::fs::set_permissions(&authority_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(HostRunRoot::open(&root).is_err());
}

struct Fragmented(Vec<u8>);
impl Read for Fragmented {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.0.is_empty() {
            return Ok(0);
        }
        let take = buffer.len().min(2).min(self.0.len());
        buffer[..take].copy_from_slice(&self.0[..take]);
        self.0.drain(..take);
        Ok(take)
    }
}

#[test]
fn ndjson_frames_accept_fragmentation_and_refuse_oversize() {
    let mut reader = BufReader::new(Fragmented(b"{\"a\":1}\n".to_vec()));
    assert_eq!(parse_test_frame(&mut reader).unwrap(), b"{\"a\":1}");
    let mut reader = BufReader::new(Cursor::new(vec![b'x'; crate::MAX_FRAME_BYTES + 1]));
    assert!(matches!(
        parse_test_frame(&mut reader),
        Err(ClientError::Frame(_))
    ));
}

#[test]
fn oversized_attach_chunk_is_refused() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        let encoded =
            base64::engine::general_purpose::STANDARD
                .encode(vec![0u8; crate::MAX_OUTPUT_CHUNK_BYTES + 1]);
        send_response(
            &mut stream,
            &request,
            json!({"event":"output","stream":"stdout","offset":0,"dataB64":encoded}),
        );
    });
    assert!(matches!(
        client.attach("r", 0, 0),
        Err(ClientError::InvalidAttach(_))
    ));
    server.join().unwrap();
}

#[test]
fn ambiguous_submit_resolves_status_then_reuses_the_same_request_id() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let requests = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let captured = requests.clone();
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        for step in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            captured.lock().unwrap().push(request.clone());
            match step {
                0 => drop(stream),
                1 => send_error(&mut stream, &request, "unknown_run"),
                _ => send_response(
                    &mut stream,
                    &request,
                    json!({"runId":"r1","state":"queued","position":1}),
                ),
            }
        }
    });
    let body = json!({"clientRequestId":"00000000-0000-4000-8000-000000000001","caller":"worker","repository":"o/r","cwd":fixture.root_path,"selector":"qa:core","argv":["effigy","qa:core"],"class":"heavy","classSource":"manifest","priority":"interactive","budgetFallback":{"cpu":1,"memoryBytes":8},"capacityDeadlineMs":1000,"runTimeoutMs":2000,"env":{},"cancelOnDisconnect":true});
    assert_eq!(client.submit(&body).unwrap()["runId"], "r1");
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests[0]["method"], "submit");
    assert_eq!(requests[1]["method"], "status");
    assert_eq!(requests[2]["method"], "submit");
    assert_eq!(
        requests[0]["body"]["clientRequestId"],
        requests[2]["body"]["clientRequestId"]
    );
}

#[test]
fn submit_conflict_is_not_retried() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        send_error(&mut stream, &request, "conflict");
    });
    let body = json!({"clientRequestId":"00000000-0000-4000-8000-000000000001","caller":"worker","repository":"o/r","cwd":fixture.root_path,"selector":"qa:core","argv":["effigy","qa:core"],"class":"heavy","classSource":"manifest","priority":"interactive","budgetFallback":{"cpu":1,"memoryBytes":8},"capacityDeadlineMs":1000,"runTimeoutMs":2000,"env":{},"cancelOnDisconnect":true});
    assert!(
        matches!(client.submit(&body), Err(ClientError::Wire(crate::WireError { ref code, .. })) if code == "conflict")
    );
    server.join().unwrap();
}

#[test]
fn scheduler_token_cannot_be_supplied_in_submit_environment_or_error_text() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let body = json!({"clientRequestId":"00000000-0000-4000-8000-000000000001","caller":"worker","repository":"o/r","cwd":fixture.root_path,"selector":"qa:core","argv":["effigy","qa:core"],"class":"heavy","classSource":"manifest","priority":"interactive","budgetFallback":{"cpu":1,"memoryBytes":8},"capacityDeadlineMs":1000,"runTimeoutMs":2000,"env":{"HOST_RUN_TOKEN":"private-secret"},"cancelOnDisconnect":true});
    let error = client.submit(&body).unwrap_err();
    assert!(matches!(error, ClientError::InvalidSubmission(_)));
    assert!(!error.to_string().contains("private-secret"));
    fixture.listener.set_nonblocking(true).unwrap();
    assert!(fixture.listener.accept().is_err());
}

#[test]
fn stale_epoch_refreshes_authority_before_status_recovery() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let root_path = fixture.root_path.clone();
    let next = Authority {
        epoch: 4,
        ..fixture.authority.clone()
    };
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let request = read_request(&mut first);
        write_authority(&root_path, &next);
        send_error(&mut first, &request, "stale_epoch");
        drop(first);
        let (mut second, _) = listener.accept().unwrap();
        let request = read_request(&mut second);
        assert_eq!(request["epoch"], 4);
        send_response(
            &mut second,
            &request,
            json!({"runId":"r2","state":"running","epoch":4}),
        );
    });
    assert_eq!(client.status(&json!({"runId":"r2"})).unwrap()["epoch"], 4);
    server.join().unwrap();
}

#[test]
fn attach_reports_expiration_and_preserves_stream_byte_offsets_without_duplicates() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        let id = request["id"].clone();
        let mut emit = |body: Value| {
            serde_json::to_writer(&mut stream, &json!({"v":1,"id":id,"ok":true,"body":body}))
                .unwrap();
            stream.write_all(b"\n").unwrap();
        };
        emit(json!({"event":"output_expired","stream":"stdout","availableFrom":4}));
        emit(json!({"event":"output","stream":"stdout","offset":4,"dataB64":"dGFpbA=="}));
        emit(json!({"event":"output","stream":"stdout","offset":4,"dataB64":"dGFpbA=="}));
        emit(json!({"event":"output","stream":"stderr","offset":0,"dataB64":"ZXJy"}));
        emit(json!({"event":"settled","settlement":ok_settlement("r3")}));
    });
    let events = client.attach("r3", 2, 0).unwrap();
    assert!(events.contains(&AttachEvent::OutputExpired {
        stream: OutputStream::Stdout,
        available_from: 4
    }));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                AttachEvent::Output {
                    stream: OutputStream::Stdout,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(events.iter().any(|event| matches!(event, AttachEvent::Output { stream:OutputStream::Stdout, offset:4, data } if data == b"tail")));
    assert!(events.iter().any(|event| matches!(event, AttachEvent::Output { stream:OutputStream::Stderr, offset:0, data } if data == b"err")));
    server.join().unwrap();
}

#[test]
fn attach_recovers_across_endpoint_absence_with_current_status_and_exact_offsets() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let listener = fixture.listener;
    let socket_path = fixture.socket_path.clone();
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let request = read_request(&mut first);
        assert_eq!(request["method"], "attach");
        let id = request["id"].clone();
        for body in [
            json!({"event":"output","stream":"stdout","offset":0,"dataB64":"YWJj"}),
            json!({"event":"output","stream":"stderr","offset":0,"dataB64":"ZXJy"}),
        ] {
            serde_json::to_writer(
                &mut first,
                &json!({"v":1,"id":id.clone(),"ok":true,"body":body}),
            )
            .unwrap();
            first.write_all(b"\n").unwrap();
        }
        drop(listener);
        std::fs::remove_file(&socket_path).unwrap();
        drop(first);
        thread::sleep(Duration::from_millis(80));
        let replacement = UnixListener::bind(&socket_path).unwrap();
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)).unwrap();

        let (mut status, _) = replacement.accept().unwrap();
        let request = read_request(&mut status);
        assert_eq!(request["method"], "status");
        assert_eq!(request["epoch"], 3);
        assert_eq!(request["body"]["runId"], "r-reconnect");
        send_response(
            &mut status,
            &request,
            json!({"runId":"r-reconnect","state":"running","epoch":3}),
        );
        drop(status);

        let (mut second, _) = replacement.accept().unwrap();
        let request = read_request(&mut second);
        assert_eq!(request["method"], "attach");
        assert_eq!(request["body"]["fromOffset"]["stdout"], 3);
        assert_eq!(request["body"]["fromOffset"]["stderr"], 3);
        let id = request["id"].clone();
        for body in [
            json!({"event":"output","stream":"stdout","offset":1,"dataB64":"YmNkZWY="}),
            json!({"event":"output","stream":"stderr","offset":2,"dataB64":"ciE="}),
            json!({"event":"settled","settlement":ok_settlement("r-reconnect")}),
        ] {
            serde_json::to_writer(
                &mut second,
                &json!({"v":1,"id":id.clone(),"ok":true,"body":body}),
            )
            .unwrap();
            second.write_all(b"\n").unwrap();
        }
    });
    let mut events = Vec::new();
    let settlement = client
        .attach_stream("r-reconnect", 0, 0, |event| events.push(event))
        .unwrap();
    assert_eq!(settlement.run_id, "r-reconnect");
    let collect = |stream| {
        events
            .iter()
            .filter_map(|event| match event {
                AttachEvent::Output {
                    stream: actual,
                    data,
                    ..
                } if *actual == stream => Some(data.as_slice()),
                _ => None,
            })
            .flatten()
            .copied()
            .collect::<Vec<_>>()
    };
    assert_eq!(collect(OutputStream::Stdout), b"abcdef");
    assert_eq!(collect(OutputStream::Stderr), b"err!");
    server.join().unwrap();
}

#[test]
fn output_expired_is_preserved_through_bounded_recovery() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let listener = fixture.listener;
    let socket_path = fixture.socket_path.clone();
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let request = read_request(&mut first);
        assert_eq!(request["method"], "attach");
        let id = request["id"].clone();
        let expired = json!({"event":"output_expired","stream":"stdout","availableFrom":4});
        serde_json::to_writer(&mut first, &json!({"v":1,"id":id,"ok":true,"body":expired}))
            .unwrap();
        first.write_all(b"\n").unwrap();
        drop(listener);
        std::fs::remove_file(&socket_path).unwrap();
        drop(first);
        thread::sleep(Duration::from_millis(80));
        let replacement = UnixListener::bind(&socket_path).unwrap();
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)).unwrap();

        let (mut status, _) = replacement.accept().unwrap();
        let request = read_request(&mut status);
        send_response(
            &mut status,
            &request,
            json!({"runId":"r-expired-roll","state":"running","epoch":3}),
        );
        drop(status);
        let (mut second, _) = replacement.accept().unwrap();
        let request = read_request(&mut second);
        assert_eq!(request["body"]["fromOffset"]["stdout"], 4);
        let id = request["id"].clone();
        for body in [
            json!({"event":"output","stream":"stdout","offset":4,"dataB64":"dGFpbA=="}),
            json!({"event":"settled","settlement":ok_settlement("r-expired-roll")}),
        ] {
            serde_json::to_writer(
                &mut second,
                &json!({"v":1,"id":id.clone(),"ok":true,"body":body}),
            )
            .unwrap();
            second.write_all(b"\n").unwrap();
        }
    });
    let events = client.attach("r-expired-roll", 0, 0).unwrap();
    assert!(events.contains(&AttachEvent::OutputExpired {
        stream: OutputStream::Stdout,
        available_from: 4,
    }));
    assert!(events.contains(&AttachEvent::Output {
        stream: OutputStream::Stdout,
        offset: 4,
        data: b"tail".to_vec(),
    }));
    server.join().unwrap();
}

#[test]
fn attach_recovery_exhaustion_is_bounded_and_reports_unknown_scheduler_state() {
    let fixture = make_fixture();
    let timer = Arc::new(FakeReconnectTimer::default());
    let mut client = client(&fixture).with_reconnect_timer(timer.clone());
    let listener = fixture.listener;
    let socket_path = fixture.socket_path.clone();
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let request = read_request(&mut first);
        assert_eq!(request["method"], "attach");
        drop(listener);
        std::fs::remove_file(socket_path).unwrap();
        drop(first);
    });
    let error = client
        .attach_stream("r-exhausted", 0, 0, |_| {})
        .unwrap_err();
    assert!(matches!(error, ClientError::SchedulerUnreachable));
    assert_eq!(timer.now(), Duration::from_secs(5));
    server.join().unwrap();
}

#[test]
fn attach_recovery_budget_survives_trusted_no_progress_reattaches() {
    let fixture = make_fixture();
    let timer = Arc::new(FakeReconnectTimer::default());
    let mut client = client(&fixture).with_reconnect_timer(timer.clone());
    let listener = fixture.listener;
    listener.set_nonblocking(true).unwrap();
    let stop_server = Arc::new(AtomicU64::new(0));
    let status_count = Arc::new(AtomicU64::new(0));
    let attach_count = Arc::new(AtomicU64::new(0));
    let server_stop = stop_server.clone();
    let server_status_count = status_count.clone();
    let server_attach_count = attach_count.clone();
    let server = thread::spawn(move || {
        let accept = || loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    return Some(stream);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if server_stop.load(Ordering::Relaxed) != 0 {
                        return None;
                    }
                    thread::yield_now();
                }
                Err(error) => panic!("private listener accept failed: {error}"),
            }
        };

        let mut initial = accept().expect("initial attach connection");
        let request = read_request(&mut initial);
        assert_eq!(request["method"], "attach");
        server_attach_count.fetch_add(1, Ordering::Relaxed);
        drop(initial);

        while server_stop.load(Ordering::Relaxed) == 0 {
            let Some(mut status) = accept() else {
                break;
            };
            let request = read_request(&mut status);
            assert_eq!(request["method"], "status");
            assert_eq!(request["body"]["runId"], "r-no-progress");
            server_status_count.fetch_add(1, Ordering::Relaxed);
            send_response(
                &mut status,
                &request,
                json!({"runId":"r-no-progress","state":"running","epoch":3}),
            );
            drop(status);

            let Some(mut attach) = accept() else {
                break;
            };
            let request = read_request(&mut attach);
            assert_eq!(request["method"], "attach");
            assert_eq!(request["body"]["runId"], "r-no-progress");
            server_attach_count.fetch_add(1, Ordering::Relaxed);
            drop(attach);
        }
    });

    let error = client
        .attach_stream("r-no-progress", 0, 0, |_| {})
        .unwrap_err();
    stop_server.store(1, Ordering::Relaxed);
    server.join().unwrap();

    assert!(matches!(error, ClientError::SchedulerUnreachable));
    assert_eq!(timer.now(), Duration::from_secs(5));
    let statuses = status_count.load(Ordering::Relaxed);
    let attaches = attach_count.load(Ordering::Relaxed);
    assert!(
        statuses > 1,
        "the trusted server answered repeated status requests"
    );
    assert_eq!(
        attaches,
        statuses + 1,
        "each status was followed by a no-progress attach"
    );
}

#[test]
fn attach_recovery_fails_closed_on_token_key_mode_change_without_retry() {
    let fixture = make_fixture();
    let timer = Arc::new(FakeReconnectTimer::default());
    let mut client = client(&fixture).with_reconnect_timer(timer.clone());
    let listener = fixture.listener;
    let key_path = fixture.root_path.join("token.key");
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let _request = read_request(&mut first);
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        drop(first);
        thread::sleep(Duration::from_millis(100));
        drop(listener);
    });
    let error = client
        .attach_stream("r-untrusted", 0, 0, |_| {})
        .unwrap_err();
    assert!(matches!(error, ClientError::Trust(_)));
    assert_eq!(timer.now(), Duration::from_millis(50));
    server.join().unwrap();
}

#[test]
fn attach_recovery_does_not_retry_malformed_protocol() {
    let fixture = make_fixture();
    let timer = Arc::new(FakeReconnectTimer::default());
    let mut client = client(&fixture).with_reconnect_timer(timer.clone());
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let _request = read_request(&mut first);
        first.write_all(b"not-json\n").unwrap();
    });
    let error = client
        .attach_stream("r-malformed", 0, 0, |_| {})
        .unwrap_err();
    assert!(matches!(error, ClientError::Decode(_)));
    assert_eq!(timer.now(), Duration::ZERO);
    server.join().unwrap();
}

#[test]
fn attach_recovery_refuses_an_unexpected_authority_epoch_without_retry() {
    let fixture = make_fixture();
    let timer = Arc::new(FakeReconnectTimer::default());
    let mut client = client(&fixture).with_reconnect_timer(timer.clone());
    let listener = fixture.listener;
    let root_path = fixture.root_path.clone();
    let authority = fixture.authority.clone();
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let _request = read_request(&mut first);
        let changed = Authority {
            epoch: 4,
            ..authority
        };
        write_authority(&root_path, &changed);
        drop(first);
    });
    let error = client
        .attach_stream("r-epoch-change", 0, 0, |_| {})
        .unwrap_err();
    assert!(matches!(error, ClientError::Trust(_)));
    assert_eq!(timer.now(), Duration::from_millis(50));
    server.join().unwrap();
}

#[test]
fn cancel_retries_the_same_run_during_endpoint_roll() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let listener = fixture.listener;
    let socket_path = fixture.socket_path.clone();
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let request = read_request(&mut first);
        assert_eq!(request["method"], "cancel");
        assert_eq!(request["body"]["runId"], "r-cancel-roll");
        drop(listener);
        std::fs::remove_file(&socket_path).unwrap();
        drop(first);
        thread::sleep(Duration::from_millis(80));
        let replacement = UnixListener::bind(&socket_path).unwrap();
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (mut second, _) = replacement.accept().unwrap();
        let retry = read_request(&mut second);
        assert_eq!(retry["method"], "cancel");
        assert_eq!(retry["body"]["runId"], "r-cancel-roll");
        send_response(&mut second, &retry, json!({"stopping":true}));
    });
    assert_eq!(
        client.cancel("r-cancel-roll", "interrupted").unwrap()["stopping"],
        true
    );
    server.join().unwrap();
}

#[test]
fn recovery_root_open_accepts_a_trusted_authority_while_the_socket_is_absent() {
    let fixture = make_fixture();
    let socket_path = fixture.socket_path.clone();
    drop(fixture.listener);
    std::fs::remove_file(socket_path).unwrap();
    assert!(HostRunRoot::open(&fixture.root_path).is_err());
    let (root, authority) = HostRunRoot::open_for_recovery(&fixture.root_path).unwrap();
    assert_eq!(root.path(), fixture.root_path);
    assert_eq!(authority.epoch, fixture.authority.epoch);
}

#[test]
fn authenticated_previous_epoch_status_recovers_within_the_bounded_window() {
    let mut fixture = make_fixture();
    fixture.authority.epoch = 4;
    write_authority(&fixture.root_path, &fixture.authority);
    write_token_keys(&fixture.root_path, 4, [6u8; 32], Some((3, [5u8; 32])));
    let client = client(&fixture);
    let socket_path = fixture.socket_path.clone();
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        drop(listener);
        std::fs::remove_file(&socket_path).unwrap();
        thread::sleep(Duration::from_millis(80));
        let replacement = UnixListener::bind(&socket_path).unwrap();
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (mut status, _) = replacement.accept().unwrap();
        let request = read_request(&mut status);
        assert_eq!(request["method"], "status");
        assert_eq!(request["epoch"], 4);
        assert_eq!(request["body"]["runId"], "r-previous");
        send_response(
            &mut status,
            &request,
            json!({"runId":"r-previous","state":"running","epoch":4}),
        );
    });

    let now = Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap();
    let payload = json!({
        "runId":"r-previous",
        "epoch":3,
        "class":"heavy",
        "root":fixture.root_path.clone(),
        "exp":now.timestamp_millis() + 60_000
    });
    let payload_part = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&payload).unwrap());
    let mac = crate::token::hmac_sha256(&[5u8; 32], payload_part.as_bytes());
    let token = format!(
        "{payload_part}.{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac)
    );
    let accepted = client
        .with_clock(Arc::new(FixedClock(now)))
        .validate_parent_token(Some(&token), &fixture.root_path)
        .unwrap()
        .unwrap();
    assert_eq!(accepted.run_id, "r-previous");
    assert_eq!(accepted.epoch, 3);
    server.join().unwrap();
}

#[test]
fn malformed_settlement_is_refused_and_unknown_result_is_not_invented() {
    let malformed = json!({"format":"host.run.settlement","version":1,"runId":"r","outcome":"passed","launched":true,"settledAt":"2026-10-01T14:00:00Z","result":null,"containers":[]});
    assert!(matches!(
        parse_settlement(&malformed),
        Err(ClientError::InvalidSettlement(_))
    ));
    let lost = json!({"format":"host.run.settlement","version":1,"runId":"r","outcome":"lost","launched":true,"settledAt":"2026-10-01T14:00:00Z","result":null,"containers":[]});
    assert_eq!(parse_settlement(&lost).unwrap().result, None);
    let actual = json!({
        "format":"host.run.settlement","version":1,"runId":"r","outcome":"failed","launched":true,
        "settledAt":"2026-10-01T14:00:00Z","containers":[],
        "result":{"format":"host.run.result","version":1,"runId":"r","epoch":3,"pgid":42,
            "startIdentity":"42@boot:1","exitCode":1,"signal":null,"startedAt":"2026-10-01T13:00:00Z",
            "endedAt":"2026-10-01T13:01:00Z","wallMs":60000,"cpuMs":null,"escapedDescendants":[]}
    });
    assert_eq!(
        parse_settlement(&actual).unwrap().result.unwrap()["exitCode"],
        1
    );
}

#[test]
fn cancellation_retries_are_protocol_idempotent_for_the_same_run() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert_eq!(request["method"], "cancel");
            assert_eq!(request["body"]["runId"], "r4");
            send_response(&mut stream, &request, json!({"stopping":true}));
        }
    });
    client.cancel("r4", "test").unwrap();
    client.cancel("r4", "test").unwrap();
    server.join().unwrap();
}

#[test]
fn pending_facts_replay_until_stored_copy_ack_and_conflict_keeps_fact() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let fact = container_started_fact("r5", 3, "docker", "c1", &FixedClock(Utc::now())).unwrap();
    let listener = fixture.listener;
    let first = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _request = read_request(&mut stream);
        drop(stream);
    });
    assert!(client.report_facts(std::slice::from_ref(&fact)).is_err());
    first.join().unwrap();
    let _ = std::fs::remove_file(&fixture.socket_path);
    let listener = UnixListener::bind(&fixture.socket_path).unwrap();
    std::fs::set_permissions(&fixture.socket_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let fact_copy = fact.clone();
    let second = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert_eq!(request["body"]["facts"][0], fact_copy);
        send_response(&mut stream, &request, json!({"acks":[fact_copy]}));
    });
    assert_eq!(client.flush_pending_facts().unwrap(), vec![fact.clone()]);
    second.join().unwrap();
    assert_eq!(
        std::fs::read(fixture.root_path.join("pending-facts.jsonl")).unwrap(),
        b""
    );

    let _ = std::fs::remove_file(&fixture.socket_path);
    let listener = UnixListener::bind(&fixture.socket_path).unwrap();
    std::fs::set_permissions(&fixture.socket_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let changed = {
        let mut copy = fact.clone();
        copy["containerId"] = json!("other");
        copy
    };
    let fact_id = fact["factId"].clone();
    let third = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        send_response(&mut stream, &request, json!({"acks":[changed]}));
    });
    assert!(matches!(
        client.report_facts(std::slice::from_ref(&fact)),
        Err(ClientError::ReportConflict(_))
    ));
    third.join().unwrap();
    let pending: Value = serde_json::from_slice(
        std::fs::read(fixture.root_path.join("pending-facts.jsonl"))
            .unwrap()
            .split(|b| *b == b'\n')
            .next()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(pending["factId"], fact_id);
}

#[test]
fn incomplete_fact_ack_is_reported_and_remains_pending() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let fact = crate::nested_fact("p", "qa:one", &FixedClock(Utc::now())).unwrap();
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        send_response(&mut stream, &request, json!({"acks":[]}));
    });
    assert!(matches!(
        client.report_facts(&[fact]),
        Err(ClientError::ReportUnacked(_))
    ));
    server.join().unwrap();
    let pending = std::fs::read_to_string(fixture.root_path.join("pending-facts.jsonl")).unwrap();
    assert!(!pending.trim().is_empty());
}

#[test]
fn typed_report_covers_nested_override_and_container_lifecycle_facts() {
    let fixture = make_fixture();
    let mut client = client(&fixture).with_clock(Arc::new(FixedClock(
        Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap(),
    )));
    let facts = [
        crate::HostRunFact::Nested {
            parent_run_id: "parent".into(),
            selector: "qa:inner".into(),
        },
        crate::HostRunFact::Override {
            reason: "operator request".into(),
            selector: "qa:one".into(),
        },
        crate::HostRunFact::ContainerStarted {
            run_id: "run".into(),
            epoch: 3,
            runtime: "docker".into(),
            container_id: "container".into(),
        },
        crate::HostRunFact::ContainerRemoved {
            run_id: "run".into(),
            epoch: 3,
            runtime: "docker".into(),
            container_id: "container".into(),
            removed: Some(true),
        },
    ];
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        let sent = request["body"]["facts"].as_array().unwrap();
        assert_eq!(sent[0]["kind"], "nested");
        assert_eq!(sent[1]["kind"], "override");
        assert_eq!(sent[2]["event"], "started");
        assert_eq!(sent[3]["event"], "removed");
        assert!(sent.iter().all(|fact| fact["factId"].is_string()));
        send_response(&mut stream, &request, json!({"acks":sent}));
    });
    let acknowledgements = client.report_typed_facts(&facts).unwrap();
    assert_eq!(acknowledgements.len(), facts.len());
    server.join().unwrap();
}

#[test]
fn concurrent_fact_writers_serialize_and_keep_both_facts() {
    let fixture = make_fixture();
    let fact_a = crate::nested_fact("parent", "qa:a", &FixedClock(Utc::now())).unwrap();
    let fact_b =
        crate::container_started_fact("r", 3, "docker", "c", &FixedClock(Utc::now())).unwrap();
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            send_response(
                &mut stream,
                &request,
                json!({"acks":request["body"]["facts"]}),
            );
        }
    });
    let root_path = fixture.root_path.clone();
    let fact_a_clone = fact_a.clone();
    let thread_a = thread::spawn(move || {
        let (root, authority) = HostRunRoot::open(&root_path).unwrap();
        HostRunClient::open(root, authority)
            .report_facts(&[fact_a_clone])
            .unwrap();
    });
    let root_path = fixture.root_path.clone();
    let thread_b = thread::spawn(move || {
        let (root, authority) = HostRunRoot::open(&root_path).unwrap();
        HostRunClient::open(root, authority)
            .report_facts(&[fact_b])
            .unwrap();
    });
    thread_a.join().unwrap();
    thread_b.join().unwrap();
    server.join().unwrap();
    assert_eq!(
        std::fs::read(fixture.root_path.join("pending-facts.jsonl")).unwrap(),
        b""
    );
}

#[test]
fn unknown_identity_and_legacy_fractional_identity_never_match() {
    assert!(!start_identity_matches(u32::MAX, ""));
    let pid = std::process::id();
    let identity = canonical_start_identity(pid).unwrap();
    #[cfg(target_os = "macos")]
    let legacy = format!("{}.000Z", identity.strip_suffix('Z').unwrap());
    #[cfg(target_os = "linux")]
    let legacy = format!("{identity}.000Z");
    assert!(!start_identity_matches(pid, &legacy));
}

#[test]
fn offline_journal_writers_serialize_and_keep_every_fact() {
    let fixture = make_fixture();
    let handles: Vec<_> = (0..4)
        .map(|index| {
            let path = fixture.root_path.clone();
            thread::spawn(move || {
                let root = HostRunRoot::open_journal(&path, false).unwrap();
                let fact = crate::override_fact(
                    "outage drill",
                    &format!("selector-{index}"),
                    &FixedClock(Utc::now()),
                )
                .unwrap();
                crate::journal_facts_offline(&root, std::slice::from_ref(&fact)).unwrap();
                // An identical copy is a no-op, not a duplicate line.
                crate::journal_facts_offline(&root, &[fact]).unwrap();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let journal = std::fs::read_to_string(fixture.root_path.join("pending-facts.jsonl")).unwrap();
    assert_eq!(journal.lines().count(), 4);
}

#[test]
fn journal_root_is_created_private_and_unsafe_roots_are_refused() {
    use std::os::unix::fs::PermissionsExt;
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("state/host-run");
    assert!(HostRunRoot::open_journal(&root, false).is_err());
    let created = HostRunRoot::open_journal(&root, true).unwrap();
    let mode = std::fs::metadata(created.path())
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(mode, 0o700);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(HostRunRoot::open_journal(&root, true).is_err());
}
