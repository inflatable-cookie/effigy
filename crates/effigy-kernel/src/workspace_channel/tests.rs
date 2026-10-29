//! §8.11 lane A proofs: the wire, the handshake, the grant and the replay
//! ordering, against a scripted transport, strict fakes for the host-engine
//! seams and the kernel's own reservation records.
use super::*;
use crate::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, Limits, ObserveContext, OperationContext,
};
use crate::gate::Wait;
use crate::id::{
    CheckoutId, CheckoutKind, ExecutionTarget, InvocationSpec, ResourceId, Route, Surface, TaskId,
};
use crate::observe::{AbsenceProof, Observed};
use crate::outcome::Invocation;
use crate::root::StateRoot;
use crate::run::{BeginOpts, Effect, Reconciled, Run, SettlementAdapter};
use crate::store::Store;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::time::{Duration, Instant};

const SESSION: u8 = 200;
const SESSION_GENERATION: u64 = 1;

fn effects(placement: Placement) -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(60)),
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
        placement,
    )
    .effects()
    .expect("an effect context")
}
fn observe_context() -> ObserveContext {
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
    .expect("an observation context")
}
fn cleanup(cx: &EffectContext) -> CleanupContext {
    cx.operation()
        .cleanup(Duration::from_secs(10))
        .expect("cleanup")
}
fn checkout() -> CheckoutId {
    CheckoutId::new(CheckoutKind::EphemeralClone, [7; 16])
}
fn task(name: &str) -> TaskId {
    TaskId::new(checkout(), "", Surface::Published, name, None).expect("task id")
}
fn invocation(owner: ResourceId) -> ResolvedInvocation {
    ResolvedInvocation::new(InvocationSpec {
        task: task("workspace-channel"),
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner,
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .expect("resolved invocation")
}
fn next_argv() -> Invocation {
    Invocation::validated(vec![OsString::from_vec(b"run".to_vec())], |args| {
        args == ["run"]
    })
    .expect("invocation")
}
/// No effect was planned, so settle finds nothing to undo.
#[derive(Debug)]
struct NoEffects;
impl SettlementAdapter for NoEffects {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<crate::run::Fence> {
        Observed::Absent(AbsenceProof::NoGateInode)
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Absent(AbsenceProof::NoGateInode)
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        Observed::Absent(AbsenceProof::NoGateInode)
    }
}
/// One journal with one admitted workspace session run, and its own generation.
fn journal(
    name: &str,
) -> (
    effigy_test_support::Fixture,
    EffectContext,
    StateRoot,
    Session,
) {
    let fixture = effigy_test_support::Fixture::new(name).expect("fixture");
    let cx = effects(Placement::Host);
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let owner = task("session").resource();
    let run = Run::begin(
        &cx,
        &root,
        &invocation(owner),
        BeginOpts {
            id: RunId::new(1, [SESSION; 10]).expect("run id"),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next_argv(),
            custody: None,
        },
        &cleanup(&cx),
        &mut NoEffects,
    )
    .expect("session run");
    let generation = Generation::new(SESSION_GENERATION);
    drop(run);
    let session = Session {
        run: RunId::new(1, [SESSION; 10]).expect("run id"),
        generation,
    };
    (fixture, cx, root, session)
}

/// A scripted byte transport: it reads at most `chunk` bytes per call, so a
/// partial read is the normal case, and records everything the host writes.
#[derive(Debug)]
struct Pipe {
    input: Vec<u8>,
    cursor: usize,
    written: Vec<u8>,
    peer: Option<ServerIdentity>,
    chunk: usize,
}
impl Pipe {
    fn new(input: Vec<u8>, peer: Option<ServerIdentity>) -> Self {
        Self {
            input,
            cursor: 0,
            written: Vec::new(),
            peer,
            chunk: 3,
        }
    }
}
impl Transport for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.input.len() - self.cursor;
        if left == 0 {
            return Ok(0);
        }
        let take = left.min(buf.len()).min(self.chunk);
        buf[..take].copy_from_slice(&self.input[self.cursor..self.cursor + take]);
        self.cursor += take;
        Ok(take)
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.written.extend_from_slice(bytes);
        Ok(())
    }
    fn verified_peer(&self) -> Option<ServerIdentity> {
        self.peer.clone()
    }
}
/// Client frames as a byte stream.
fn stream(frames: &[Frame]) -> Vec<u8> {
    let mut out = Vec::new();
    for frame in frames {
        let body = encode_frame(frame).expect("encoded frame");
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&body);
    }
    out
}
/// Every frame the host wrote, decoded in order.
fn written_frames(pipe: &Pipe) -> Vec<Frame> {
    let mut frames = Vec::new();
    let mut cursor = 0;
    while cursor + 4 <= pipe.written.len() {
        let mut prefix = [0u8; 4];
        prefix.copy_from_slice(&pipe.written[cursor..cursor + 4]);
        let length = u32::from_be_bytes(prefix) as usize;
        cursor += 4;
        assert!(cursor + length <= pipe.written.len(), "a truncated frame");
        frames.push(decode_frame(&pipe.written[cursor..cursor + length]).expect("valid frame"));
        cursor += length;
    }
    assert_eq!(cursor, pipe.written.len(), "trailing bytes on the wire");
    frames
}

#[derive(Debug)]
struct Ids(VecDeque<RunId>);
impl Ids {
    fn new(runs: &[u8]) -> Self {
        Self(
            runs.iter()
                .map(|n| RunId::new(7, [*n; 10]).expect("run id"))
                .collect(),
        )
    }
}
impl RunIds for Ids {
    fn next(&mut self) -> Result<RunId, Refusal> {
        self.0
            .pop_front()
            .ok_or_else(|| Refusal::unavailable("the test ran out of run ids"))
    }
}
#[derive(Debug)]
struct Resolver {
    calls: usize,
    owner: ResourceId,
    fail: bool,
}
impl Resolver {
    fn new(owner: ResourceId) -> Self {
        Self {
            calls: 0,
            owner,
            fail: false,
        }
    }
}
impl super::Resolver for Resolver {
    fn resolve(
        &mut self,
        _: &EffectContext,
        _: &InvocationBody,
    ) -> Result<ResolvedInvocation, Refusal> {
        self.calls += 1;
        if self.fail {
            return Err(Refusal::forbidden(
                "the host does not resolve this selector",
            ));
        }
        Ok(invocation(self.owner.clone()))
    }
}
#[derive(Debug, Default)]
struct Admitter {
    tickets: Vec<AdmissionTicket>,
    refuse: Option<Refusal>,
}
impl Admission for Admitter {
    fn admit(
        &mut self,
        _: &EffectContext,
        ticket: &AdmissionTicket,
        _: &ResolvedInvocation,
    ) -> Result<(), Refusal> {
        if let Some(refusal) = &self.refuse {
            return Err(refusal.clone());
        }
        self.tickets.push(ticket.clone());
        Ok(())
    }
}
#[derive(Debug, Default)]
struct Observer {
    calls: usize,
}
impl super::Observer for Observer {
    fn observe(&mut self, _: &ObserveContext, body: &ObserveBody) -> Result<Value, Refusal> {
        self.calls += 1;
        Ok(serde_json::json!({ "op": body.op }))
    }
}

