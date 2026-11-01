use super::support::*;
use effigy_kernel::ctx::ObserveContext;
use effigy_kernel::custody::Stream;
use effigy_kernel::id::Digest;
use effigy_kernel::root::{Family, StateRoot};
use effigy_kernel::run::{self, Attachment, Client, Release, RequestId};
use effigy_kernel::workspace_channel::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::rc::Rc;

fn expect_admission(ledger: &SharedLedger) {
    ledger.borrow_mut().expect([
        Call::Resolve(body()),
        Call::Entropy,
        Call::Admit(
            id(10),
            serde_json::to_value(resolved("child")).expect("expected host plan"),
        ),
    ]);
}

#[test]
fn retry_and_reconnect_never_double_admit_and_changed_digest_conflicts() {
    let host = Host::new();
    let (owner, session) = host.session();
    let ledger = Rc::new(RefCell::new(Ledger::default()));
    let runs = Rc::new(RefCell::new(vec![]));
    expect_admission(&ledger);
    let request = request(session.session(), 1, RequestBody::Invoke(body()));
    for _ in 0..2 {
        // A fresh dispatcher has forgotten all connection-local state. Only
        // the real kernel journal can supply the answer after reconnect.
        let mut server = server(
            &host,
            session.session(),
            ledger.clone(),
            runs.clone(),
            session_operations(),
        );
        let mut frames = authenticated(session.session());
        frames.extend([
            Frame::Request(request.clone()),
            Frame::Request(request.clone()),
        ]);
        let mut changed = request.clone();
        if let RequestBody::Invoke(body) = &mut changed.body {
            body.args.push("changed".into());
        }
        frames.push(Frame::Request(changed));
        let mut pipe = Pipe::new(&frames);
        server
            .serve(
                &mut pipe,
                &host.cx,
                &host.observe(),
                &host.root,
                &mut NoStreams,
            )
            .expect("replay");
        let accepted = pipe
            .frames()
            .into_iter()
            .filter_map(|frame| match frame {
                Frame::Response(Response {
                    outcome: ResponseOutcome::Accepted(a),
                    ..
                }) => Some(a.run),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(accepted, vec![id(10), id(10)]);
        refused(&pipe, RefusalCode::IdConflict);
    }
    assert_eq!(runs.borrow().len(), 1, "same-ID retry double-admitted");
    ledger.borrow().done();
    settle(&host, &runs);
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("session cleanup");
    host.assert_foreign();
}

#[test]
fn owner_loss_returns_unknown_and_never_readmits() {
    let host = Host::new();
    let (owner, session) = host.session();
    let ledger = Rc::new(RefCell::new(Ledger::default()));
    let runs = Rc::new(RefCell::new(vec![]));
    expect_admission(&ledger);
    let mut server = server(
        &host,
        session.session(),
        ledger.clone(),
        runs.clone(),
        session_operations(),
    );
    let request = request(session.session(), 1, RequestBody::Invoke(body()));
    let mut frames = authenticated(session.session());
    frames.push(Frame::Request(request.clone()));
    let mut pipe = Pipe::new(&frames);
    server
        .serve(
            &mut pipe,
            &host.cx,
            &host.observe(),
            &host.root,
            &mut NoStreams,
        )
        .expect("first admission");
    runs.borrow_mut().clear(); // Lose the live workload owner, retaining its journal.
    let mut frames = authenticated(session.session());
    frames.push(Frame::Request(request));
    let mut pipe = Pipe::new(&frames);
    server
        .serve(
            &mut pipe,
            &host.cx,
            &host.observe(),
            &host.root,
            &mut NoStreams,
        )
        .expect("reconnect");
    refused(&pipe, RefusalCode::Unknown);
    ledger.borrow().done();
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("session cleanup");
    host.assert_foreign();
}

fn attached(session: run::Session) -> Vec<Frame> {
    let mut frames = authenticated(session);
    frames.push(Frame::Request(request(
        session,
        1,
        RequestBody::Invoke(body()),
    )));
    frames.push(Frame::Request(request(
        session,
        2,
        RequestBody::Attach(AttachBody {
            run: id(10),
            streams: vec![StreamKind::TerminalOutput],
            cursor: None,
            cursors: None,
        }),
    )));
    frames
}

#[test]
fn reused_server_does_not_transfer_owner_to_a_fresh_connection() {
    for different_credential in [false, true] {
        let host = Host::new();
        let (owner, session) = host.session();
        let ledger = Rc::new(RefCell::new(Ledger::default()));
        let runs = Rc::new(RefCell::new(vec![]));
        expect_admission(&ledger);
        let mut server = server(
            &host,
            session.session(),
            ledger.clone(),
            runs.clone(),
            session_operations(),
        );
        let other = Secret::new([42; 32]);
        server.grants_mut().issue(
            other.clone(),
            GrantBinding {
                host_user: "operator".into(),
                checkout: "fixture".into(),
                session: session.session(),
                environment_generation: effigy_kernel::id::Generation::new(4),
                container_id: "full-container-id".into(),
                vm_boot_id: "boot-1".into(),
            },
            session_operations(),
            [Target::Host].into_iter().collect(),
        );
        let mut first = authenticated(session.session());
        first.push(Frame::Request(request(
            session.session(),
            1,
            RequestBody::Invoke(body()),
        )));
        server
            .serve(
                &mut Pipe::new(&first),
                &host.cx,
                &host.observe(),
                &host.root,
                &mut NoStreams,
            )
            .expect("admit on first connection");
        let mut second = authenticated(session.session());
        if different_credential {
            second[1] = Frame::Authenticate(other);
        }
        second.push(Frame::Request(request(
            session.session(),
            1,
            RequestBody::Invoke(body()),
        )));
        second.push(Frame::Request(request(
            session.session(),
            2,
            RequestBody::Attach(AttachBody {
                run: id(10),
                streams: vec![StreamKind::TerminalOutput],
                cursor: None,
                cursors: None,
            }),
        )));
        let mut pipe = Pipe::new(&second);
        server
            .serve(
                &mut pipe,
                &host.cx,
                &host.observe(),
                &host.root,
                &mut NoStreams,
            )
            .expect("attach on second connection");
        let replayed = pipe.frames().iter().any(|frame| {
            matches!(frame,
        Frame::Response(Response { id: request_id, outcome: ResponseOutcome::Accepted(accepted) })
            if *request_id == RequestId::new([1; 16]) && accepted.run == id(10))
        });
        let stops = host.stops(id(10));
        settle(&host, &runs);
        owner
            .settle(&cleanup(&host.cx), &mut NoEffects)
            .expect("session cleanup");
        host.assert_foreign();
        ledger.borrow().done();
        assert!(replayed, "replay must return the original admission");
        assert_eq!(
            stops, 0,
            "a fresh connection's EOF stopped the first connection's run"
        );
    }
}

#[test]
fn fresh_connection_cannot_detach_another_connections_owner() {
    let host = Host::new();
    let (owner, session) = host.session();
    let ledger = Rc::new(RefCell::new(Ledger::default()));
    let runs = Rc::new(RefCell::new(vec![]));
    expect_admission(&ledger);
    let mut server = server(
        &host,
        session.session(),
        ledger.clone(),
        runs.clone(),
        session_operations(),
    );
    let mut first = authenticated(session.session());
    first.push(Frame::Request(request(
        session.session(),
        1,
        RequestBody::Invoke(body()),
    )));
    server
        .serve(
            &mut Pipe::new(&first),
            &host.cx,
            &host.observe(),
            &host.root,
            &mut NoStreams,
        )
        .expect("admission");
    let mut second = authenticated(session.session());
    second.push(Frame::Request(request(
        session.session(),
        3,
        RequestBody::Cancel(RunBody { run: id(10) }),
    )));
    second.push(Frame::Request(request(
        session.session(),
        2,
        RequestBody::Detach(RunBody { run: id(10) }),
    )));
    let mut pipe = Pipe::new(&second);
    server
        .serve(
            &mut pipe,
            &host.cx,
            &host.observe(),
            &host.root,
            &mut NoStreams,
        )
        .expect("second connection");
    let refusals = pipe
        .frames()
        .iter()
        .filter(|frame| {
            matches!(
                frame,
                Frame::Response(Response {
                    outcome: ResponseOutcome::Refused(_),
                    ..
                })
            )
        })
        .count();
    let stops = host.stops(id(10));
    settle(&host, &runs);
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("session cleanup");
    ledger.borrow().done();
    host.assert_foreign();
    assert_eq!(
        refusals, 2,
        "a fresh connection used another connection's owner"
    );
    assert_eq!(
        stops, 0,
        "a fresh connection stopped another connection's run"
    );
}
fn input() -> StreamFrame {
    StreamFrame {
        id: RequestId::new([2; 16]),
        stream_id: 0,
        seq: 0,
        kind: StreamKind::TerminalInput,
        payload: Some(encode_stream_payload(b"typed input")),
        rows: None,
        columns: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalEffect {
    Input(Vec<u8>),
    Resize(TerminalSize),
}

/// The front end owns terminal delivery. Script every call and fence it against
/// the actual attachment journal immediately before recording the effect.
struct StrictTerminal {
    observe: ObserveContext,
    root: StateRoot,
    expected: VecDeque<TerminalEffect>,
    latest_size: Option<TerminalSize>,
}
impl StrictTerminal {
    fn record(&mut self, attachment: &Attachment, effect: TerminalEffect) -> Result<(), Refusal> {
        if !matches!(
            attachment.attached_observed(&self.observe, &self.root),
            Ok(true)
        ) {
            return Err(Refusal::invalid("terminal attachment is no longer live"));
        }
        assert_eq!(
            attachment.run(),
            id(10),
            "terminal effect targeted another run"
        );
        assert_eq!(
            self.expected.pop_front(),
            Some(effect.clone()),
            "unexpected terminal effect"
        );
        if let TerminalEffect::Resize(size) = effect {
            self.latest_size = Some(size);
        }
        Ok(())
    }
    fn done(&self) {
        assert!(
            self.expected.is_empty(),
            "terminal effects not delivered: {:?}",
            self.expected
        );
    }
}
impl Terminal for StrictTerminal {
    fn input(&mut self, attachment: &Attachment, bytes: &[u8]) -> Result<(), Refusal> {
        self.record(attachment, TerminalEffect::Input(bytes.to_vec()))
    }
    fn resize(&mut self, attachment: &Attachment, size: TerminalSize) -> Result<(), Refusal> {
        self.record(attachment, TerminalEffect::Resize(size))
    }
}

fn retained(
    host: &Host,
    plan: Vec<(i64, Stream)>,
    expected: Vec<TerminalEffect>,
) -> RetainedStreams<StrictTerminal> {
    RetainedStreams::new(
        host.observe(),
        host.root.clone(),
        id(10),
        plan,
        StrictTerminal {
            observe: host.observe(),
            root: host.root.clone(),
            expected: expected.into(),
            latest_size: None,
        },
    )
}

/// Only the fresh fixture is writable. These are custody's actual segment
/// names and private permissions; RetainedStreams/run::read_log own the read.
fn write_segment(host: &Host, stream: Stream, base: u64, bytes: &[u8]) {
    let directory = host
        .root
        .path()
        .join(Family::Runs.dir_name())
        .join(id(10).to_string());
    assert!(directory.starts_with(host.fixture.path()));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .expect("private captured-log directory");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join(format!("1.{}.{base}.log", stream.name())))
        .expect("fresh captured segment");
    file.write_all(bytes).expect("captured segment bytes");
}

fn data_frames(frames: &[Frame]) -> Vec<&StreamFrame> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Stream(stream) => Some(stream),
            _ => None,
        })
        .collect()
}

