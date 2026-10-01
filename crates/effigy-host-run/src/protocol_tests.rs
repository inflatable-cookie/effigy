use crate::transport::{parse_settlement, parse_test_frame};
use crate::{
    canonical_start_identity, container_started_fact, start_identity_matches, AttachEvent,
    Authority, ClientError, Clock, HostRunClient, HostRunRoot, IdentityProvider, OutputStream,
    TokenKeys,
};
use base64::Engine;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Cursor, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
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
    Fixture {
        _temp: temp,
        root_path,
        socket_path,
        listener,
        authority,
    }
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

#[test]
fn authority_and_socket_are_owned_private_and_peer_identity_must_match() {
    let fixture = make_fixture();
    let root = HostRunRoot::open(&fixture.root_path).unwrap().0;
    let bad_authority = fixture.authority.clone();
    let mut client = HostRunClient::open(root, bad_authority)
        .with_identity_provider(Arc::new(FixedIdentity("wrong-generation")));
    let (release, wait_for_release) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (_stream, _) = fixture.listener.accept().unwrap();
        wait_for_release.recv().unwrap();
    });
    let error = client.status(&json!({})).unwrap_err();
    release.send(()).unwrap();
    assert!(matches!(error, ClientError::SchedulerUnreachable));
    server.join().unwrap();

    let fixture = make_fixture();
    let root = HostRunRoot::open(&fixture.root_path).unwrap().0;
    let wrong_pid = Authority {
        pid: fixture.authority.pid.saturating_add(1),
        ..fixture.authority.clone()
    };
    let mut client = HostRunClient::open(root, wrong_pid);
    let server = thread::spawn(move || {
        let _ = fixture.listener.accept().unwrap();
    });
    assert!(matches!(
        client.status(&json!({})),
        Err(ClientError::SchedulerUnreachable)
    ));
    server.join().unwrap();

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
        &fixture.root_path.join("authority.json"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(HostRunRoot::open(&fixture.root_path).is_err());

    let fixture = make_fixture();
    std::fs::set_permissions(
        &fixture.root_path.join("run"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(HostRunRoot::open(&fixture.root_path).is_err());

    let fixture = make_fixture();
    std::fs::set_permissions(&fixture.root_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(HostRunRoot::open(&fixture.root_path).is_err());
}

#[test]
fn token_key_requires_private_regular_file() {
    let fixture = make_fixture();
    let path = fixture.root_path.join("token.key");
    let content = json!({"format":"host.run.keys","version":1,"current":{"epoch":3,"keyB64":base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([5u8;32])}});
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
    let content = json!({"format":"host.run.keys","version":1,"current":{"epoch":3,"keyB64":base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([5u8;32])}});
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
fn attach_reconnect_resumes_at_delivered_byte_offset() {
    let fixture = make_fixture();
    let mut client = client(&fixture);
    let listener = fixture.listener;
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let request = read_request(&mut first);
        let first_frame = json!({
            "v":1,"id":request["id"],"ok":true,
            "body":{"event":"output","stream":"stdout","offset":0,"dataB64":"YWJj"}
        });
        serde_json::to_writer(&mut first, &first_frame).unwrap();
        first.write_all(b"\n").unwrap();
        drop(first);

        let (mut second, _) = listener.accept().unwrap();
        let request = read_request(&mut second);
        assert_eq!(request["body"]["fromOffset"]["stdout"], 3);
        let output = json!({
            "v":1,"id":request["id"],"ok":true,
            "body":{"event":"output","stream":"stdout","offset":3,"dataB64":"ZGVm"}
        });
        serde_json::to_writer(&mut second, &output).unwrap();
        second.write_all(b"\n").unwrap();
        let settled = json!({
            "v":1,"id":request["id"],"ok":true,
            "body":{"event":"settled","settlement":ok_settlement("r-reconnect")}
        });
        serde_json::to_writer(&mut second, &settled).unwrap();
        second.write_all(b"\n").unwrap();
    });
    let mut events = Vec::new();
    let settlement = client
        .attach_stream("r-reconnect", 0, 0, |event| events.push(event))
        .unwrap();
    assert_eq!(settlement.run_id, "r-reconnect");
    assert_eq!(
        events
            .iter()
            .filter_map(|event| match event {
                AttachEvent::Output {
                    stream: OutputStream::Stdout,
                    offset,
                    data,
                } => Some((*offset, data.as_slice())),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![(0, b"abc".as_slice()), (3, b"def".as_slice())]
    );
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
        &std::fs::read(fixture.root_path.join("pending-facts.jsonl"))
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