fn pinned() -> Digest {
    Digest::of(b"pinned server identity")
}
fn host() -> Negotiation {
    Negotiation {
        context_generation: Generation::new(3),
        session_run_id: RunId::new(1, [SESSION; 10]).expect("run id"),
        artifact: HostArtifact {
            effigy_version: "0.15.0".into(),
            binary_digest: Digest::of(b"guest artifact"),
        },
    }
}
fn hello() -> Hello {
    Hello {
        schema: SCHEMA.into(),
        protocol_versions: vec![PROTOCOL_V1],
        effigy_version: "0.15.0".into(),
        binary_digest: Digest::of(b"guest artifact"),
        context_generation: Generation::new(3),
        session_run_id: RunId::new(1, [SESSION; 10]).expect("run id"),
    }
}
fn fence(session: Session) -> SessionFence {
    SessionFence {
        context_generation: Generation::new(3),
        environment_generation: Generation::new(4),
        session,
        container_id: "c0ffee".into(),
        vm_boot_id: "boot-1".into(),
    }
}
fn binding(session: Session) -> GrantBinding {
    GrantBinding {
        host_user: "operator".into(),
        checkout: "checkout".into(),
        session,
        environment_generation: Generation::new(4),
        container_id: "c0ffee".into(),
        vm_boot_id: "boot-1".into(),
    }
}
fn capability() -> Secret {
    Secret::new([0xab; 32])
}
fn request(session: Session, body: RequestBody, id: u8) -> Request {
    Request {
        id: RequestId::new([id; 16]),
        context_generation: Generation::new(3),
        environment_generation: Generation::new(4),
        session_run_id: session.run,
        container_id: "c0ffee".into(),
        vm_boot_id: "boot-1".into(),
        body,
    }
}
fn invocation_body(selector: &str) -> InvocationBody {
    InvocationBody {
        selector: Selector::Task(selector.into()),
        cwd: "packages/app".into(),
        target: Target::Host,
        args: vec!["--plan".into()],
        output: OutputMode::Json,
    }
}
fn server(session: Session, ids: &[u8]) -> Server<Admitter, Resolver, Observer, Ids> {
    let mut grants = GrantBook::new();
    grants.issue(
        capability(),
        binding(session),
        session_operations(),
        [Target::Host].into_iter().collect(),
    );
    Server::new(
        host(),
        fence(session),
        pinned(),
        grants,
        Dispatcher::new(
            Admitter::default(),
            Resolver::new(task("workspace-channel").resource()),
            Observer::default(),
            Ids::new(ids),
        ),
    )
}
fn peer() -> Option<ServerIdentity> {
    Some(ServerIdentity { digest: pinned() })
}

/// A run admitted through the kernel's own reservation, so attachment and
/// control proofs act on a live owner.
fn admitted_run<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    session: Session,
    request: u8,
    ids: &[u8],
) -> (Run<'a>, RunId) {
    // Its own owner gate, so two admitted runs in one proof do not contend.
    let owner = task(&format!("workspace-channel-{request}")).resource();
    let mut dispatcher = Dispatcher::new(
        Admitter::default(),
        Resolver::new(owner.clone()),
        Observer::default(),
        Ids::new(ids),
    );
    let id = RequestId::new([request; 16]);
    let AdmissionReply::Accepted(accepted) = dispatcher
        .invoke(
            cx,
            &observe_context(),
            root,
            session,
            id,
            Digest::of(b"invite"),
            &invocation_body("build"),
        )
        .expect("reserved")
    else {
        panic!("a fresh admission is accepted");
    };
    let ticket = dispatcher.admission.tickets[0].clone();
    let run = Run::begin_admitted(
        cx,
        root,
        &invocation(owner),
        BeginOpts {
            id: ticket.run(),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next_argv(),
            custody: None,
        },
        &cleanup(cx),
        &mut NoEffects,
        ticket,
    )
    .expect("admitted");
    (run, accepted.run)
}

/// One connection's captured output, as the attachment's owner would publish it.
#[derive(Debug)]
struct ScriptedStreams {
    chunks: VecDeque<StreamChunk>,
}
impl ScriptedStreams {
    fn frames(count: usize) -> Self {
        Self {
            chunks: (0..count)
                .map(|_| StreamChunk {
                    stream_id: 1,
                    kind: StreamKind::Stdout,
                    bytes: vec![0x41; MAX_STREAM_PAYLOAD],
                })
                .collect(),
        }
    }
    fn remaining(&self) -> usize {
        self.chunks.len()
    }
}
impl Streams for ScriptedStreams {
    fn next_chunk(&mut self) -> Option<StreamChunk> {
        self.chunks.pop_front()
    }
}

#[test]
fn a_frame_round_trips_through_its_length_prefix() {
    let hello = hello();
    let body = encode_frame(&Frame::Hello(hello.clone())).expect("encoded");
    assert_eq!(decode_frame(&body).expect("decoded"), Frame::Hello(hello));
}

#[test]
fn malformed_oversized_duplicate_key_and_unknown_frames_refuse() {
    let bad = [
        br"not json".to_vec(),
        br#"{"frame":"hello","frame":"refuse"}"#.to_vec(),
        br#"{"frame":"nonsense"}"#.to_vec(),
        br#"{"frame":"request","id":"00","method":"invoke","body":{}}"#.to_vec(),
        br#"{"frame":"hello","schema":"effigy.workspace-channel.v1","protocol_versions":[1],"effigy_version":"0.15.0","binary_digest":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"context_generation":3,"session_run_id":[0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,200,0,0,0,0,0,0,0,0,0,0,0,0,0],"executable":"/bin/sh"}"#.to_vec(),
        // A repeated key nested inside a body is refused just the same.
        format!(
            r#"{{"frame":"request","id":"{}","method":"invoke","context_generation":3,"environment_generation":4,"session_run_id":[0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,200,0,0,0,0,0,0,0,0,0,0,0,0,0],"container_id":"c","vm_boot_id":"b","body":{{"selector":{{"task":"x"}},"cwd":"a","target":"host","args":[],"output":"text","args":[]}}}}"#,
            RequestId::new([1; 16])
        )
        .into_bytes(),
    ];
    for body in bad {
        let refusal = decode_frame(&body).expect_err("a bad frame refuses");
        assert_eq!(refusal.code, RefusalCode::InvalidRequest, "{body:?}");
    }
    // A body at the frame limit is refused before it is parsed.
    let mut oversized = vec![b'{'; MAX_FRAME_BYTES + 1];
    oversized[0] = b'{';
    assert_eq!(
        decode_frame(&oversized).expect_err("oversized").code,
        RefusalCode::InvalidRequest
    );
}

#[test]
fn a_declared_length_over_the_limit_is_refused_before_its_body_is_read() {
    let mut pipe = Pipe::new(
        ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes().to_vec(),
        peer(),
    );
    let refusal = read_frame(&mut pipe).expect_err("an oversized frame refuses");
    assert_eq!(refusal.code, RefusalCode::InvalidRequest);
    // Nothing of the body was allocated or consumed.
    assert_eq!(pipe.cursor, 4);
    // A body that ends early is a refusal, not a partial frame.
    let mut truncated = Pipe::new(16u32.to_be_bytes().to_vec(), peer());
    truncated.input.extend_from_slice(b"{\"fra");
    assert_eq!(
        read_frame(&mut truncated).expect_err("truncated").code,
        RefusalCode::InvalidRequest
    );
}

#[test]
fn pin_mismatch_refuses_before_a_credential_is_disclosed() {
    let (_fixture, cx, root, session) = journal("effigy-channel-pin");
    let mut host = server(session, &[1]);
    let other = Some(ServerIdentity {
        digest: Digest::of(b"another host"),
    });
    let mut pipe = Pipe::new(stream(&[Frame::Authenticate(capability())]), other);
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect_err("a pin mismatch refuses");
    assert_eq!(refusal.code, RefusalCode::IdentityMismatch);
    let frames = written_frames(&pipe);
    assert_eq!(frames.len(), 1);
    assert!(matches!(&frames[0], Frame::Refuse(r) if r.code == RefusalCode::IdentityMismatch));
    // An unverified connection is not a pin either.
    let mut anonymous = Pipe::new(Vec::new(), None);
    assert_eq!(
        host.serve(
            &mut anonymous,
            &cx,
            &observe_context(),
            &root,
            &mut NoStreams
        )
        .expect_err("an unverified peer refuses")
        .code,
        RefusalCode::IdentityMismatch
    );
    assert!(written_frames(&anonymous)
        .iter()
        .all(|frame| matches!(frame, Frame::Refuse(_))));
}

#[test]
fn version_and_digest_mismatch_refuse_before_authentication() {
    let (_fixture, cx, root, session) = journal("effigy-channel-version");
    for broken in [
        Hello {
            protocol_versions: vec![2],
            ..hello()
        },
        Hello {
            effigy_version: "0.14.1".into(),
            ..hello()
        },
        Hello {
            binary_digest: Digest::of(b"another guest"),
            ..hello()
        },
        Hello {
            context_generation: Generation::new(2),
            ..hello()
        },
        Hello {
            session_run_id: RunId::new(1, [99; 10]).expect("run id"),
            ..hello()
        },
        Hello {
            schema: "effigy.workspace-channel.v0".into(),
            ..hello()
        },
    ] {
        let mut host = server(session, &[1]);
        let mut pipe = Pipe::new(
            stream(&[Frame::Hello(broken), Frame::Authenticate(capability())]),
            peer(),
        );
        let refusal = host
            .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
            .expect_err("a mismatch refuses");
        assert!(matches!(
            refusal.code,
            RefusalCode::VersionMismatch
                | RefusalCode::StaleContext
                | RefusalCode::IdentityMismatch
        ));
        let frames = written_frames(&pipe);
        assert_eq!(frames.len(), 1, "{frames:?}");
        assert!(matches!(frames[0], Frame::Refuse(_)));
        assert!(!pipe.written.windows(4).any(|w| w == b"auth"));
    }
}