fn invalid_input(mut inputs: Vec<StreamFrame>, delivered: usize) {
    let host = Host::new();
    let (owner, session) = host.session();
    let ledger = Rc::new(RefCell::new(Ledger::default()));
    let runs = Rc::new(RefCell::new(vec![]));
    expect_admission(&ledger);
    let mut server = server(
        &host,
        session.session(),
        ledger.clone(),
        runs.clone(),
        session_operations(),
    );
    let mut frames = attached(session.session());
    frames.extend(inputs.drain(..).map(Frame::Stream));
    let mut pipe = Pipe::new(&frames);
    let mut streams = retained(
        &host,
        vec![(1, Stream::Stdout)],
        vec![TerminalEffect::Input(b"typed input".to_vec()); delivered],
    );
    let result = server.serve(
        &mut pipe,
        &host.cx,
        &host.observe(),
        &host.root,
        &mut streams,
    );
    // Clean up before asserting a channel defect, including all foreign guards.
    settle(&host, &runs);
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("session cleanup");
    host.assert_foreign();
    ledger.borrow().done();
    streams.terminal_mut().done();
    assert!(
        matches!(
            result,
            Err(Refusal {
                code: RefusalCode::InvalidRequest,
                ..
            })
        ),
        "stream identity/sequence must refuse before dispatch, got {result:?}"
    );
    refused(&pipe, RefusalCode::InvalidRequest);
}

