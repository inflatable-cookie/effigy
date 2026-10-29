use super::support::*;
use effigy_kernel::id::Digest;
use effigy_kernel::run::{self, Client, Release, RequestId};
use effigy_kernel::workspace_channel::*;
use std::cell::RefCell;
use std::collections::VecDeque;
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
        }),
    )));
    frames
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

fn invalid_input(mut inputs: Vec<StreamFrame>) {
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
    let result = server.serve(
        &mut pipe,
        &host.cx,
        &host.observe(),
        &host.root,
        &mut NoStreams,
    );
    // Clean up before asserting a channel defect, including all foreign guards.
    settle(&host, &runs);
    owner
        .settle(&cleanup(&host.cx), &mut NoEffects)
        .expect("session cleanup");
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
        "stream identity/sequence must refuse before dispatch, got {result:?}"
    );
    refused(&pipe, RefusalCode::InvalidRequest);
}

#[test]
fn terminal_input_for_unknown_stream_is_refused() {
    let mut input = input();
    input.stream_id = 999;
    invalid_input(vec![input]);
}
#[test]
fn terminal_input_for_another_request_is_refused() {
    let mut input = input();
    input.id = RequestId::new([99; 16]);
    invalid_input(vec![input]);
}
#[test]
fn repeated_terminal_input_is_not_replayed() {
    invalid_input(vec![input(), input()]);
}
#[test]
fn out_of_order_terminal_input_is_refused() {
    let mut ahead = input();
    ahead.seq = 2;
    invalid_input(vec![input(), ahead]);
}

struct Chunks(VecDeque<StreamChunk>);
impl Streams for Chunks {
    fn next_chunk(&mut self) -> Option<StreamChunk> {
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
    let mut chunks = Chunks(
        (0..20)
            .map(|_| StreamChunk {
                stream_id: 0,
                kind: StreamKind::TerminalOutput,
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
    assert!(stops >= 1, "full stream window suppressed cancellation");
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
    let mut frames = authenticated(session.session());
    frames.push(Frame::Request(request(
        session.session(),
        4,
        RequestBody::Attach(AttachBody {
            run: id(10),
            streams: vec![StreamKind::TerminalOutput],
            cursor: Some(40),
        }),
    )));
    let mut pipe = Pipe::new(&frames);
    let mut chunks = Chunks(
        [StreamChunk {
            stream_id: 0,
            kind: StreamKind::TerminalOutput,
            bytes: b"current".to_vec(),
        }]
        .into(),
    );
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
        run::reserve(&host.cx, &host.root, key, digest.clone(), id(20)).expect("reservation"),
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