#[test]
fn no_request_crosses_before_authentication() {
    let (_fixture, cx, root, session) = journal("effigy-channel-preauth");
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Request(request(
                session,
                RequestBody::Observe(ObserveBody {
                    op: "run:status".into(),
                    target: None,
                }),
                1,
            )),
        ]),
        peer(),
    );
    let mut host = server(session, &[1]);
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect_err("an unauthenticated request refuses");
    assert_eq!(refusal.code, RefusalCode::Unauthenticated);
    assert_eq!(host.dispatcher.observer.calls, 0);
    let frames = written_frames(&pipe);
    assert!(matches!(frames[0], Frame::HelloOk(_)));
    assert!(matches!(&frames[1], Frame::Refuse(r) if r.code == RefusalCode::Unauthenticated));
}

#[test]
fn an_unknown_capability_refuses_without_dispatch() {
    let (_fixture, cx, root, session) = journal("effigy-channel-capability");
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(Secret::new([0x11; 32])),
        ]),
        peer(),
    );
    let mut host = server(session, &[1]);
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect_err("an unissued capability refuses");
    assert_eq!(refusal.code, RefusalCode::Unauthenticated);
    assert_eq!(host.dispatcher.observer.calls, 0);
    assert_eq!(host.dispatcher.resolver.calls, 0);
}

#[test]
fn no_credential_reaches_a_formatted_value_a_log_or_a_refusal() {
    let secret = Secret::new([0x5a; 32]);
    let text = hex_text(secret.bytes());
    for rendered in [
        format!("{secret:?}"),
        format!("{secret}"),
        format!(
            "{:?}",
            EnrolRequest {
                credential: secret.clone(),
                capability: [0x33; 32],
                session_run_id: RunId::new(1, [SESSION; 10]).expect("run id"),
                session_generation: Generation::new(1),
                environment_generation: Generation::new(4),
                container_id: "c0ffee".into(),
                vm_boot_id: "boot-1".into(),
            }
        ),
        format!(
            "{:?}",
            Refusal::unauthenticated("this capability was not issued")
        ),
    ] {
        assert!(!rendered.contains(&text), "{rendered}");
        assert!(!rendered.contains(&text[..16]), "{rendered}");
    }
    // The frame carries it, and the frame is the only place it crosses.
    let body = encode_frame(&Frame::Authenticate(secret.clone())).expect("encoded");
    assert!(String::from_utf8_lossy(&body).contains(&text));
    // A refusal never echoes the frame that carried it.
    let refusal = Refusal::unauthenticated("this capability was not issued by this session");
    let encoded = serde_json::to_string(&refusal).expect("encoded refusal");
    assert!(!encoded.contains(&text));
    assert!(encoded.len() < MAX_DIAGNOSTIC_BYTES + 128);
}