#[test]
fn terminal_input_for_unknown_stream_is_refused() {
    let mut input = input();
    input.stream_id = 999;
    invalid_input(vec![input], 0);
}
#[test]
fn terminal_input_for_another_request_is_refused() {
    let mut input = input();
    input.id = RequestId::new([99; 16]);
    invalid_input(vec![input], 0);
}
#[test]
fn repeated_terminal_input_is_not_replayed() {
    invalid_input(vec![input(), input()], 1);
}
#[test]
fn out_of_order_terminal_input_is_refused() {
    let mut ahead = input();
    ahead.seq = 2;
    invalid_input(vec![input(), ahead], 1);
}

struct Chunks(VecDeque<StreamChunk>);
impl Streams for Chunks {
    fn next_chunk(&mut self, _attachment: Option<&Attachment>) -> Option<StreamChunk> {
        self.0.pop_front()
    }
}

#[test]
fn backpressure_is_bounded_and_cancel_stays_live_without_ack() {
    let host = Host::new();
    let (owner, session) = host.session();
    let ledger = Rc::new(RefCell::new(Ledger::default()));
    let runs = Rc::new(RefCell::new(vec![]));
    expect_admission(&ledger);
    ledger.borrow_mut().expect([Call::Entropy]); // cancellation identity only
    let mut server = server(
        &host,
        session.session(),
        ledger.clone(),
        runs.clone(),
        session_operations(),
    );
    let mut frames = attached(session.session());
    frames.push(Frame::Request(request(
        session.session(),
        3,
        RequestBody::Cancel(RunBody { run: id(10) }),
    )));
    let mut pipe = Pipe::new(&frames);
    let stops_at_cancel_response = Rc::new(RefCell::new(None));
    let recorded = stops_at_cancel_response.clone();
    let database_path = host.root.path().join(effigy_kernel::store::DB_NAME);
    pipe.on_response = Some(Box::new(move |response| {
        if response.id == RequestId::new([3; 16]) {
            let database = rusqlite::Connection::open_with_flags(
                &database_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .expect("fixture journal");
            let count: i64 = database
                .query_row(
                    "SELECT count(*) FROM requests WHERE run_id=?1 AND state='pending'",
                    [id(10).to_string()],
                    |row| row.get(0),
                )
                .expect("cancel committed before response");
            *recorded.borrow_mut() = Some(count);
        }
    }));
    let mut chunks = Chunks(
        (0..20)
            .map(|_| StreamChunk {
                stream_id: 0,
                kind: StreamKind::TerminalOutput,
                cursor: None,
                bytes: vec![42; MAX_STREAM_PAYLOAD],
            })
            .collect(),
    );
    let result = server.serve(
        &mut pipe,
        &host.cx,
        &host.observe(),
        &host.root,
        &mut chunks,
    );
    let stops = host.stops(id(10));
    settle(&host, &runs);
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("session cleanup");
    host.assert_foreign();
    result.expect("cancel remains live under a full stream window");
    ledger.borrow().done();
    let frames = pipe.frames();
    let data = frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Stream(s) => Some(s),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(data.len(), 16);
    assert_eq!(
        data.iter()
            .map(
                |s| decode_stream_payload(s.payload.as_deref().expect("payload"))
                    .expect("base64")
                    .len()
            )
            .sum::<usize>(),
        512 * 1024
    );
    assert_eq!(
        data.iter().map(|s| s.seq).collect::<Vec<_>>(),
        (0..16).collect::<Vec<_>>()
    );
    assert_eq!(
        *stops_at_cancel_response.borrow(),
        Some(1),
        "cancel must commit while backpressured, before EOF can request stop"
    );
    assert!(
        stops >= 1,
        "EOF may coalesce a matching pending stop, never erase it"
    );
    assert!(frames.iter().any(|f| matches!(f, Frame::Response(Response { id, outcome: ResponseOutcome::Observed(_), .. }) if *id == RequestId::new([3;16]))));
    assert_eq!(
        chunks.0.len(),
        3,
        "source must backpressure after at most one held chunk"
    );
}

#[test]
fn sequence_is_monotonic_per_stream_across_output_kinds() {
    let mut output = Outbound::new();
    let id = RequestId::new([1; 16]);
    assert!(output
        .push_chunk(id, 0, StreamKind::Stdout, b"out")
        .is_empty());
    assert!(output
        .push_chunk(id, 0, StreamKind::Stderr, b"err")
        .is_empty());
    let frames = output.take();
    assert_eq!(
        frames.iter().map(|s| s.seq).collect::<Vec<_>>(),
        vec![0, 1],
        "§8.11 sequence is per stream, not per (stream, kind)"
    );
}

#[test]
fn eof_detach_and_secondary_loss_preserve_foreign_custody() {
    for release in [Release::Eof, Release::Detach] {
        let host = Host::new();
        let (owner, session) = host.session();
        let ledger = Rc::new(RefCell::new(Ledger::default()));
        let runs = Rc::new(RefCell::new(vec![]));
        expect_admission(&ledger);
        let mut server = server(
            &host,
            session.session(),
            ledger.clone(),
            runs.clone(),
            session_operations(),
        );
        let mut frames = attached(session.session());
        if release == Release::Detach {
            frames.push(Frame::Request(request(
                session.session(),
                3,
                RequestBody::Detach(RunBody { run: id(10) }),
            )));
        }
        let mut pipe = Pipe::new(&frames);
        server
            .serve(
                &mut pipe,
                &host.cx,
                &host.observe(),
                &host.root,
                &mut NoStreams,
            )
            .expect("owner release");
        assert_eq!(host.stops(id(10)), usize::from(release == Release::Eof));
        if release == Release::Detach {
            let mut secondary = run::attach_secondary(
                &host.cx,
                &host.root,
                session.session(),
                id(10),
                run::AttachmentId::new([55; 16]),
            )
            .expect("secondary");
            assert_eq!(secondary.client(), Client::Secondary);
            secondary
                .release(&host.cx, &host.root, Release::Eof)
                .expect("secondary EOF");
            assert_eq!(
                host.stops(id(10)),
                0,
                "secondary disconnect stopped the owner"
            );
        }
        settle(&host, &runs);
        owner
            .settle(&cleanup(&host.cx), &mut NoEffects)
            .expect("session cleanup");
        host.assert_foreign();
        ledger.borrow().done();
    }
}

#[test]
fn reconnect_cursor_must_resume_or_report_a_gap_never_restart_at_zero() {
    let host = Host::new();
    let (owner, session) = host.session();
    let ledger = Rc::new(RefCell::new(Ledger::default()));
    let runs = Rc::new(RefCell::new(vec![]));
    expect_admission(&ledger);
    let mut server = server(
        &host,
        session.session(),
        ledger.clone(),
        runs.clone(),
        session_operations(),
    );
    let mut frames = attached(session.session());
    frames.push(Frame::Request(request(
        session.session(),
        3,
        RequestBody::Detach(RunBody { run: id(10) }),
    )));
    let mut pipe = Pipe::new(&frames);
    let mut chunks = Chunks(
        [StreamChunk {
            stream_id: 0,
            kind: StreamKind::TerminalOutput,
            cursor: None,
            bytes: b"first".to_vec(),
        }]
        .into(),
    );
    server
        .serve(
            &mut pipe,
            &host.cx,
            &host.observe(),
            &host.root,
            &mut chunks,
        )
        .expect("first stream");
    // A reconnect is a fresh dispatcher/secondary; the previous owner explicitly
    // detached. Reusing its dispatcher would try to reopen a released owner.
    server = super::support::server(
        &host,
        session.session(),
        ledger.clone(),
        runs.clone(),
        session_operations(),
    );
    let mut frames = authenticated(session.session());
    frames.push(Frame::Request(request(
        session.session(),
        4,
        RequestBody::Attach(AttachBody {
            run: id(10),
            streams: vec![StreamKind::Stdout],
            cursor: Some(40),
            cursors: None,
        }),
    )));
    let mut pipe = Pipe::new(&frames);
    // Captured files, rather than a fabricated cursor, are the retained
    // authority. The prefix through 100 has rotated away since this client
    // consumed offset 40.
    write_segment(&host, Stream::Stdout, 100, b"current");
    let mut chunks = retained(&host, vec![(1, Stream::Stdout)], vec![]);
    let result = server.serve(
        &mut pipe,
        &host.cx,
        &host.observe(),
        &host.root,
        &mut chunks,
    );
    settle(&host, &runs);
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("session cleanup");
    host.assert_foreign();
    ledger.borrow().done();
    let frames = pipe.frames();
    assert!(
        result.is_err()
            || frames.iter().any(|frame| matches!(
                frame,
                Frame::Response(Response {
                    outcome: ResponseOutcome::Refused(Refusal {
                        code: RefusalCode::InvalidRequest,
                        ..
                    }),
                    ..
                })
            ))
            || frames
                .iter()
                .any(|f| matches!(f, Frame::Stream(s) if s.kind == StreamKind::Gap || s.seq >= 40)),
        "reconnect cursor was silently ignored: {frames:?}"
    );
    assert!(
        !frames
            .iter()
            .any(|f| matches!(f, Frame::Stream(s) if s.kind.is_data() && s.seq < 40)),
        "reconnect presented missing output as complete"
    );
    result.expect("retained reconnect serves its explicit gap");
    let output = data_frames(&frames);
    assert_eq!(output.len(), 2, "exactly a gap and its retained data");
    assert_eq!(output[0].gap_range(), Some(StreamGap { from: 40, to: 100 }));
    assert_eq!(output[0].seq, 40);
    assert_eq!(output[1].seq, 100);
    assert_eq!(
        decode_stream_payload(output[1].payload.as_deref().expect("payload")).expect("base64"),
        b"current"
    );
    chunks.terminal_mut().done();
}

#[test]
fn terminal_delivery_is_once_resize_is_latest_and_detach_buffers_nothing() {
    let host = Host::new();
    let (owner, session) = host.session();
    let ledger = Rc::new(RefCell::new(Ledger::default()));
    let runs = Rc::new(RefCell::new(vec![]));
    expect_admission(&ledger);
    let mut server = server(
        &host,
        session.session(),
        ledger.clone(),
        runs.clone(),
        session_operations(),
    );
    let sizes = [
        TerminalSize {
            rows: 24,
            columns: 80,
        },
        TerminalSize {
            rows: 50,
            columns: 120,
        },
    ];
    let mut streams = retained(
        &host,
        vec![(1, Stream::Stdout)],
        vec![
            TerminalEffect::Input(b"typed input".to_vec()),
            TerminalEffect::Resize(sizes[0]),
            TerminalEffect::Resize(sizes[1]),
        ],
    );
    let mut frames = attached(session.session());
    frames.push(Frame::Stream(input()));
    for (index, size) in sizes.into_iter().enumerate() {
        frames.push(Frame::Stream(StreamFrame {
            id: RequestId::new([2; 16]),
            stream_id: 0,
            seq: index as u64 + 1,
            kind: StreamKind::Resize,
            payload: None,
            rows: Some(size.rows),
            columns: Some(size.columns),
        }));
    }
    frames.push(Frame::Request(request(
        session.session(),
        3,
        RequestBody::Detach(RunBody { run: id(10) }),
    )));
    let mut late = input();
    late.seq = 3;
    frames.push(Frame::Stream(late));
    let mut pipe = Pipe::new(&frames);
    let result = server.serve(
        &mut pipe,
        &host.cx,
        &host.observe(),
        &host.root,
        &mut streams,
    );
    settle(&host, &runs);
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("cleanup");
    host.assert_foreign();
    ledger.borrow().done();
    assert!(
        matches!(
            result,
            Err(Refusal {
                code: RefusalCode::InvalidRequest,
                ..
            })
        ),
        "detached input must close before delivery: {result:?}"
    );
    refused(&pipe, RefusalCode::InvalidRequest);
    streams.terminal_mut().done();
    assert_eq!(streams.terminal_mut().latest_size, Some(sizes[1]));
}

#[test]
fn reconnect_resumes_each_retained_stream_and_released_attachment_cannot_read() {
    let host = Host::new();
    let (owner, session) = host.session();
    let ledger = Rc::new(RefCell::new(Ledger::default()));
    let runs = Rc::new(RefCell::new(vec![]));
    expect_admission(&ledger);
    let mut first = server(
        &host,
        session.session(),
        ledger.clone(),
        runs.clone(),
        session_operations(),
    );
    let mut frames = attached(session.session());
    frames.push(Frame::Request(request(
        session.session(),
        3,
        RequestBody::Detach(RunBody { run: id(10) }),
    )));
    first
        .serve(
            &mut Pipe::new(&frames),
            &host.cx,
            &host.observe(),
            &host.root,
            &mut NoStreams,
        )
        .expect("detached owner");
    write_segment(&host, Stream::Stdout, 0, b"consumedstdout");
    write_segment(&host, Stream::Stderr, 0, b"oldstderr");
    let plan = vec![(1, Stream::Stdout), (1, Stream::Stderr)];
    let mut streams = retained(&host, plan.clone(), vec![]);
    let positions = BTreeMap::from([(0, 8), (1, 3)]);
    assert_eq!(
        streams.resume(
            id(10),
            &Cursors {
                primary: 8,
                streams: positions.clone()
            }
        ),
        Some(positions.clone())
    );
    // An unreported secondary cursor must not silently restart that stream.
    assert_eq!(
        streams.resume(
            id(10),
            &Cursors {
                primary: 8,
                streams: BTreeMap::new()
            }
        ),
        None
    );
    let mut second = server(
        &host,
        session.session(),
        ledger.clone(),
        runs.clone(),
        session_operations(),
    );
    let mut frames = authenticated(session.session());
    frames.push(Frame::Request(request(
        session.session(),
        4,
        RequestBody::Attach(AttachBody {
            run: id(10),
            streams: vec![StreamKind::Stdout, StreamKind::Stderr],
            cursor: Some(8),
            cursors: Some(positions),
        }),
    )));
    let mut pipe = Pipe::new(&frames);
    second
        .serve(
            &mut pipe,
            &host.cx,
            &host.observe(),
            &host.root,
            &mut streams,
        )
        .expect("retained reconnect");
    let frames = pipe.frames();
    let output = data_frames(&frames);
    assert_eq!(output.len(), 2);
    for (frame, (stream, seq, bytes)) in output
        .iter()
        .zip([(0, 8, b"stdout".as_slice()), (1, 3, b"stderr".as_slice())])
    {
        assert_eq!(
            (frame.id, frame.stream_id, frame.seq),
            (RequestId::new([4; 16]), stream, seq)
        );
        assert_eq!(
            decode_stream_payload(frame.payload.as_deref().expect("payload")).expect("base64"),
            bytes
        );
    }
    assert_eq!(
        host.stops(id(10)),
        0,
        "secondary EOF must not stop detached owner"
    );
    // Holding an Attachment after its release is not read or terminal authority.
    let mut attachment = run::attach_secondary(
        &host.cx,
        &host.root,
        session.session(),
        id(10),
        run::AttachmentId::new([56; 16]),
    )
    .expect("secondary");
    let mut fenced = retained(&host, plan, vec![]);
    let start = fenced
        .resume(id(10), &Cursors::fresh())
        .expect("fresh reader");
    fenced.start(&attachment, &start);
    attachment
        .release(&host.cx, &host.root, Release::Detach)
        .expect("release");
    assert!(
        fenced.next_chunk(Some(&attachment)).is_none(),
        "released attachment published retained bytes"
    );
    assert!(
        fenced.input(&attachment, b"late").is_err(),
        "released terminal input was buffered"
    );
    assert!(
        fenced
            .resize(
                &attachment,
                TerminalSize {
                    rows: 60,
                    columns: 140
                }
            )
            .is_err(),
        "released resize was buffered"
    );
    fenced.terminal_mut().done();
    streams.terminal_mut().done();
    settle(&host, &runs);
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("cleanup");
    host.assert_foreign();
    ledger.borrow().done();
}

#[test]
fn same_id_without_a_committed_admission_is_unknown_not_reusable() {
    let host = Host::new();
    let (owner, session) = host.session();
    let key = run::AdmissionKey {
        session: session.session(),
        request: RequestId::new([1; 16]),
    };
    let digest = Digest::of(b"uncommitted");
    assert!(matches!(
        run::reserve_resolved(
            &host.cx,
            &host.root,
            key,
            digest.clone(),
            id(20),
            &resolved("child")
        )
        .expect("reservation"),
        run::Reservation::Fresh(_)
    ));
    assert!(matches!(
        run::lookup(&host.observe(), &host.root, key, digest),
        effigy_kernel::observe::Observed::Verified(run::Recovered::Unknown { .. }, _)
    ));
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("cleanup");
    host.assert_foreign();
}