#[test]
fn observe_writes_nothing_and_retries_re_observe() {
    let (_fixture, cx, root, session) = journal("effigy-channel-observe");
    let mut grants = GrantBook::new();
    grants.issue(
        capability(),
        binding(session),
        session_operations(),
        [Target::Host].into_iter().collect(),
    );
    let mut host = Server::new(
        host(),
        fence(session),
        pinned(),
        grants,
        Dispatcher::new(
            Admitter::default(),
            Resolver::new(task("workspace-channel").resource()),
            Observer::default(),
            Ids::new(&[1]),
        ),
    );
    let observe = |id: u8| {
        Frame::Request(request(
            session,
            RequestBody::Observe(ObserveBody {
                op: "run:status".into(),
                target: None,
            }),
            id,
        ))
    };
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            observe(1),
            observe(2),
        ]),
        peer(),
    );
    let served = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect("served");
    assert_eq!(served.responses, 2);
    assert_eq!(host.dispatcher.observer.calls, 2);
    // Observe reserved nothing: the kernel's record table has no row for it.
    let reader = match Store::read(&observe_context(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("root {other:?}"),
    };
    let rows: i64 = reader
        .conn()
        .query_row("SELECT count(*) FROM workspace_admissions", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(rows, 0);
}

#[test]
fn a_traversing_or_privileged_body_refuses_before_admission() {
    let (_fixture, cx, root, session) = journal("effigy-channel-body");
    let mut host = server(session, &[1, 2]);
    let mut refusal = |body: InvocationBody| {
        host.dispatcher
            .invoke(
                &cx,
                &observe_context(),
                &root,
                session,
                RequestId::new([1; 16]),
                Digest::of(b"d"),
                &body,
            )
            .expect_err("refused")
    };
    let mut traversal = invocation_body("build");
    traversal.cwd = "../outside".into();
    assert_eq!(refusal(traversal).code, RefusalCode::InvalidRequest);
    let mut absolute = invocation_body("build");
    absolute.cwd = "/etc".into();
    assert_eq!(refusal(absolute).code, RefusalCode::InvalidRequest);
    let mut too_many = invocation_body("build");
    too_many.args = (0..257).map(|n| format!("a{n}")).collect();
    assert_eq!(refusal(too_many).code, RefusalCode::InvalidRequest);
    let mut long_argument = invocation_body("build");
    long_argument.args = vec!["x".repeat(MAX_PATH_BYTES + 1)];
    assert_eq!(refusal(long_argument).code, RefusalCode::InvalidRequest);
    assert_eq!(host.dispatcher.resolver.calls, 0);
    assert!(host.dispatcher.admission.tickets.is_empty());

    // An invocation body cannot carry the fields a guest must never choose.
    for field in [
        "\"executable\":\"/bin/sh\"",
        "\"state_root\":\"/tmp\"",
        "\"entry_point\":\"__kernel\"",
    ] {
        let body = format!(
            r#"{{"frame":"request","id":"{}","method":"invoke","context_generation":3,"environment_generation":4,"session_run_id":[0,0,0,0,0,0,0,1,0,0,0,0,0,0,0,0,0,0,200,0,0,0,0,0,0,0,0,0,0,0,0,0],"container_id":"c","vm_boot_id":"b","body":{{"selector":{{"task":"build"}},"cwd":"a","target":"host","args":[],"output":"text",{field}}}}}"#,
            RequestId::new([1; 16])
        );
        assert_eq!(
            decode_frame(body.as_bytes())
                .expect_err("a privileged field refuses")
                .code,
            RefusalCode::InvalidRequest
        );
    }
}

#[test]
fn a_grant_scopes_the_operation_the_target_and_its_generation() {
    let session = Session {
        run: RunId::new(1, [SESSION; 10]).expect("run id"),
        generation: Generation::new(SESSION_GENERATION),
    };
    let mut grants = GrantBook::new();
    grants.issue(
        capability(),
        binding(session),
        [Method::Observe, Method::Invoke].into_iter().collect(),
        [Target::Host].into_iter().collect(),
    );
    let grant = grants
        .authenticate(&capability())
        .expect("authenticated")
        .clone();
    assert!(grant.authorise(Method::Observe, None).is_ok());
    assert!(grant.authorise(Method::Invoke, Some(&Target::Host)).is_ok());
    assert_eq!(
        grant
            .authorise(Method::Cancel, None)
            .expect_err("narrowed")
            .code,
        RefusalCode::Forbidden
    );
    assert_eq!(
        grant
            .authorise(
                Method::Invoke,
                Some(&Target::Container {
                    service: "api".into()
                })
            )
            .expect_err("another target")
            .code,
        RefusalCode::Forbidden
    );
    // A stale generation is refused by the fence, and a revoked grant grants
    // nothing at all.
    assert_eq!(
        fence(session)
            .verify(&Request {
                environment_generation: Generation::new(5),
                ..request(
                    session,
                    RequestBody::Observe(ObserveBody {
                        op: "run:status".into(),
                        target: None,
                    }),
                    1
                )
            })
            .expect_err("a stale environment generation")
            .code,
        RefusalCode::StaleContext
    );
    grants.revoke_generation(session.run, session.generation);
    assert_eq!(
        grants
            .authenticate(&capability())
            .expect_err("revoked")
            .code,
        RefusalCode::Unauthenticated
    );
}

#[test]
fn a_fresh_invocation_reserves_once_and_a_retry_never_readmits() {
    let (_fixture, cx, root, session) = journal("effigy-channel-replay");
    let owner = task("workspace-channel").resource();
    let mut dispatcher = Dispatcher::new(
        Admitter::default(),
        Resolver::new(owner.clone()),
        Observer::default(),
        Ids::new(&[11, 12, 14]),
    );
    let id = RequestId::new([4; 16]);
    let digest = Digest::of(b"invite");
    // The first attempt reserves and hands exactly that ticket to the engine.
    let reply = dispatcher
        .invoke(
            &cx,
            &observe_context(),
            &root,
            session,
            id,
            digest.clone(),
            &invocation_body("build"),
        )
        .expect("admitted");
    let AdmissionReply::Accepted(accepted) = reply else {
        panic!("a fresh admission is accepted");
    };
    assert_eq!(dispatcher.admission.tickets.len(), 1);
    assert_eq!(accepted.run, dispatcher.admission.tickets[0].run());
    assert_eq!(accepted.placement, Placement::Host);

    // The same id and digest after a reconnect, and after a restart, reads the
    // kernel's recorded reservation: the run was never admitted, so the answer
    // is `unknown` and no second run is launched.
    let mut restarted = Dispatcher::new(
        Admitter::default(),
        Resolver::new(owner.clone()),
        Observer::default(),
        Ids::new(&[13]),
    );
    for _ in 0..2 {
        let refusal = restarted
            .invoke(
                &cx,
                &observe_context(),
                &root,
                session,
                id,
                digest.clone(),
                &invocation_body("build"),
            )
            .expect_err("a reservation with no admission is unknown");
        assert_eq!(refusal.code, RefusalCode::Unknown);
        assert!(restarted.admission.tickets.is_empty());
    }
    // A different digest for the same id is a conflict, decided before the
    // resolver or the engine sees it.
    let refusal = restarted
        .invoke(
            &cx,
            &observe_context(),
            &root,
            session,
            id,
            Digest::of(b"another intent"),
            &invocation_body("test"),
        )
        .expect_err("a changed digest conflicts");
    assert_eq!(refusal.code, RefusalCode::IdConflict);
    assert!(restarted.admission.tickets.is_empty());
    assert_eq!(restarted.resolver.calls, 0);
}

#[test]
fn a_recorded_admission_and_outcome_answer_retries_across_a_restart() {
    let (_fixture, cx, root, session) = journal("effigy-channel-recorded");
    let owner = task("workspace-channel").resource();
    let mut dispatcher = Dispatcher::new(
        Admitter::default(),
        Resolver::new(owner.clone()),
        Observer::default(),
        Ids::new(&[81, 82]),
    );
    let id = RequestId::new([10; 16]);
    let digest = Digest::of(b"invite");
    let body = invocation_body("build");
    let reply = dispatcher
        .invoke(
            &cx,
            &observe_context(),
            &root,
            session,
            id,
            digest.clone(),
            &body,
        )
        .expect("reserved");
    let AdmissionReply::Accepted(reserved) = reply else {
        panic!("a fresh admission is accepted");
    };
    let ticket = dispatcher.admission.tickets[0].clone();
    assert_eq!(ticket.run(), reserved.run);

    // The engine admits exactly the run the reservation named.
    let mut admitted = Run::begin_admitted(
        &cx,
        &root,
        &invocation(owner.clone()),
        BeginOpts {
            id: ticket.run(),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next_argv(),
            custody: None,
        },
        &cleanup(&cx),
        &mut NoEffects,
        ticket,
    )
    .expect("admitted");

    // A retry while the run is live recovers the recorded admission, and the
    // engine is never asked to admit again.
    let mut restarted = Dispatcher::new(
        Admitter::default(),
        Resolver::new(owner.clone()),
        Observer::default(),
        Ids::new(&[83]),
    );
    let recovered = restarted
        .invoke(
            &cx,
            &observe_context(),
            &root,
            session,
            id,
            digest.clone(),
            &body,
        )
        .expect("recovered");
    assert_eq!(recovered, AdmissionReply::Accepted(reserved));
    assert!(restarted.admission.tickets.is_empty());

    // Once the run closes, its recorded outcome answers the same key.
    admitted
        .finish(crate::outcome::WorkloadOutcome::Succeeded)
        .expect("finish");
    admitted
        .settle(&cleanup(&cx), &mut NoEffects)
        .expect("settle");
    let mut after = Dispatcher::new(
        Admitter::default(),
        Resolver::new(owner.clone()),
        Observer::default(),
        Ids::new(&[84]),
    );
    let AdmissionReply::Completed(completed) = after
        .invoke(
            &cx,
            &observe_context(),
            &root,
            session,
            id,
            digest.clone(),
            &body,
        )
        .expect("completed")
    else {
        panic!("a closed run's recorded outcome is completed");
    };
    assert!(completed.outcome.to_string().contains("Succeeded"));
    assert_eq!(completed.exit, ExitStatus::UNKNOWN);
    assert!(after.admission.tickets.is_empty());
    assert_eq!(
        after
            .invoke(
                &cx,
                &observe_context(),
                &root,
                session,
                id,
                Digest::of(b"another intent"),
                &body,
            )
            .expect_err("a changed digest conflicts")
            .code,
        RefusalCode::IdConflict
    );
    // One key, one run: the replay never admitted a second one.
    let reader = match Store::read(&observe_context(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("root {other:?}"),
    };
    let runs: i64 = reader
        .conn()
        .query_row(
            "SELECT count(*) FROM runs WHERE owner=?1",
            [owner.name().to_string()],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(runs, 1);
}

#[test]
fn an_admission_refusal_keeps_the_reservation_so_a_retry_cannot_readmit() {
    let (_fixture, cx, root, session) = journal("effigy-channel-refused");
    let owner = task("workspace-channel").resource();
    let admitter = Admitter {
        refuse: Some(Refusal::forbidden("the host declined this admission")),
        ..Admitter::default()
    };
    let mut dispatcher = Dispatcher::new(
        admitter,
        Resolver::new(owner.clone()),
        Observer::default(),
        Ids::new(&[21, 22]),
    );
    let id = RequestId::new([5; 16]);
    let digest = Digest::of(b"invite");
    assert_eq!(
        dispatcher
            .invoke(
                &cx,
                &observe_context(),
                &root,
                session,
                id,
                digest.clone(),
                &invocation_body("build"),
            )
            .expect_err("refused")
            .code,
        RefusalCode::Forbidden
    );
    // The reservation survives the refusal, so the retry is `unknown`, not a
    // second attempt.
    dispatcher.admission.refuse = None;
    let refusal = dispatcher
        .invoke(
            &cx,
            &observe_context(),
            &root,
            session,
            id,
            digest.clone(),
            &invocation_body("build"),
        )
        .expect_err("unknown");
    assert_eq!(refusal.code, RefusalCode::Unknown);
    assert!(dispatcher.admission.tickets.is_empty());
}

#[test]
fn an_unresolvable_selector_records_nothing_so_a_retry_is_judged_afresh() {
    let (_fixture, cx, root, session) = journal("effigy-channel-unresolved");
    let owner = task("workspace-channel").resource();
    let mut resolver = Resolver::new(owner);
    resolver.fail = true;
    let mut dispatcher = Dispatcher::new(
        Admitter::default(),
        resolver,
        Observer::default(),
        Ids::new(&[31, 32]),
    );
    let id = RequestId::new([6; 16]);
    let digest = Digest::of(b"invite");
    for _ in 0..2 {
        assert_eq!(
            dispatcher
                .invoke(
                    &cx,
                    &observe_context(),
                    &root,
                    session,
                    id,
                    digest.clone(),
                    &invocation_body("build"),
                )
                .expect_err("unresolved")
                .code,
            RefusalCode::Forbidden
        );
    }
    // Resolution refuses before the reservation, so the key is not poisoned
    // and no run id was spent: once the selector resolves, this id admits.
    let reader = match Store::read(&observe_context(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("root {other:?}"),
    };
    let rows: i64 = reader
        .conn()
        .query_row("SELECT count(*) FROM workspace_admissions", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(rows, 0);
    drop(reader);
    dispatcher.resolver.fail = false;
    let AdmissionReply::Accepted(accepted) = dispatcher
        .invoke(
            &cx,
            &observe_context(),
            &root,
            session,
            id,
            digest,
            &invocation_body("build"),
        )
        .expect("admitted once the selector resolves")
    else {
        panic!("a fresh admission is accepted");
    };
    assert_eq!(dispatcher.admission.tickets.len(), 1);
    assert_eq!(accepted.run, dispatcher.admission.tickets[0].run());
}

#[test]
fn a_guest_placement_cannot_invoke_and_a_foreign_run_cannot_be_steered() {
    let (_fixture, cx, root, session) = journal("effigy-channel-guest");
    let mut dispatcher = Dispatcher::new(
        Admitter::default(),
        Resolver::new(task("workspace-channel").resource()),
        Observer::default(),
        Ids::new(&[41, 42, 43]),
    );
    let guest = effects(Placement::Guest);
    assert_eq!(
        dispatcher
            .invoke(
                &guest,
                &observe_context(),
                &root,
                session,
                RequestId::new([7; 16]),
                Digest::of(b"invite"),
                &invocation_body("build"),
            )
            .expect_err("a guest cannot dispatch a host admission")
            .code,
        RefusalCode::Forbidden
    );
    // Cancelling or detaching a run this session never admitted refuses, so
    // naming a run id grants nothing.
    let foreign = RunBody {
        run: RunId::new(1, [SESSION; 10]).expect("run id"),
    };
    for refusal in [
        dispatcher
            .cancel(&cx, &root, session, None, &foreign)
            .expect_err("a foreign run"),
        dispatcher
            .detach(&cx, &root, session, &foreign)
            .expect_err("a foreign run"),
    ] {
        assert_eq!(refusal.code, RefusalCode::Forbidden);
    }
    assert_eq!(
        dispatcher
            .attach(
                &cx,
                &root,
                session,
                RequestId::new([8; 16]),
                &AttachBody {
                    run: foreign.run,
                    streams: vec![StreamKind::Stdout],
                    cursor: None,
                },
            )
            .expect_err("a foreign run cannot be attached")
            .code,
        RefusalCode::Forbidden
    );
}

#[test]
fn an_owner_detach_and_a_secondary_eof_both_leave_the_run_running() {
    let (_fixture, cx, root, session) = journal("effigy-channel-attach");
    let owner = task("workspace-channel").resource();
    let mut connection = Dispatcher::new(
        Admitter::default(),
        Resolver::new(owner.clone()),
        Observer::default(),
        Ids::new(&[91, 92]),
    );
    let id = RequestId::new([11; 16]);
    let digest = Digest::of(b"invite");
    let body = invocation_body("build");
    let AdmissionReply::Accepted(accepted) = connection
        .invoke(
            &cx,
            &observe_context(),
            &root,
            session,
            id,
            digest.clone(),
            &body,
        )
        .expect("reserved")
    else {
        panic!("a fresh admission is accepted");
    };
    let ticket = connection.admission.tickets[0].clone();
    let mut run = Run::begin_admitted(
        &cx,
        &root,
        &invocation(owner.clone()),
        BeginOpts {
            id: ticket.run(),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next_argv(),
            custody: None,
        },
        &cleanup(&cx),
        &mut NoEffects,
        ticket,
    )
    .expect("admitted");
    assert_eq!(accepted.run, run.id());

    // The connection that admitted the run is its owning client.
    let mut owner_handle = connection
        .attach(
            &cx,
            &root,
            session,
            id,
            &AttachBody {
                run: accepted.run,
                streams: vec![StreamKind::Stdout],
                cursor: Some(0),
            },
        )
        .expect("owner attachment");
    assert_eq!(owner_handle.client(), Client::Owner);
    assert_eq!(owner_handle.run(), accepted.run);
    assert!(owner_handle.attached(&cx, &root).expect("attached"));

    // A second terminal is secondary, and its EOF releases only its own row.
    let mut second = Dispatcher::new(
        Admitter::default(),
        Resolver::new(owner.clone()),
        Observer::default(),
        Ids::new(&[93]),
    );
    let mut secondary = second
        .attach(
            &cx,
            &root,
            session,
            RequestId::new([12; 16]),
            &AttachBody {
                run: accepted.run,
                streams: vec![StreamKind::Stdout, StreamKind::Stderr],
                cursor: None,
            },
        )
        .expect("secondary attachment");
    assert_eq!(secondary.client(), Client::Secondary);
    secondary
        .release(&cx, &root, Release::Eof)
        .expect("secondary EOF");
    assert!(matches!(
        run::lookup(
            &observe_context(),
            &root,
            run::AdmissionKey {
                session,
                request: id
            },
            digest.clone()
        ),
        Observed::Verified(Recovered::Admitted { .. }, _)
    ));
    assert!(owner_handle
        .attached(&cx, &root)
        .expect("owner still attached"));

    // An acknowledged detach leaves the run going; the run itself then closes.
    owner_handle
        .release(&cx, &root, Release::Detach)
        .expect("owner detach");
    assert!(matches!(
        run::lookup(
            &observe_context(),
            &root,
            run::AdmissionKey {
                session,
                request: id
            },
            digest
        ),
        Observed::Verified(Recovered::Admitted { .. }, _)
    ));
    run.finish(crate::outcome::WorkloadOutcome::Succeeded)
        .expect("finish");
    run.settle(&cleanup(&cx), &mut NoEffects).expect("settle");
}

#[test]
fn recorded_admissions_map_to_accepted_completed_conflict_and_unknown() {
    let run = RunId::new(9, [9; 10]).expect("run id");
    let accepted = replay(Recovered::Admitted {
        run,
        fence: crate::run::Fence {
            resource: task("workspace-channel").resource(),
            generation: Generation::new(1),
        },
    })
    .expect("accepted");
    assert_eq!(
        accepted,
        AdmissionReply::Accepted(Accepted {
            run,
            placement: Placement::Host
        })
    );
    let done = replay(Recovered::Completed {
        run,
        outcome: RecordedOutcome {
            workload: crate::outcome::WorkloadOutcome::Succeeded,
            cleanup_complete: true,
        },
    })
    .expect("completed");
    assert!(matches!(done, AdmissionReply::Completed(_)));
    assert_eq!(
        replay(Recovered::Conflict {
            recorded: Digest::of(b"other")
        })
        .expect_err("conflict")
        .code,
        RefusalCode::IdConflict
    );
    for reason in [
        crate::run::UnknownAdmission::ReservedNotAdmitted,
        crate::run::UnknownAdmission::AdmissionNotRecovered,
    ] {
        assert_eq!(
            replay(Recovered::Unknown { reason })
                .expect_err("unknown")
                .code,
            RefusalCode::Unknown
        );
    }
}

#[test]
fn a_stream_window_backpressures_data_and_frees_only_what_was_acknowledged() {
    let mut outbound = Outbound::new();
    let id = RequestId::new([9; 16]);
    let payload = vec![0x7f; MAX_STREAM_PAYLOAD];
    let mut frames = 0;
    loop {
        let left = outbound.push_chunk(id, 1, StreamKind::Stdout, &payload);
        if left.len() == payload.len() {
            break;
        }
        frames += 1;
        assert!(frames <= MAX_UNACKED_STREAM_FRAMES);
        assert!(left.is_empty());
    }
    assert_eq!(frames, MAX_UNACKED_STREAM_FRAMES);
    let queued = outbound.take();
    assert_eq!(queued.len(), MAX_UNACKED_STREAM_FRAMES);
    assert_eq!(outbound.window().unacked(), MAX_UNACKED_STREAM_FRAMES);
    assert_eq!(
        outbound.window().queued(),
        MAX_UNACKED_STREAM_FRAMES * MAX_STREAM_PAYLOAD
    );
    // The window is full, so a further data frame is held back.
    let left = outbound.push_chunk(id, 1, StreamKind::Stdout, &payload);
    assert_eq!(left, payload.as_slice());
    // An acknowledgement frees exactly the frames at or below its sequence.
    outbound.acknowledge(1, 7);
    assert_eq!(outbound.window().unacked(), MAX_UNACKED_STREAM_FRAMES - 8);
    for freed in 0..8 {
        assert!(
            outbound
                .push_chunk(id, 1, StreamKind::Stdout, &payload)
                .is_empty(),
            "frame {freed} fits the acknowledged space"
        );
    }
    assert_eq!(
        outbound.push_chunk(id, 1, StreamKind::Stdout, &payload),
        payload.as_slice(),
        "the window is full again"
    );
    let resumed = outbound.take();
    assert_eq!(resumed.len(), 8);
    assert_eq!(resumed[0].seq, 16, "the sequence continues after a resume");
    // A chunk larger than one frame is split into bounded frames, never sent
    // as an oversized payload.
    let mut fresh = Outbound::new();
    let large = vec![0x5a; MAX_STREAM_PAYLOAD * 2 + 1];
    assert!(fresh
        .push_chunk(id, 1, StreamKind::Stdout, &large)
        .is_empty());
    let split = fresh.take();
    assert_eq!(split.len(), 3);
    for frame in &split {
        let payload = frame
            .payload
            .as_deref()
            .expect("a data frame carries bytes");
        assert!(decode_stream_payload(payload).expect("base64").len() <= MAX_STREAM_PAYLOAD);
    }
}

#[test]
fn a_stream_payload_is_base64_and_bounded() {
    for length in [0usize, 1, 2, 3, 4, 31, MAX_STREAM_PAYLOAD] {
        let bytes: Vec<u8> = (0..length).map(|n| (n % 251) as u8).collect();
        let encoded = encode_stream_payload(&bytes);
        assert_eq!(decode_stream_payload(&encoded).expect("decoded"), bytes);
    }
    let too_long = "A".repeat(MAX_STREAM_PAYLOAD.div_ceil(3) * 4 + 4);
    assert_eq!(
        decode_stream_payload(&too_long)
            .expect_err("oversized")
            .code,
        RefusalCode::InvalidRequest
    );
    for bad in ["!!!!", "AAA", "AA=A", "A==="] {
        assert_eq!(
            decode_stream_payload(bad).expect_err("not base64").code,
            RefusalCode::InvalidRequest
        );
    }
    let resize = StreamFrame {
        id: RequestId::new([1; 16]),
        stream_id: 1,
        seq: 0,
        kind: StreamKind::Resize,
        payload: None,
        rows: Some(0),
        columns: Some(80),
    };
    assert_eq!(
        decode_frame(
            &serde_json::to_vec(&serde_json::json!({
                "frame": "stream",
                "id": resize.id,
                "stream_id": 1,
                "seq": 0,
                "kind": "resize",
                "rows": 0,
                "columns": 80,
            }))
            .expect("json")
        )
        .expect_err("a zero row survives no resize")
        .code,
        RefusalCode::InvalidRequest
    );
}

#[test]
fn a_full_conversation_serves_observe_invoke_attach_refusals_and_eof() {
    let (_fixture, cx, root, session) = journal("effigy-channel-conversation");
    let mut host = server(session, &[51, 52]);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            Frame::Request(request(
                session,
                RequestBody::Observe(ObserveBody {
                    op: "run:status".into(),
                    target: None,
                }),
                1,
            )),
            Frame::Request(request(
                session,
                RequestBody::Invoke(invocation_body("build")),
                2,
            )),
            // A stale generation is answered per request, not by closing.
            Frame::Request(Request {
                context_generation: Generation::new(2),
                ..request(
                    session,
                    RequestBody::Observe(ObserveBody {
                        op: "run:status".into(),
                        target: None,
                    }),
                    3,
                )
            }),
        ]),
        peer(),
    );
    let served = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect("served");
    assert_eq!(served.responses, 3);
    assert!(served.closed);
    let frames = written_frames(&pipe);
    assert!(matches!(frames[0], Frame::HelloOk(_)));
    let responses: Vec<&Frame> = frames
        .iter()
        .filter(|frame| matches!(frame, Frame::Response(_)))
        .collect();
    assert_eq!(responses.len(), 3);
    assert!(matches!(
        responses[0],
        Frame::Response(Response {
            outcome: ResponseOutcome::Observed(_),
            ..
        })
    ));
    assert!(matches!(
        responses[1],
        Frame::Response(Response {
            outcome: ResponseOutcome::Accepted(Accepted {
                placement: Placement::Host,
                ..
            }),
            ..
        })
    ));
    assert!(matches!(
        responses[2],
        Frame::Response(Response {
            outcome: ResponseOutcome::Refused(Refusal {
                code: RefusalCode::StaleContext,
                ..
            }),
            ..
        })
    ));
    assert_eq!(host.dispatcher.admission.tickets.len(), 1);
}

#[test]
fn an_enrolment_issues_a_scoped_capability_and_nothing_else() {
    let (_fixture, cx, root, session) = journal("effigy-channel-enrol");
    let enrolment = Secret::new([0x77; 32]);
    let mut grants = GrantBook::new();
    grants.enrollable(Enrolment {
        credential: enrolment.clone(),
        session,
        environment_generation: Generation::new(4),
        container_id: "c0ffee".into(),
        vm_boot_id: "boot-1".into(),
        host_user: "operator".into(),
        checkout: "checkout".into(),
        scope: EnrolmentScope {
            operations: [Method::Observe].into_iter().collect(),
            targets: BTreeSet::new(),
        },
    });
    let mut host = Server::new(
        host(),
        fence(session),
        pinned(),
        grants,
        Dispatcher::new(
            Admitter::default(),
            Resolver::new(task("workspace-channel").resource()),
            Observer::default(),
            Ids::new(&[61]),
        ),
    );
    let enrolled = Secret::new([0x42; 32]);
    let enrol = EnrolRequest {
        credential: enrolment,
        capability: *enrolled.bytes(),
        session_run_id: session.run,
        session_generation: session.generation,
        environment_generation: Generation::new(4),
        container_id: "c0ffee".into(),
        vm_boot_id: "boot-1".into(),
    };
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Enrol(enrol),
            Frame::Request(request(
                session,
                RequestBody::Observe(ObserveBody {
                    op: "run:status".into(),
                    target: None,
                }),
                1,
            )),
            // Enrolment's scope does not include an invocation.
            Frame::Request(request(
                session,
                RequestBody::Invoke(invocation_body("build")),
                2,
            )),
        ]),
        peer(),
    );
    let served = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect("served");
    assert_eq!(served.responses, 2);
    let frames = written_frames(&pipe);
    let responses: Vec<&Frame> = frames
        .iter()
        .filter(|frame| matches!(frame, Frame::Response(_)))
        .collect();
    assert!(matches!(
        responses[0],
        Frame::Response(Response {
            outcome: ResponseOutcome::Observed(_),
            ..
        })
    ));
    assert!(matches!(
        responses[1],
        Frame::Response(Response {
            outcome: ResponseOutcome::Refused(Refusal {
                code: RefusalCode::Forbidden,
                ..
            }),
            ..
        })
    ));
    assert!(host.dispatcher.admission.tickets.is_empty());
    // The issued capability is memory-only and bound to the enrolment's scope.
    let grant = host.grants.authenticate(&enrolled).expect("issued");
    assert_eq!(grant.session(), session);
    assert!(grant.authorise(Method::Observe, None).is_ok());
}

#[test]
fn an_enrolment_cannot_name_another_session() {
    let (_fixture, cx, root, session) = journal("effigy-channel-enrol-fence");
    let enrolment = Secret::new([0x78; 32]);
    let mut grants = GrantBook::new();
    grants.enrollable(Enrolment {
        credential: enrolment.clone(),
        session,
        environment_generation: Generation::new(4),
        container_id: "c0ffee".into(),
        vm_boot_id: "boot-1".into(),
        host_user: "operator".into(),
        checkout: "checkout".into(),
        scope: EnrolmentScope {
            operations: [Method::Observe].into_iter().collect(),
            targets: BTreeSet::new(),
        },
    });
    let mut host = Server::new(
        host(),
        fence(session),
        pinned(),
        grants,
        Dispatcher::new(
            Admitter::default(),
            Resolver::new(task("workspace-channel").resource()),
            Observer::default(),
            Ids::new(&[64]),
        ),
    );
    let enrolled = Secret::new([0x43; 32]);
    // The credential is issued for this host session, so a frame that claims
    // another run or generation refuses before a capability exists.
    for claimed in [
        Session {
            run: RunId::new(1, [77; 10]).expect("run id"),
            generation: session.generation,
        },
        Session {
            run: session.run,
            generation: Generation::new(9),
        },
    ] {
        let mut pipe = Pipe::new(
            stream(&[
                Frame::Hello(hello()),
                Frame::Enrol(EnrolRequest {
                    credential: enrolment.clone(),
                    capability: *enrolled.bytes(),
                    session_run_id: claimed.run,
                    session_generation: claimed.generation,
                    environment_generation: Generation::new(4),
                    container_id: "c0ffee".into(),
                    vm_boot_id: "boot-1".into(),
                }),
            ]),
            peer(),
        );
        let refusal = host
            .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
            .expect_err("a foreign session refuses");
        assert_eq!(refusal.code, RefusalCode::IdentityMismatch);
        assert!(host.grants.authenticate(&enrolled).is_err());
    }
    // The same credential for its own session enrols the capability.
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Enrol(EnrolRequest {
                credential: enrolment,
                capability: *enrolled.bytes(),
                session_run_id: session.run,
                session_generation: session.generation,
                environment_generation: Generation::new(4),
                container_id: "c0ffee".into(),
                vm_boot_id: "boot-1".into(),
            }),
            Frame::Request(request(
                session,
                RequestBody::Observe(ObserveBody {
                    op: "run:status".into(),
                    target: None,
                }),
                1,
            )),
        ]),
        peer(),
    );
    let served = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect("enrolled");
    assert_eq!(served.responses, 1);
    assert_eq!(
        host.grants
            .authenticate(&enrolled)
            .expect("issued")
            .session(),
        session
    );
}

#[test]
fn a_grant_is_used_only_on_the_connection_its_binding_names() {
    let (_fixture, cx, root, session) = journal("effigy-channel-grant-fence");
    let other = Session {
        run: RunId::new(1, [88; 10]).expect("run id"),
        generation: Generation::new(1),
    };
    let mut grants = GrantBook::new();
    // The capability is bound to another session than this connection's fence.
    grants.issue(
        capability(),
        GrantBinding {
            session: other,
            ..binding(other)
        },
        session_operations(),
        [Target::Host].into_iter().collect(),
    );
    let mut host = Server::new(
        host(),
        fence(session),
        pinned(),
        grants,
        Dispatcher::new(
            Admitter::default(),
            Resolver::new(task("workspace-channel").resource()),
            Observer::default(),
            Ids::new(&[65]),
        ),
    );
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            Frame::Request(request(
                session,
                RequestBody::Observe(ObserveBody {
                    op: "run:status".into(),
                    target: None,
                }),
                1,
            )),
        ]),
        peer(),
    );
    let served = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect("served");
    assert_eq!(served.responses, 1);
    assert_eq!(host.dispatcher.observer.calls, 0);
    let frames = written_frames(&pipe);
    assert!(matches!(
        &frames[1],
        Frame::Response(Response {
            outcome: ResponseOutcome::Refused(Refusal {
                code: RefusalCode::IdentityMismatch,
                ..
            }),
            ..
        })
    ));
    // The same grant on its own connection is accepted.
    let mut own_grants = GrantBook::new();
    own_grants.issue(
        capability(),
        binding(session),
        session_operations(),
        [Target::Host].into_iter().collect(),
    );
    let mut own = Server::new(
        crate::workspace_channel::tests::host(),
        fence(session),
        pinned(),
        own_grants,
        Dispatcher::new(
            Admitter::default(),
            Resolver::new(task("workspace-channel").resource()),
            Observer::default(),
            Ids::new(&[66]),
        ),
    );
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            Frame::Request(request(
                session,
                RequestBody::Observe(ObserveBody {
                    op: "run:status".into(),
                    target: None,
                }),
                1,
            )),
        ]),
        peer(),
    );
    own.serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect("served");
    assert_eq!(own.dispatcher.observer.calls, 1);
}

#[test]
fn an_attachment_is_replaced_or_refused_and_never_orphaned() {
    let (_fixture, cx, root, session) = journal("effigy-channel-attach-lifecycle");
    let (_first, first_run) = admitted_run(&cx, &root, session, 21, &[101]);
    let (_second, second_run) = admitted_run(&cx, &root, session, 22, &[102]);
    let mut host = server(session, &[103]);
    let attach = |id: u8, run: RunId| {
        Frame::Request(request(
            session,
            RequestBody::Attach(AttachBody {
                run,
                streams: vec![StreamKind::Stdout],
                cursor: None,
            }),
            id,
        ))
    };
    // The conversation ends on a frame no authenticated connection may carry,
    // so the host closes it without an EOF release: exactly the state a client
    // that vanished leaves for the liveness watch.
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach(1, first_run),
            // Same run, another request id: this connection's own secondary is
            // replaced, and the old row is released rather than orphaned.
            attach(2, first_run),
            // Another run at the same time refuses; one connection, one
            // attachment.
            attach(3, second_run),
            Frame::Hello(hello()),
        ]),
        peer(),
    );
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect_err("an unexpected frame closes the connection");
    assert_eq!(refusal.code, RefusalCode::InvalidRequest);
    let frames = written_frames(&pipe);
    let responses: Vec<&Response> = frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Response(response) => Some(response),
            _ => None,
        })
        .collect();
    assert_eq!(responses.len(), 3);
    let opened = |response: &Response| match &response.outcome {
        ResponseOutcome::Observed(value) => {
            serde_json::from_value::<AttachView>(value.clone()).expect("an attach view")
        }
        other => panic!("expected an attachment, got {other:?}"),
    };
    let first = opened(responses[0]);
    let second = opened(responses[1]);
    assert_eq!(first.run, first_run);
    assert_eq!(second.run, first_run);
    assert_ne!(first.attachment, second.attachment);
    assert!(matches!(
        &responses[2].outcome,
        ResponseOutcome::Refused(Refusal {
            code: RefusalCode::Forbidden,
            ..
        })
    ));
    let reader = match Store::read(&observe_context(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("root {other:?}"),
    };
    let state = |attachment: AttachmentId| -> String {
        reader
            .conn()
            .query_row(
                "SELECT state FROM run_attachments WHERE attachment_id=?1",
                [attachment.to_string()],
                |row| row.get(0),
            )
            .expect("an attachment row")
    };
    // The replaced secondary was released when it was replaced; the newest one
    // is still attached for its client, and the owner row was never touched.
    assert_eq!(state(first.attachment), "released");
    assert_eq!(state(second.attachment), "attached");
    let secondaries: i64 = reader
        .conn()
        .query_row(
            "SELECT count(*) FROM run_attachments WHERE run_id=?1 AND lifetime='secondary'",
            [first_run.to_string()],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(secondaries, 2, "the replace opened exactly one more row");
    let foreign: i64 = reader
        .conn()
        .query_row(
            "SELECT count(*) FROM run_attachments WHERE run_id=?1",
            [second_run.to_string()],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(foreign, 1, "only the refused run's owner row exists");
}

#[test]
fn a_full_stream_window_still_serves_cancel_and_resumes_after_an_ack() {
    let (_fixture, cx, root, session) = journal("effigy-channel-backpressure");
    let (_run, live) = admitted_run(&cx, &root, session, 31, &[111]);
    let mut host = server(session, &[112, 113]);
    let mut streams = ScriptedStreams::frames(MAX_UNACKED_STREAM_FRAMES + 4);
    let cancel = |id: u8| {
        Frame::Request(request(
            session,
            RequestBody::Cancel(RunBody { run: live }),
            id,
        ))
    };
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            Frame::Request(request(
                session,
                RequestBody::Attach(AttachBody {
                    run: live,
                    streams: vec![StreamKind::Stdout],
                    cursor: None,
                }),
                1,
            )),
            // The window fills below; these controls are still answered.
            cancel(2),
            cancel(3),
            Frame::Stream(StreamFrame {
                id: RequestId::new([1; 16]),
                stream_id: 1,
                seq: (MAX_UNACKED_STREAM_FRAMES as u64) - 1,
                kind: StreamKind::Ack,
                payload: None,
                rows: None,
                columns: None,
            }),
        ]),
        peer(),
    );
    let served = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("served");
    assert_eq!(served.responses, 3);
    let frames = written_frames(&pipe);
    let data = |frames: &[Frame]| {
        frames
            .iter()
            .filter(|frame| {
                matches!(
                    frame,
                    Frame::Stream(StreamFrame {
                        kind: StreamKind::Stdout,
                        ..
                    })
                )
            })
            .count()
    };
    // Sixteen data frames fill the window; the cancel responses are written
    // anyway, and only the acknowledgement lets the held chunk resume.
    assert_eq!(data(&frames), MAX_UNACKED_STREAM_FRAMES + 4);
    assert!(streams.remaining() == 0);
    let cancel_responses = frames
        .iter()
        .filter(|frame| {
            matches!(
                frame,
                Frame::Response(Response {
                    outcome: ResponseOutcome::Observed(_),
                    ..
                }) if true
            )
        })
        .count();
    assert_eq!(cancel_responses, 3, "attach and both cancels were answered");
    // Both cancellations reached the run's mailbox as stop requests.
    let reader = match Store::read(&observe_context(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("root {other:?}"),
    };
    let stops: i64 = reader
        .conn()
        .query_row(
            "SELECT count(*) FROM requests WHERE run_id=?1 AND state='pending'",
            [live.to_string()],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(stops, 2);
}

fn enrol_request(session: Session, capability: Secret) -> EnrolRequest {
    EnrolRequest {
        credential: Secret::new([0; 32]),
        capability: *capability.bytes(),
        session_run_id: session.run,
        session_generation: session.generation,
        environment_generation: Generation::new(4),
        container_id: "c0ffee".into(),
        vm_boot_id: "boot-1".into(),
    }
}

fn enrolable_book(session: Session, credential: &Secret) -> GrantBook {
    let mut grants = GrantBook::new();
    grants.enrollable(Enrolment {
        credential: credential.clone(),
        session,
        environment_generation: Generation::new(4),
        container_id: "c0ffee".into(),
        vm_boot_id: "boot-1".into(),
        host_user: "operator".into(),
        checkout: "checkout".into(),
        scope: EnrolmentScope {
            operations: [Method::Observe].into_iter().collect(),
            targets: BTreeSet::new(),
        },
    });
    grants
}

#[test]
fn a_revoked_generation_ends_the_enrolment_credential() {
    let (_fixture, cx, root, session) = journal("effigy-channel-revoke-enrol");
    let credential = Secret::new([0x79; 32]);
    let mut grants = enrolable_book(session, &credential);
    let first = Secret::new([0x44; 32]);
    let mut request = enrol_request(session, first.clone());
    request.credential = credential.clone();
    grants.enrol(&credential, &request).expect("enrolled");
    assert!(grants.authenticate(&first).is_ok());

    // Revoking another session leaves this one enrolable.
    grants.revoke_generation(RunId::new(1, [77; 10]).expect("run id"), Generation::new(9));
    assert!(grants.enrolable());
    // Revoking this generation ends both the issued capability and the
    // credential that could mint another one (§8.11).
    grants.revoke_generation(session.run, session.generation);
    assert!(!grants.enrolable());
    assert_eq!(
        grants.authenticate(&first).expect_err("revoked").code,
        RefusalCode::Unauthenticated
    );
    let second = Secret::new([0x45; 32]);
    let mut again = enrol_request(session, second.clone());
    again.credential = credential.clone();
    assert_eq!(
        grants
            .enrol(&credential, &again)
            .expect_err("the credential is gone with its generation")
            .code,
        RefusalCode::Forbidden
    );

    // The same over the wire: a re-enrolment after revocation refuses and
    // issues nothing.
    let mut wire = enrolable_book(session, &credential);
    wire.revoke_generation(session.run, session.generation);
    let mut host = Server::new(
        host(),
        fence(session),
        pinned(),
        wire,
        Dispatcher::new(
            Admitter::default(),
            Resolver::new(task("workspace-channel").resource()),
            Observer::default(),
            Ids::new(&[67]),
        ),
    );
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Enrol(enrol_request(session, Secret::new([0x46; 32]))),
        ]),
        peer(),
    );
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect_err("a revoked enrolment refuses");
    assert_eq!(refusal.code, RefusalCode::Forbidden);
    assert!(!host.grants.enrolable());
    assert!(host.grants.authenticate(&Secret::new([0x46; 32])).is_err());
}

#[test]
fn a_reused_server_starts_each_connection_with_no_stream_state() {
    let (_fixture, cx, root, session) = journal("effigy-channel-reuse");
    let (_run, live) = admitted_run(&cx, &root, session, 41, &[121]);
    let mut host = server(session, &[122, 123]);
    let attach = |id: u8| {
        Frame::Request(request(
            session,
            RequestBody::Attach(AttachBody {
                run: live,
                streams: vec![StreamKind::Stdout],
                cursor: None,
            }),
            id,
        ))
    };
    // The first connection fills the window, detaches, then closes on a frame
    // no authenticated connection may carry (so no EOF release).
    let mut first_streams = ScriptedStreams::frames(MAX_UNACKED_STREAM_FRAMES + 2);
    let mut first = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach(1),
            Frame::Request(request(
                session,
                RequestBody::Detach(RunBody { run: live }),
                2,
            )),
            Frame::Hello(hello()),
        ]),
        peer(),
    );
    let _ = host
        .serve(
            &mut first,
            &cx,
            &observe_context(),
            &root,
            &mut first_streams,
        )
        .expect_err("the connection closes");
    assert_eq!(
        host.outbound.window().unacked(),
        MAX_UNACKED_STREAM_FRAMES,
        "the first connection left its window full"
    );
    assert!(first_streams.remaining() > 0);
    assert!(host.stream_id.is_none(), "the detach cleared the stream");
    assert!(host.carry.is_none(), "the detach dropped the held chunk");

    // The second connection reuses the same server: it publishes nothing
    // before its own attach, and its frames name its own request id.
    let mut second_streams = ScriptedStreams::frames(MAX_UNACKED_STREAM_FRAMES);
    let mut second = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach(3),
        ]),
        peer(),
    );
    host.serve(
        &mut second,
        &cx,
        &observe_context(),
        &root,
        &mut second_streams,
    )
    .expect("served");
    let frames = written_frames(&second);
    let attach_at = frames
        .iter()
        .position(|frame| matches!(frame, Frame::Response(_)))
        .expect("the attach was answered");
    let data: Vec<&StreamFrame> = frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Stream(stream) if stream.kind == StreamKind::Stdout => Some(stream),
            _ => None,
        })
        .collect();
    assert_eq!(
        data.len(),
        MAX_UNACKED_STREAM_FRAMES,
        "the window was reset for the new connection"
    );
    let first_data = frames
        .iter()
        .position(
            |frame| matches!(frame, Frame::Stream(stream) if stream.kind == StreamKind::Stdout),
        )
        .expect("data was published");
    assert!(
        first_data > attach_at,
        "nothing is published before this connection attaches"
    );
    let own = RequestId::new([3; 16]);
    assert!(
        data.iter().all(|frame| frame.id == own),
        "every frame names this connection's attach"
    );
    assert_eq!(second_streams.remaining(), 0);
}

#[test]
fn a_connection_is_bounded_and_a_bad_frame_closes_it() {
    let (_fixture, cx, root, session) = journal("effigy-channel-bounded");
    let mut host = server(session, &[71]);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            Frame::Hello(hello()),
        ]),
        peer(),
    );
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect_err("a second hello refuses");
    assert_eq!(refusal.code, RefusalCode::InvalidRequest);
    let frames = written_frames(&pipe);
    assert!(matches!(frames[0], Frame::HelloOk(_)));
    assert!(matches!(&frames[1], Frame::Refuse(r) if r.code == RefusalCode::InvalidRequest));
}

#[test]
fn a_relative_working_directory_is_the_only_one_accepted() {
    for good in ["", "packages/app", "./app", "app//nested/./x"] {
        assert!(check_relative_path(good).is_ok(), "{good}");
    }
    for bad in ["/etc", "../x", "a/../../b", "a\\b", "a\0b"] {
        assert_eq!(
            check_relative_path(bad).expect_err("refused").code,
            RefusalCode::InvalidRequest,
            "{bad:?}"
        );
    }
    // An empty argument list is a body limit, not a selector rule.
    let mut body = invocation_body("build");
    body.args.clear();
    assert!(body.check().is_ok());
    let long = "x".repeat(MAX_PATH_BYTES + 1);
    let mut selector = invocation_body("build");
    selector.selector = Selector::Shell(long);
    assert_eq!(
        selector.check().expect_err("bounded").code,
        RefusalCode::InvalidRequest
    );
}
