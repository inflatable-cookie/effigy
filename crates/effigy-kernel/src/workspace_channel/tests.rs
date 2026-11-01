//! §8.11 lane A proofs: the wire, the handshake, the grant and the replay
//! ordering, against a scripted transport, strict fakes for the host-engine
//! seams and the kernel's own reservation records.
use super::*;
use crate::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, Limits, ObserveContext, OperationContext,
};
use crate::custody::Stream;
use crate::fs::{FileName, Trust};
use crate::gate::Wait;
use crate::id::{
    CheckoutId, CheckoutKind, ExecutionTarget, InvocationSpec, ResourceId, Route, Surface, TaskId,
};
use crate::observe::{AbsenceProof, Observed};
use crate::outcome::Invocation;
use crate::root::{Family, StateRoot};
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

#[cfg(feature = "fault-injection")]
struct ArmReleaseOnAttach {
    inner: Pipe,
    point: crate::fault::Point,
}
#[cfg(feature = "fault-injection")]
impl Transport for ArmReleaseOnAttach {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.inner.write_all(bytes)?;
        if matches!(decode_frame(&bytes[4..]), Ok(Frame::Response(Response { id, outcome: ResponseOutcome::Observed(_), .. })) if id == RequestId::new([1; 16]))
        {
            crate::fault::arm_point(self.point, crate::fault::Fault::Eperm);
        }
        Ok(())
    }
    fn verified_peer(&self) -> Option<ServerIdentity> {
        self.inner.verified_peer()
    }
}

#[test]
#[cfg(feature = "fault-injection")]
fn secondary_eof_release_fault_retains_exact_handle_and_refuses_new_connections() {
    use crate::fault::{self, Point};
    for (point, still_attached) in [
        (Point::RunAttachReleaseBefore, true),
        (Point::RunAttachReleaseAfter, false),
    ] {
        let (_fixture, cx, root, session) = journal("effigy-channel-secondary-release-fault");
        let (_run, live) = admitted_run(&cx, &root, session, 92, &[172]);
        let mut host = server(session, &[173]);
        let mut pipe = ArmReleaseOnAttach {
            inner: Pipe::new(
                stream(&[
                    Frame::Hello(hello()),
                    Frame::Authenticate(capability()),
                    Frame::Request(request(
                        session,
                        RequestBody::Attach(AttachBody {
                            run: live,
                            streams: vec![StreamKind::Stdout],
                            cursor: None,
                            cursors: None,
                        }),
                        1,
                    )),
                ]),
                peer(),
            ),
            point,
        };
        let error = host
            .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
            .expect_err("release uncertainty");
        assert_eq!(error.code, RefusalCode::Unknown);
        assert!(fault::fired(), "the intended release fault must fire");
        assert!(host.cleanup_pending());
        fault::arm(None);
        let handle = host
            .unsettled_attachment
            .as_ref()
            .expect("exact handle retained");
        assert_eq!(handle.run(), live);
        assert_eq!(handle.client(), Client::Secondary);
        assert_eq!(
            handle.attached(&cx, &root).expect("durable row read"),
            still_attached
        );
        let reader = match Store::read(&observe_context(), &root) {
            Observed::Verified(reader, _) => reader,
            other => panic!("journal unavailable: {other:?}"),
        };
        let live_slots: i64 = reader.conn().query_row(
            "SELECT count(*) FROM run_attachments WHERE run_id=?1 AND lifetime='secondary' AND state='attached'",
            [live.to_string()], |row| row.get(0),
        ).expect("secondary slots");
        assert_eq!(live_slots, i64::from(still_attached));
        let stops: i64 = reader
            .conn()
            .query_row(
                "SELECT count(*) FROM requests WHERE run_id=?1",
                [live.to_string()],
                |row| row.get(0),
            )
            .expect("stop requests");
        assert_eq!(stops, 0, "secondary release never stops the owner");
        let mut retry = Pipe::new(
            stream(&[Frame::Hello(hello()), Frame::Authenticate(capability())]),
            peer(),
        );
        assert_eq!(
            host.serve(&mut retry, &cx, &observe_context(), &root, &mut NoStreams)
                .expect_err("unsettled cleanup fences service")
                .code,
            RefusalCode::Unknown
        );
        let fresh = cleanup(&cx)
            .operation()
            .effects()
            .expect("fresh cleanup authority");
        host.retry_cleanup(&fresh, &root)
            .expect("same attachment retry");
        assert!(!host.cleanup_pending());
    }
}

#[test]
#[cfg(feature = "fault-injection")]
fn replacement_release_fault_retains_both_secondary_handles() {
    use crate::fault::{self, Point};
    for (point, fail_new_cleanup) in [
        (Point::RunAttachReleaseBefore, None),
        (Point::RunAttachReleaseAfter, None),
        (
            Point::RunAttachReleaseBefore,
            Some(Point::RunAttachReleaseBefore),
        ),
        (
            Point::RunAttachReleaseAfter,
            Some(Point::RunAttachReleaseBefore),
        ),
        (
            Point::RunAttachReleaseBefore,
            Some(Point::RunAttachReleaseAfter),
        ),
        (
            Point::RunAttachReleaseAfter,
            Some(Point::RunAttachReleaseAfter),
        ),
    ] {
        let (_fixture, cx, root, session) = journal("effigy-channel-replacement-fault");
        let (_run, live) = admitted_run(&cx, &root, session, 93, &[175]);
        let mut host = server(session, &[176]);
        let attach = |id| {
            Frame::Request(request(
                session,
                RequestBody::Attach(AttachBody {
                    run: live,
                    streams: vec![StreamKind::Stdout],
                    cursor: None,
                    cursors: None,
                }),
                id,
            ))
        };
        let mut pipe = ArmReleaseOnAttach {
            inner: Pipe::new(
                stream(&[
                    Frame::Hello(hello()),
                    Frame::Authenticate(capability()),
                    attach(1),
                    attach(2),
                ]),
                peer(),
            ),
            point,
        };
        let _result = host.serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams);
        let fired = fault::fired();
        fault::arm(None);
        assert!(fired, "replacement release fault must fire");
        assert!(
            host.cleanup_pending(),
            "unsettled replacement must fence new service"
        );
        let old = host
            .unsettled_attachment
            .as_ref()
            .expect("old handle retained");
        let new = host
            .unsettled_replacement
            .as_ref()
            .expect("new handle retained");
        assert_eq!(old.id(), AttachmentId::new([1; 16]));
        assert_eq!(new.id(), AttachmentId::new([2; 16]));
        assert_eq!(old.run(), live);
        assert_eq!(new.run(), live);
        assert_eq!(
            old.attached(&cx, &root).unwrap(),
            point == Point::RunAttachReleaseBefore
        );
        assert!(new.attached(&cx, &root).unwrap());
        let mut retry = Pipe::new(
            stream(&[Frame::Hello(hello()), Frame::Authenticate(capability())]),
            peer(),
        );
        assert_eq!(
            host.serve(&mut retry, &cx, &observe_context(), &root, &mut NoStreams)
                .expect_err("replacement latch")
                .code,
            RefusalCode::Unknown
        );
        let fresh = cleanup(&cx)
            .operation()
            .effects()
            .expect("fresh cleanup authority");
        if let Some(new_fault) = fail_new_cleanup {
            fault::arm_nth(new_fault, 1, fault::Fault::Eperm);
            assert_eq!(
                host.retry_cleanup(&fresh, &root)
                    .expect_err("new release fails")
                    .code,
                RefusalCode::Unknown
            );
            assert!(fault::fired());
            fault::arm(None);
            assert!(host.cleanup_pending());
            assert!(
                host.unsettled_attachment.is_none(),
                "old exact release was reconciled"
            );
            assert_eq!(
                host.unsettled_replacement.as_ref().map(Attachment::id),
                Some(AttachmentId::new([2; 16]))
            );
            assert_eq!(
                host.unsettled_replacement
                    .as_ref()
                    .unwrap()
                    .attached(&cx, &root)
                    .unwrap(),
                new_fault == Point::RunAttachReleaseBefore
            );
        }
        host.retry_cleanup(&fresh, &root)
            .expect("both exact secondaries recover");
        assert!(!host.cleanup_pending());
        let reader = match Store::read(&observe_context(), &root) {
            Observed::Verified(reader, _) => reader,
            other => panic!("journal unavailable: {other:?}"),
        };
        let remaining: i64 = reader.conn().query_row("SELECT count(*) FROM run_attachments WHERE run_id=?1 AND lifetime='secondary' AND state='attached'", [live.to_string()], |row| row.get(0)).unwrap();
        let stops: i64 = reader
            .conn()
            .query_row(
                "SELECT count(*) FROM requests WHERE run_id=?1",
                [live.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);
        assert_eq!(stops, 0);
    }
}

#[cfg(feature = "fault-injection")]
struct LiveAdmitter<'a> {
    root: &'a StateRoot,
    runs: Vec<Run<'a>>,
}
#[cfg(feature = "fault-injection")]
impl Admission for LiveAdmitter<'_> {
    fn admitted_fence(&self, ticket: &AdmissionTicket) -> Option<run::Fence> {
        self.runs
            .iter()
            .find(|run| run.id() == ticket.run())
            .map(|run| run.fence().clone())
    }
    fn admit(
        &mut self,
        cx: &EffectContext,
        ticket: &AdmissionTicket,
        resolved: &ResolvedInvocation,
    ) -> Result<(), Refusal> {
        let run = Run::begin_admitted(
            cx,
            self.root,
            resolved,
            BeginOpts {
                id: ticket.run(),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next_argv(),
                custody: None,
            },
            &cleanup(cx),
            &mut NoEffects,
            ticket.clone(),
        )
        .map_err(refusal_from)?;
        self.runs.push(run);
        Ok(())
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn mismatched_supervisor_fence_retains_the_started_owner_without_authority() {
    struct Mismatched<'a>(LiveAdmitter<'a>);
    impl Admission for Mismatched<'_> {
        fn admit(
            &mut self,
            cx: &EffectContext,
            ticket: &AdmissionTicket,
            resolved: &ResolvedInvocation,
        ) -> Result<(), Refusal> {
            self.0.admit(cx, ticket, resolved)
        }
        fn admitted_fence(&self, ticket: &AdmissionTicket) -> Option<run::Fence> {
            let mut fence = self.0.admitted_fence(ticket)?;
            fence.generation = Generation::new(0);
            Some(fence)
        }
    }
    let (_fixture, cx, root, session) = journal("effigy-channel-ack-mismatch");
    let mut dispatcher = Dispatcher::new(
        Mismatched(LiveAdmitter {
            root: &root,
            runs: Vec::new(),
        }),
        Resolver::new(task("workspace-channel").resource()),
        Observer::default(),
        Ids::new(&[21, 22]),
    );
    let error = dispatcher
        .invoke(
            &cx,
            &observe_context(),
            &root,
            session,
            RequestId::new([21; 16]),
            Digest::of(b"invite"),
            &invocation_body("build"),
        )
        .expect_err("mismatched acknowledgement must fail closed");
    assert_eq!(error.code, RefusalCode::Unknown);
    assert_eq!(dispatcher.admission.0.runs.len(), 1);
    let run_id = dispatcher.admission.0.runs[0].id();
    assert!(dispatcher.admitted.contains(&run_id));
    assert!(dispatcher.admitted_owners.is_empty());
    assert_eq!(
        dispatcher
            .reopen_owner(&cx, &root, session, run_id)
            .expect_err("no owner authority")
            .code,
        RefusalCode::Unknown
    );
    let reader = match Store::read(&observe_context(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("journal unavailable: {other:?}"),
    };
    let stops: i64 = reader
        .conn()
        .query_row(
            "SELECT count(*) FROM requests WHERE run_id=?1",
            [run_id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stops, 0);
    drop(reader);
    for run in dispatcher.admission.0.runs.drain(..) {
        run.settle(&cleanup(&cx), &mut NoEffects)
            .expect("fixture cleanup");
    }
}

#[cfg(feature = "fault-injection")]
struct ArmReleaseOnInvoke {
    inner: Pipe,
    point: crate::fault::Point,
}
#[cfg(feature = "fault-injection")]
impl Transport for ArmReleaseOnInvoke {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.inner.write_all(bytes)?;
        if matches!(
            decode_frame(&bytes[4..]),
            Ok(Frame::Response(Response {
                outcome: ResponseOutcome::Accepted(_),
                ..
            }))
        ) {
            crate::fault::arm_point(self.point, crate::fault::Fault::Eperm);
        }
        Ok(())
    }
    fn verified_peer(&self) -> Option<ServerIdentity> {
        self.inner.verified_peer()
    }
}

#[test]
#[cfg(feature = "fault-injection")]
fn pre_attach_eof_fault_retains_exact_owner_handle() {
    use crate::fault::{self, Point};
    for point in [Point::RunAttachReleaseBefore, Point::RunAttachReleaseAfter] {
        let (_fixture, cx, root, session) = journal("effigy-channel-pre-attach-eof-fault");
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
                LiveAdmitter {
                    root: &root,
                    runs: vec![],
                },
                Resolver::new(task("workspace-channel").resource()),
                Observer::default(),
                Ids::new(&[177]),
            ),
        );
        let live = RunId::new(7, [177; 10]).unwrap();
        let mut pipe = ArmReleaseOnInvoke {
            inner: Pipe::new(
                stream(&[
                    Frame::Hello(hello()),
                    Frame::Authenticate(capability()),
                    Frame::Request(request(
                        session,
                        RequestBody::Invoke(invocation_body("build")),
                        1,
                    )),
                ]),
                peer(),
            ),
            point,
        };
        let error = host
            .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
            .expect_err("EOF cleanup is uncertain");
        assert_eq!(error.code, RefusalCode::Unknown);
        assert!(fault::fired());
        fault::arm(None);
        let retained = host
            .unsettled_attachment
            .as_ref()
            .map(|handle| (handle.run(), handle.fence().clone()));
        let fresh = cleanup(&cx)
            .operation()
            .effects()
            .expect("fresh cleanup authority");
        let retry = host.retry_cleanup(&fresh, &root);
        let reader = match Store::read(&observe_context(), &root) {
            Observed::Verified(reader, _) => reader,
            other => panic!("journal unavailable: {other:?}"),
        };
        let requests: i64 = reader
            .conn()
            .query_row(
                "SELECT count(*) FROM requests WHERE run_id=?1",
                [live.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        let request = reader
            .conn()
            .query_row(
                "SELECT action FROM requests WHERE run_id=?1",
                [live.to_string()],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|encoded| serde_json::from_str::<crate::run::Request>(&encoded).ok());
        drop(reader);
        for run in host.dispatcher.admission.runs.drain(..) {
            run.settle(&cleanup(&cx), &mut NoEffects).unwrap();
        }
        assert_eq!(
            retained.as_ref().map(|(run, _)| *run),
            Some(live),
            "the exact failed-release owner must remain held"
        );
        assert!(
            retry.is_ok(),
            "same owner cleanup should recover while the run is live"
        );
        assert_eq!(requests, 1, "retry must leave exactly one stop");
        assert!(
            request.is_some_and(|request| request.run == live
                && retained
                    .as_ref()
                    .is_some_and(|(_, fence)| &request.expected == fence)
                && matches!(request.action, crate::run::RequestAction::Stop)),
            "stop must name the exact run and fence"
        );
    }
}

#[test]
#[cfg(feature = "fault-injection")]
fn pre_attach_detach_fault_retains_exact_owner_handle() {
    use crate::fault::{self, Point};
    for point in [Point::RunAttachReleaseBefore, Point::RunAttachReleaseAfter] {
        let (_fixture, cx, root, session) = journal("effigy-channel-pre-attach-detach-fault");
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
                LiveAdmitter {
                    root: &root,
                    runs: vec![],
                },
                Resolver::new(task("workspace-channel").resource()),
                Observer::default(),
                Ids::new(&[177]),
            ),
        );
        let live = RunId::new(7, [177; 10]).unwrap();
        let mut pipe = ArmReleaseOnInvoke {
            inner: Pipe::new(
                stream(&[
                    Frame::Hello(hello()),
                    Frame::Authenticate(capability()),
                    Frame::Request(request(
                        session,
                        RequestBody::Invoke(invocation_body("build")),
                        1,
                    )),
                    Frame::Request(request(
                        session,
                        RequestBody::Detach(RunBody { run: live }),
                        2,
                    )),
                ]),
                peer(),
            ),
            point,
        };
        let error = host
            .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
            .expect_err("detach cleanup is uncertain");
        assert_eq!(error.code, RefusalCode::Unknown);
        assert!(fault::fired());
        fault::arm(None);
        let retained = host
            .unsettled_attachment
            .as_ref()
            .map(|handle| (handle.run(), handle.id(), handle.pending_release()));
        let fresh = cleanup(&cx).operation().effects().unwrap();
        let retry = host.retry_cleanup(&fresh, &root);
        let reader = match Store::read(&observe_context(), &root) {
            Observed::Verified(reader, _) => reader,
            other => panic!("journal unavailable: {other:?}"),
        };
        let requests: i64 = reader
            .conn()
            .query_row(
                "SELECT count(*) FROM requests WHERE run_id=?1",
                [live.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        drop(reader);
        for run in host.dispatcher.admission.runs.drain(..) {
            run.settle(&cleanup(&cx), &mut NoEffects).unwrap();
        }
        assert_eq!(
            retained,
            Some((
                live,
                crate::run::owner_attachment_id(live),
                Some(Release::Detach)
            )),
            "the exact failed-detach owner and cause must remain held"
        );
        assert!(retry.is_ok(), "same owner detach should recover while live");
        assert_eq!(requests, 0, "detach must not record an EOF stop");
    }
}

#[cfg(feature = "fault-injection")]
struct MultiResolver;
#[cfg(feature = "fault-injection")]
impl super::Resolver for MultiResolver {
    fn resolve(
        &mut self,
        _: &EffectContext,
        body: &InvocationBody,
    ) -> Result<ResolvedInvocation, Refusal> {
        let Selector::Task(name) = &body.selector else {
            return Err(Refusal::invalid("task required"));
        };
        Ok(invocation(task(name).resource()))
    }
}
#[cfg(feature = "fault-injection")]
struct ArmAfterTwoInvokes<'a> {
    inner: Pipe,
    accepted: usize,
    cx: &'a EffectContext,
    root: &'a StateRoot,
}
#[cfg(feature = "fault-injection")]
impl Transport for ArmAfterTwoInvokes<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.inner.write_all(bytes)?;
        if matches!(
            decode_frame(&bytes[4..]),
            Ok(Frame::Response(Response {
                outcome: ResponseOutcome::Accepted(_),
                ..
            }))
        ) {
            self.accepted += 1;
            if self.accepted == 2 {
                let mut writer = Store::open(self.cx, self.root)
                    .map_err(|error| io::Error::other(error.to_string()))?;
                writer
                    .transaction(self.cx, |txn| {
                        txn.conn().execute(
                            "UPDATE runs SET generation=generation+1 WHERE run_id=?1",
                            [RunId::new(7, [181; 10]).unwrap().to_string()],
                        )?;
                        txn.conn().execute(
                            "UPDATE resources SET generation=generation+1 WHERE name=?1",
                            [task("second").resource().name().to_string()],
                        )?;
                        Ok::<_, crate::run::RunError>(())
                    })
                    .map_err(|error| io::Error::other(error.to_string()))?;
                crate::fault::arm_point(
                    crate::fault::Point::RunAttachReleaseBefore,
                    crate::fault::Fault::Eperm,
                );
            }
        }
        Ok(())
    }
    fn verified_peer(&self) -> Option<ServerIdentity> {
        self.inner.verified_peer()
    }
}

#[test]
#[cfg(feature = "fault-injection")]
fn multiple_admitted_owners_retry_after_first_eof_release_fault() {
    let (_fixture, cx, root, session) = journal("effigy-channel-multi-owner-eof");
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
            LiveAdmitter {
                root: &root,
                runs: vec![],
            },
            MultiResolver,
            Observer::default(),
            Ids::new(&[180, 181]),
        ),
    );
    let mut pipe = ArmAfterTwoInvokes {
        inner: Pipe::new(
            stream(&[
                Frame::Hello(hello()),
                Frame::Authenticate(capability()),
                Frame::Request(request(
                    session,
                    RequestBody::Invoke(invocation_body("first")),
                    1,
                )),
                Frame::Request(request(
                    session,
                    RequestBody::Invoke(invocation_body("second")),
                    2,
                )),
            ]),
            peer(),
        ),
        accepted: 0,
        cx: &cx,
        root: &root,
    };
    let result = host.serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams);
    let fired = crate::fault::fired();
    crate::fault::arm(None);
    let fresh = cleanup(&cx).operation().effects().unwrap();
    let fences = host
        .dispatcher
        .admission
        .runs
        .iter()
        .map(|run| (run.id(), run.fence().clone()))
        .collect::<Vec<_>>();
    let (_foreign_fixture, _, foreign_root, _) = journal("effigy-channel-foreign-retry");
    let foreign = host.retry_cleanup(&fresh, &foreign_root);
    let second = RunId::new(7, [181; 10]).unwrap();
    let second_fence = fences
        .iter()
        .find(|(run, _)| *run == second)
        .unwrap()
        .1
        .clone();
    let mut writer = Store::open(&cx, &root).unwrap();
    let stale = host.retry_cleanup(&fresh, &root);
    let stale_requests: i64 = match Store::read(&observe_context(), &root) {
        Observed::Verified(reader, _) => reader
            .conn()
            .query_row(
                "SELECT count(*) FROM requests WHERE run_id=?1",
                [second.to_string()],
                |row| row.get(0),
            )
            .unwrap(),
        other => panic!("journal unavailable: {other:?}"),
    };
    writer
        .transaction(&cx, |txn| {
            txn.conn().execute(
                "UPDATE runs SET generation=generation-1 WHERE run_id=?1",
                [second.to_string()],
            )?;
            txn.conn().execute(
                "UPDATE resources SET generation=generation-1 WHERE name=?1",
                [second_fence.resource.name().to_string()],
            )?;
            Ok::<_, crate::run::RunError>(())
        })
        .unwrap();
    let retry = host.retry_cleanup(&fresh, &root);
    let reader = match Store::read(&observe_context(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("journal unavailable: {other:?}"),
    };
    let requests: i64 = reader
        .conn()
        .query_row("SELECT count(*) FROM requests", [], |row| row.get(0))
        .unwrap();
    let actions = {
        let mut statement = reader
            .conn()
            .prepare("SELECT action FROM requests")
            .unwrap();
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(|row| serde_json::from_str::<crate::run::Request>(&row.unwrap()).unwrap())
            .collect::<Vec<_>>()
    };
    drop(reader);
    for run in host.dispatcher.admission.runs.drain(..) {
        run.settle(&cleanup(&cx), &mut NoEffects).unwrap();
    }
    assert!(
        result.is_err() && fired,
        "first EOF release fault must fire"
    );
    assert!(foreign.is_err(), "foreign root must refuse");
    assert!(stale.is_err(), "changed original fence must refuse");
    assert_eq!(stale_requests, 0, "a replacement fence receives no stop");
    assert!(
        retry.is_ok(),
        "all original owner EOF releases should recover"
    );
    assert!(!host.cleanup_pending());
    assert_eq!(requests, 2, "each admitted owner gets one stop");
    assert!(
        actions.iter().all(
            |request| matches!(request.action, crate::run::RequestAction::Stop)
                && fences
                    .iter()
                    .any(|(run, fence)| *run == request.run && fence == &request.expected)
        ),
        "each stop must name its exact original run and fence"
    );
    assert_ne!(
        actions[0].run, actions[1].run,
        "the stops belong to distinct runs"
    );
}

#[test]
#[cfg(feature = "fault-injection")]
fn owner_release_fault_retains_exact_handle_and_reports_unknown() {
    use crate::fault::{self, Point};
    for (point, still_attached, detach, close_before_retry) in [
        (Point::RunAttachReleaseBefore, true, true, false),
        (Point::RunAttachReleaseAfter, false, true, false),
        (Point::RunAttachReleaseBefore, true, false, false),
        (Point::RunAttachReleaseAfter, false, false, false),
        (Point::RunAttachReleaseAfter, false, false, true),
    ] {
        let (_fixture, cx, root, session) = journal("effigy-channel-owner-detach-fault");
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
                LiveAdmitter {
                    root: &root,
                    runs: vec![],
                },
                Resolver::new(task("workspace-channel").resource()),
                Observer::default(),
                Ids::new(&[174]),
            ),
        );
        let live = RunId::new(7, [174; 10]).expect("run id");
        let mut frames = vec![
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            Frame::Request(request(
                session,
                RequestBody::Invoke(invocation_body("build")),
                2,
            )),
            Frame::Request(request(
                session,
                RequestBody::Attach(AttachBody {
                    run: live,
                    streams: vec![StreamKind::Stdout],
                    cursor: None,
                    cursors: None,
                }),
                1,
            )),
        ];
        if detach {
            frames.push(Frame::Request(request(
                session,
                RequestBody::Detach(RunBody { run: live }),
                3,
            )));
        }
        let mut pipe = ArmReleaseOnAttach {
            inner: Pipe::new(stream(&frames), peer()),
            point,
        };
        let error = host
            .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
            .expect_err("detach uncertainty");
        assert_eq!(error.code, RefusalCode::Unknown);
        assert!(fault::fired(), "release fault must fire");
        assert!(host.cleanup_pending());
        fault::arm(None);
        let handle = host
            .unsettled_attachment
            .as_ref()
            .expect("exact owner retained");
        assert_eq!(handle.run(), live);
        assert_eq!(handle.client(), Client::Owner);
        assert_eq!(
            handle.attached(&cx, &root).expect("durable owner row"),
            still_attached
        );
        if detach {
            assert!(written_frames(&pipe.inner).iter().any(|frame| matches!(frame,
                Frame::Response(Response { id, outcome: ResponseOutcome::Refused(refusal), .. }) if *id == RequestId::new([3;16]) && refusal.code == RefusalCode::Unknown)));
        }
        let reader = match Store::read(&observe_context(), &root) {
            Observed::Verified(reader, _) => reader,
            other => panic!("journal unavailable: {other:?}"),
        };
        let requests: i64 = reader
            .conn()
            .query_row(
                "SELECT count(*) FROM requests WHERE run_id=?1",
                [live.to_string()],
                |row| row.get(0),
            )
            .expect("requests");
        assert_eq!(
            requests,
            i64::from(!detach && !still_attached),
            "only a committed EOF queues a stop"
        );
        if !detach && !still_attached {
            let encoded: String = reader
                .conn()
                .query_row(
                    "SELECT action FROM requests WHERE run_id=?1",
                    [live.to_string()],
                    |row| row.get(0),
                )
                .expect("owner EOF request");
            let request: crate::run::Request =
                serde_json::from_str(&encoded).expect("typed request");
            assert_eq!(request.run, live);
            assert_eq!(&request.expected, handle.fence());
            assert!(matches!(request.action, crate::run::RequestAction::Stop));
        }
        drop(reader);
        if close_before_retry {
            for run in host.dispatcher.admission.runs.drain(..) {
                run.settle(&cleanup(&cx), &mut NoEffects)
                    .expect("test-owned run closes");
            }
        }
        let fresh = cleanup(&cx)
            .operation()
            .effects()
            .expect("fresh cleanup authority");
        if close_before_retry {
            assert_eq!(
                host.retry_cleanup(&fresh, &root)
                    .expect_err("compacted release stays unresolved")
                    .code,
                RefusalCode::Unknown
            );
            assert!(host.cleanup_pending());
            assert_eq!(
                host.unsettled_attachment
                    .as_ref()
                    .expect("same handle retained")
                    .run(),
                live
            );
        } else {
            host.retry_cleanup(&fresh, &root)
                .expect("same owner release retry");
            assert!(!host.cleanup_pending());
        }
        for run in host.dispatcher.admission.runs.drain(..) {
            run.settle(&cleanup(&cx), &mut NoEffects)
                .expect("test-owned run cleanup");
        }
    }
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
    let id = RequestId::new([request; 16]);
    let run_id = Ids::new(ids).next().expect("run id");
    let Reservation::Fresh(ticket) = run::reserve_resolved(
        cx,
        root,
        run::AdmissionKey {
            session,
            request: id,
        },
        Digest::of(b"invite"),
        run_id,
        &invocation(owner.clone()),
    )
    .expect("reserved") else {
        panic!("a fresh reservation is required");
    };
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
    (run, run_id)
}

#[test]
fn concurrent_recorded_admission_does_not_grant_the_loser_owner() {
    use std::sync::{mpsc, Arc, Barrier};
    struct RaceResolver {
        barrier: Arc<Barrier>,
        wait: Option<mpsc::Receiver<()>>,
    }
    impl super::Resolver for RaceResolver {
        fn resolve(
            &mut self,
            _: &EffectContext,
            _: &InvocationBody,
        ) -> Result<ResolvedInvocation, Refusal> {
            self.barrier.wait();
            if let Some(wait) = self.wait.take() {
                wait.recv_timeout(Duration::from_secs(10))
                    .map_err(|_| Refusal::unknown("race timed out"))?;
            }
            Ok(invocation(task("workspace-channel").resource()))
        }
    }
    struct RaceAdmission<'a> {
        root: &'a StateRoot,
        winner: Option<mpsc::Sender<()>>,
        done: Option<mpsc::Receiver<()>>,
        run: Option<Run<'a>>,
    }
    impl Admission for RaceAdmission<'_> {
        fn admitted_fence(&self, ticket: &AdmissionTicket) -> Option<run::Fence> {
            self.run
                .as_ref()
                .filter(|run| run.id() == ticket.run())
                .map(|run| run.fence().clone())
        }
        fn admit(
            &mut self,
            cx: &EffectContext,
            ticket: &AdmissionTicket,
            resolved: &ResolvedInvocation,
        ) -> Result<(), Refusal> {
            let Some(winner) = self.winner.take() else {
                return Err(Refusal::unknown("loser attempted admission"));
            };
            let run = Run::begin_admitted(
                cx,
                self.root,
                resolved,
                BeginOpts {
                    id: ticket.run(),
                    wait: Wait::FailFast,
                    acknowledge: vec![],
                    next: next_argv(),
                    custody: None,
                },
                &cleanup(cx),
                &mut NoEffects,
                ticket.clone(),
            )
            .map_err(refusal_from)?;
            self.run = Some(run);
            winner
                .send(())
                .map_err(|_| Refusal::unknown("race receiver closed"))?;
            self.done
                .take()
                .expect("winner owns completion receiver")
                .recv_timeout(Duration::from_secs(10))
                .map_err(|_| Refusal::unknown("race timed out"))?;
            Ok(())
        }
    }
    let (fixture, _, _, session) = journal("effigy-channel-recorded-race");
    let path = fixture.path().to_path_buf();
    let barrier = Arc::new(Barrier::new(2));
    let (admitted_tx, admitted_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let (winner, loser) = std::thread::scope(|scope| {
        let winner_path = path.clone();
        let winner_barrier = Arc::clone(&barrier);
        let winner = scope.spawn(move || {
            let cx = effects(Placement::Host);
            let root = StateRoot::effects(&cx, &winner_path).unwrap();
            let mut dispatcher = Dispatcher::new(
                RaceAdmission {
                    root: &root,
                    winner: Some(admitted_tx),
                    done: Some(done_rx),
                    run: None,
                },
                RaceResolver {
                    barrier: winner_barrier,
                    wait: None,
                },
                Observer::default(),
                Ids::new(&[178]),
            );
            let reply = dispatcher.invoke(
                &cx,
                &observe_context(),
                &root,
                session,
                RequestId::new([7; 16]),
                Digest::of(b"same race"),
                &invocation_body("build"),
            );
            let owns = dispatcher
                .admitted
                .contains(&RunId::new(7, [178; 10]).unwrap());
            if let Some(run) = dispatcher.admission.run.take() {
                run.settle(&cleanup(&cx), &mut NoEffects).unwrap();
            }
            (reply, owns)
        });
        let loser = scope.spawn(move || {
            let cx = effects(Placement::Host);
            let root = StateRoot::effects(&cx, &path).unwrap();
            let mut dispatcher = Dispatcher::new(
                RaceAdmission {
                    root: &root,
                    winner: None,
                    done: None,
                    run: None,
                },
                RaceResolver {
                    barrier,
                    wait: Some(admitted_rx),
                },
                Observer::default(),
                Ids::new(&[179]),
            );
            let reply = dispatcher.invoke(
                &cx,
                &observe_context(),
                &root,
                session,
                RequestId::new([7; 16]),
                Digest::of(b"same race"),
                &invocation_body("build"),
            );
            let owns = dispatcher
                .admitted
                .contains(&RunId::new(7, [178; 10]).unwrap());
            let expected = RunId::new(7, [178; 10]).unwrap();
            let attached = dispatcher.attach(
                &cx,
                &root,
                session,
                RequestId::new([8; 16]),
                &AttachBody {
                    run: expected,
                    streams: vec![StreamKind::Stdout],
                    cursor: None,
                    cursors: None,
                },
            );
            let secondary = attached
                .as_ref()
                .is_ok_and(|handle| handle.client() == Client::Secondary);
            let denied = dispatcher
                .detach(&cx, &root, session, &RunBody { run: expected })
                .is_err();
            let released = match attached {
                Ok(mut handle) => handle.release(&cx, &root, Release::Eof).is_ok(),
                Err(_) => false,
            };
            let _ = done_tx.send(());
            (reply, owns, secondary, denied, released)
        });
        (winner.join().unwrap(), loser.join().unwrap())
    });
    let expected = RunId::new(7, [178; 10]).unwrap();
    assert!(matches!(winner.0, Ok(AdmissionReply::Accepted(ref value)) if value.run == expected));
    assert!(
        winner.1,
        "fresh admission grants ownership to its own dispatcher"
    );
    assert!(matches!(loser.0, Ok(AdmissionReply::Accepted(ref value)) if value.run == expected));
    assert!(
        !loser.1,
        "a recorded admission must not grant ownership to the loser"
    );
    assert!(loser.2, "the loser attaches only as secondary");
    assert!(loser.3, "the loser cannot detach the owner");
    assert!(
        loser.4,
        "the loser releases its own secondary before the winner finishes"
    );
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
                    cursor: None,
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
    fn next_chunk(&mut self, _attachment: Option<&Attachment>) -> Option<StreamChunk> {
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
                    run_id: None,
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
                run_id: None,
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
                        run_id: None,
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
        .expect_err("an engine without an acknowledged owner cannot grant authority");
    assert_eq!(refusal.code, RefusalCode::Unknown);
    assert_eq!(dispatcher.admission.tickets.len(), 1);
    assert!(dispatcher.admitted_owners.is_empty());

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
    let id = RequestId::new([10; 16]);
    let digest = Digest::of(b"invite");
    let body = invocation_body("build");
    let (mut admitted, run_id) = admitted_run(&cx, &root, session, 10, &[81]);
    let reserved = Accepted {
        run: run_id,
        placement: Placement::Host,
    };

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
            "SELECT count(*) FROM runs WHERE run_id=?1",
            [run_id.to_string()],
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
    let refusal = dispatcher
        .invoke(
            &cx,
            &observe_context(),
            &root,
            session,
            id,
            digest,
            &invocation_body("build"),
        )
        .expect_err("an unacknowledged owner is refused after resolution");
    assert_eq!(refusal.code, RefusalCode::Unknown);
    assert_eq!(dispatcher.admission.tickets.len(), 1);
    assert_eq!(
        dispatcher.admission.tickets[0].run(),
        RunId::new(7, [31; 10]).unwrap()
    );
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
                    cursors: None,
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
    let (mut run, run_id) = admitted_run(&cx, &root, session, 11, &[91]);
    let accepted = Accepted {
        run: run_id,
        placement: Placement::Host,
    };
    connection.admitted.insert(run_id);
    connection.admitted_owners.insert(
        run_id,
        run::resume_owner(&cx, &root, session, run_id).expect("original owner"),
    );

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
                cursors: None,
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
                cursors: None,
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
                    run_id: None,
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
                        run_id: None,
                        op: "run:status".into(),
                        target: None,
                    }),
                    3,
                )
            }),
        ]),
        peer(),
    );
    // This synthetic admission never creates its Run row, so EOF cannot
    // establish owner cleanup. The real-admission integration covers success.
    let error = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect_err("a fabricated admission cannot settle owner EOF");
    assert_eq!(error.code, RefusalCode::Unknown);
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
            outcome: ResponseOutcome::Refused(Refusal {
                code: RefusalCode::Unknown,
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
                    run_id: None,
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
                    run_id: None,
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
                    run_id: None,
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
                    run_id: None,
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
                cursors: None,
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
    // The replaced secondary was released when it was replaced, and the newest
    // one is released with the connection that owned it: a refused or malformed
    // frame must not leave a terminal's slot unreclaimable, and the owner row is
    // never touched.
    assert_eq!(state(first.attachment), "released");
    assert_eq!(state(second.attachment), "released");
    let secondaries: i64 = reader
        .conn()
        .query_row(
            "SELECT count(*) FROM run_attachments WHERE run_id=?1 AND lifetime='secondary'",
            [first_run.to_string()],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(secondaries, 2, "the replace opened exactly one more row");
    let live_secondaries: i64 = reader
        .conn()
        .query_row(
            "SELECT count(*) FROM run_attachments WHERE run_id=?1 AND lifetime='secondary' AND state='attached'",
            [first_run.to_string()],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(
        live_secondaries, 0,
        "the closed connection left no attached secondary slot"
    );
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
                    cursors: None,
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
                cursors: None,
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
    assert!(host.attach.is_none(), "the detach cleared the stream");
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

/// One connection's terminal endpoint, as the attachment's owner implements it:
/// it records the input it was handed, keeps only the newest size, and reports
/// the retained position the cursor meets.
#[derive(Debug, Default)]
struct RecordingTerminal {
    input: Vec<Vec<u8>>,
    attachment_ids: Vec<AttachmentId>,
    size: Option<TerminalSize>,
    resizes: usize,
    chunks: VecDeque<StreamChunk>,
}
impl Terminal for RecordingTerminal {
    fn input(&mut self, attachment: &Attachment, bytes: &[u8]) -> Result<(), Refusal> {
        self.attachment_ids.push(attachment.id());
        self.input.push(bytes.to_vec());
        Ok(())
    }
    fn resize(&mut self, _attachment: &Attachment, size: TerminalSize) -> Result<(), Refusal> {
        self.resizes += 1;
        self.size = Some(size);
        Ok(())
    }
}
impl Streams for RecordingTerminal {
    fn resume(&mut self, _run: RunId, cursors: &Cursors) -> Option<StreamStart> {
        cursors
            .is_fresh()
            .then(|| StreamStart::from([(TERMINAL_STREAM, 0)]))
    }
    fn next_chunk(&mut self, _attachment: Option<&Attachment>) -> Option<StreamChunk> {
        self.chunks.pop_front()
    }
    fn input(&mut self, attachment: &Attachment, bytes: &[u8]) -> Result<(), Refusal> {
        Terminal::input(self, attachment, bytes)
    }
    fn resize(&mut self, attachment: &Attachment, size: TerminalSize) -> Result<(), Refusal> {
        Terminal::resize(self, attachment, size)
    }
}

fn attach_frame(
    session: Session,
    run: RunId,
    id: u8,
    streams: Vec<StreamKind>,
    cursor: Option<u64>,
) -> Frame {
    Frame::Request(request(
        session,
        RequestBody::Attach(AttachBody {
            run,
            streams,
            cursor,
            cursors: None,
        }),
        id,
    ))
}

/// An attach that reports the position it consumed on each stream id, which is
/// what a reconnecting client that read several retained logs reports.
fn attach_frame_cursors(
    session: Session,
    run: RunId,
    id: u8,
    streams: Vec<StreamKind>,
    cursor: Option<u64>,
    cursors: BTreeMap<u32, u64>,
) -> Frame {
    Frame::Request(request(
        session,
        RequestBody::Attach(AttachBody {
            run,
            streams,
            cursor,
            cursors: Some(cursors),
        }),
        id,
    ))
}

fn input_frame(attach: u8, seq: u64, bytes: &[u8]) -> StreamFrame {
    StreamFrame {
        id: RequestId::new([attach; 16]),
        stream_id: TERMINAL_STREAM,
        seq,
        kind: StreamKind::TerminalInput,
        payload: Some(encode_stream_payload(bytes)),
        rows: None,
        columns: None,
    }
}

fn resize_frame(attach: u8, seq: u64, rows: u16, columns: u16) -> StreamFrame {
    StreamFrame {
        id: RequestId::new([attach; 16]),
        stream_id: TERMINAL_STREAM,
        seq,
        kind: StreamKind::Resize,
        payload: None,
        rows: Some(rows),
        columns: Some(columns),
    }
}

/// Live stop requests recorded for one run. A refusal or a secondary's release
/// must leave this at zero.
fn stops(root: &StateRoot, run: RunId) -> i64 {
    let reader = match Store::read(&observe_context(), root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("root {other:?}"),
    };
    reader
        .conn()
        .query_row(
            "SELECT count(*) FROM requests WHERE run_id=?1 AND state='pending'",
            [run.to_string()],
            |row| row.get(0),
        )
        .expect("count")
}

/// Attach as a secondary, then send `frames` in order. The connection must
/// refuse and close before an invalid frame has a host effect.
fn refused_terminal_frames(name: &str, request: u8, frames: Vec<StreamFrame>) -> RecordingTerminal {
    let (_fixture, cx, root, session) = journal(name);
    let (_run, live) = admitted_run(&cx, &root, session, request, &[request]);
    let mut host = server(session, &[request, request]);
    let mut terminal = RecordingTerminal::default();
    let mut input = vec![
        Frame::Hello(hello()),
        Frame::Authenticate(capability()),
        attach_frame(session, live, 1, vec![StreamKind::TerminalOutput], None),
    ];
    input.extend(frames.into_iter().map(Frame::Stream));
    let mut pipe = Pipe::new(stream(&input), peer());
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut terminal)
        .expect_err("an invalid terminal frame closes the connection");
    assert_eq!(refusal.code, RefusalCode::InvalidRequest);
    assert_eq!(stops(&root, live), 0, "a refusal has no host effect");
    let written = written_frames(&pipe);
    assert!(
        matches!(written.last(), Some(Frame::Refuse(r)) if r.code == RefusalCode::InvalidRequest),
        "the client is told why the connection closed: {written:?}"
    );
    terminal
}

#[test]
fn output_sequence_is_monotonic_per_stream_across_kinds() {
    let mut outbound = Outbound::new();
    let id = RequestId::new([1; 16]);
    assert!(outbound
        .push_chunk(id, 0, StreamKind::Stdout, b"out")
        .is_empty());
    assert!(outbound
        .push_chunk(id, 0, StreamKind::Stderr, b"err")
        .is_empty());
    let frames = outbound.take();
    assert_eq!(
        frames.iter().map(|frame| frame.seq).collect::<Vec<_>>(),
        vec![0, 1],
        "§8.11 sequence is per stream id, not per (stream, kind)"
    );
    // The acknowledgement window is unchanged: one series, freed in order.
    assert_eq!(outbound.window().unacked(), 2);
    outbound.acknowledge(0, 0);
    assert_eq!(outbound.window().unacked(), 1);
    outbound.acknowledge(0, 1);
    assert_eq!(outbound.window().unacked(), 0);
    assert!(outbound
        .push_chunk(id, 0, StreamKind::TerminalOutput, b"next")
        .is_empty());
    assert_eq!(outbound.take()[0].seq, 2, "one series per stream id");
}

#[test]
fn terminal_input_and_resize_reach_the_attached_terminal_exactly_once() {
    let (_fixture, cx, root, session) = journal("effigy-channel-terminal-in");
    let (_run, live) = admitted_run(&cx, &root, session, 61, &[131]);
    let mut host = server(session, &[132]);
    let mut terminal = RecordingTerminal::default();
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::TerminalOutput], None),
            Frame::Stream(input_frame(1, 0, b"typed")),
            Frame::Stream(resize_frame(1, 1, 24, 80)),
            Frame::Stream(resize_frame(1, 2, 30, 100)),
            Frame::Stream(input_frame(1, 3, &[0x04])),
        ]),
        peer(),
    );
    let served = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut terminal)
        .expect("served");
    assert_eq!(served.streams, 4);
    assert!(served.closed);
    assert_eq!(
        terminal.input,
        vec![b"typed".to_vec(), vec![0x04]],
        "each input frame is delivered once, in order"
    );
    assert_eq!(terminal.resizes, 2);
    assert_eq!(
        terminal.size,
        Some(TerminalSize {
            rows: 30,
            columns: 100
        }),
        "the newest size wins"
    );
    let frames = written_frames(&pipe);
    let opened = frames
        .iter()
        .find_map(|frame| match frame {
            Frame::Response(Response {
                outcome: ResponseOutcome::Observed(value),
                ..
            }) => Some(serde_json::from_value::<AttachView>(value.clone()).expect("view")),
            _ => None,
        })
        .expect("the attach was answered");
    assert!(
        terminal
            .attachment_ids
            .iter()
            .all(|id| *id == opened.attachment),
        "input named this connection's own attachment"
    );
    // A secondary's EOF never stops the owner's run (§8.11).
    assert_eq!(stops(&root, live), 0);
}

#[test]
fn terminal_input_for_unknown_stream_is_refused() {
    let mut frame = input_frame(1, 0, b"typed");
    frame.stream_id = 999;
    let terminal = refused_terminal_frames("effigy-channel-input-unknown", 62, vec![frame]);
    assert!(terminal.input.is_empty(), "no input reached the terminal");
}

#[test]
fn terminal_input_for_another_request_is_refused() {
    let mut frame = input_frame(1, 0, b"typed");
    frame.id = RequestId::new([99; 16]);
    let terminal = refused_terminal_frames("effigy-channel-input-foreign", 63, vec![frame]);
    assert!(terminal.input.is_empty(), "no input reached the terminal");
}

#[test]
fn repeated_terminal_input_is_not_replayed() {
    let terminal = refused_terminal_frames(
        "effigy-channel-input-repeat",
        64,
        vec![input_frame(1, 0, b"typed"), input_frame(1, 0, b"typed")],
    );
    assert_eq!(
        terminal.input,
        vec![b"typed".to_vec()],
        "the delivered frame is not delivered again"
    );
}

#[test]
fn out_of_order_terminal_input_is_refused() {
    let terminal = refused_terminal_frames(
        "effigy-channel-input-ahead",
        65,
        vec![input_frame(1, 0, b"typed"), input_frame(1, 2, b"next")],
    );
    assert_eq!(
        terminal.input,
        vec![b"typed".to_vec()],
        "only the in-order frame reached the terminal"
    );
}

#[test]
fn a_malformed_or_outbound_terminal_frame_is_refused() {
    let mut missing = input_frame(1, 0, b"typed");
    missing.payload = None;
    let terminal = refused_terminal_frames("effigy-channel-input-shape", 66, vec![missing]);
    assert!(terminal.input.is_empty());

    let mut sized = input_frame(1, 0, b"typed");
    sized.rows = Some(24);
    let terminal = refused_terminal_frames("effigy-channel-input-sized", 67, vec![sized]);
    assert!(terminal.input.is_empty());

    let mut outbound = input_frame(1, 0, b"typed");
    outbound.kind = StreamKind::TerminalOutput;
    let terminal = refused_terminal_frames("effigy-channel-input-kind", 68, vec![outbound]);
    assert!(terminal.input.is_empty());

    let mut padded = resize_frame(1, 0, 24, 80);
    padded.payload = Some(encode_stream_payload(b"24x80"));
    let terminal = refused_terminal_frames("effigy-channel-resize-payload", 69, vec![padded]);
    assert_eq!(terminal.resizes, 0);
}

#[test]
fn terminal_frames_without_an_attachment_are_refused() {
    let (_fixture, cx, root, session) = journal("effigy-channel-input-unattached");
    let mut host = server(session, &[70]);
    let mut terminal = RecordingTerminal::default();
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            Frame::Stream(input_frame(1, 0, b"typed")),
        ]),
        peer(),
    );
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut terminal)
        .expect_err("a stream frame needs the client's own attachment");
    assert_eq!(refusal.code, RefusalCode::InvalidRequest);
    assert!(terminal.input.is_empty());
}

#[test]
fn resize_frames_are_sequenced_and_only_the_newest_size_is_kept() {
    let (_fixture, cx, root, session) = journal("effigy-channel-resize-burst");
    let (_run, live) = admitted_run(&cx, &root, session, 71, &[133]);
    let mut host = server(session, &[134]);
    let mut terminal = RecordingTerminal::default();
    let mut input = vec![
        Frame::Hello(hello()),
        Frame::Authenticate(capability()),
        attach_frame(session, live, 1, vec![StreamKind::TerminalOutput], None),
    ];
    let burst = 64u16;
    input.extend(
        (0..burst)
            .map(|seq| Frame::Stream(resize_frame(1, u64::from(seq), seq + 10, seq + 90)))
            .collect::<Vec<_>>(),
    );
    let mut pipe = Pipe::new(stream(&input), peer());
    host.serve(&mut pipe, &cx, &observe_context(), &root, &mut terminal)
        .expect("served");
    assert_eq!(
        terminal.resizes,
        usize::from(burst),
        "each resize was handed over as it arrived, with no queue behind it"
    );
    assert_eq!(
        terminal.size,
        Some(TerminalSize {
            rows: burst + 9,
            columns: burst + 89
        }),
        "the terminal keeps only the newest size"
    );
    let frames = written_frames(&pipe);
    assert!(
        !frames.iter().any(|frame| matches!(
            frame,
            Frame::Stream(StreamFrame {
                kind: StreamKind::Gap,
                ..
            })
        )),
        "a first attach reports no gap"
    );
}

#[test]
fn an_acknowledgement_is_only_for_a_stream_and_frame_this_connection_sent() {
    // A stream this connection never published.
    let foreign = StreamFrame {
        id: RequestId::new([1; 16]),
        stream_id: 7,
        seq: 0,
        kind: StreamKind::Ack,
        payload: None,
        rows: None,
        columns: None,
    };
    let _ = refused_terminal_frames("effigy-channel-ack-stream", 72, vec![foreign]);
    // A frame beyond the highest sequence this connection sent.
    let future = StreamFrame {
        id: RequestId::new([1; 16]),
        stream_id: TERMINAL_STREAM,
        seq: 1,
        kind: StreamKind::Ack,
        payload: None,
        rows: None,
        columns: None,
    };
    let (_fixture, cx, root, session) = journal("effigy-channel-ack-future");
    let (_run, live) = admitted_run(&cx, &root, session, 73, &[135]);
    let mut host = server(session, &[136]);
    let mut terminal = RecordingTerminal::default();
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::TerminalOutput], None),
            Frame::Stream(future),
        ]),
        peer(),
    );
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut terminal)
        .expect_err("an acknowledgement of an unsent frame refuses");
    assert_eq!(refusal.code, RefusalCode::InvalidRequest);
}

/// Write one captured segment for `run`'s `stream`, as the custody capture
/// writes it: the file name carries the absolute offset it starts at.
fn write_segment(
    root: &StateRoot,
    run: RunId,
    sequence: i64,
    stream: Stream,
    base: u64,
    bytes: &[u8],
) {
    use std::io::Write;
    let runs = root
        .family(Family::Runs)
        .expect("runs family")
        .expect("created family");
    let dir = runs
        .child(&run.to_string(), Trust::Private, true)
        .expect("run directory")
        .expect("created directory");
    let name = FileName::new(&format!("{sequence}.{}.{base}.log", stream.name())).expect("name");
    let (mut file, _) = dir
        .open_file(
            &name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            Some(0o600),
            Trust::Private,
        )
        .expect("segment")
        .expect("created segment");
    file.write_all(bytes).expect("segment bytes");
}

/// The kernel's own retained endpoint over `run`'s captured stdout, so a
/// cursor test reads real segments instead of a fake that returns a number.
fn retained_stdout(root: &StateRoot, run: RunId) -> RetainedStreams<RecordingTerminal> {
    retained_plan(root, run, vec![(1, Stream::Stdout)])
}

/// The kernel's retained endpoint over an explicit plan, so a proof can carry
/// more than one captured stream on one connection.
fn retained_plan(
    root: &StateRoot,
    run: RunId,
    plan: Vec<(i64, Stream)>,
) -> RetainedStreams<RecordingTerminal> {
    RetainedStreams::new(
        observe_context(),
        root.clone(),
        run,
        plan,
        RecordingTerminal::default(),
    )
}

fn stream_frames(frames: &[Frame]) -> Vec<&StreamFrame> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Stream(stream) => Some(stream),
            _ => None,
        })
        .collect()
}

/// Publish `count` frames on stream 0, then hand the connection these
/// acknowledgements. Returns the refusal code and whether the window is still
/// full when the connection closed.
fn acked_frames(count: usize, acks: Vec<StreamFrame>) -> (RefusalCode, usize) {
    let mut outbound = Outbound::new();
    let id = RequestId::new([9; 16]);
    for _ in 0..count {
        assert!(outbound
            .push_chunk(id, TERMINAL_STREAM, StreamKind::Stdout, b"x")
            .is_empty());
    }
    let _ = outbound.take();
    let _ = id;
    let mut accepted = None;
    for ack in acks {
        // The same rules the server applies to an acknowledgement frame.
        if ack.payload.is_some() || ack.rows.is_some() || ack.columns.is_some() {
            accepted = Some(RefusalCode::InvalidRequest);
            break;
        }
        if !outbound.acknowledge(ack.stream_id, ack.seq) {
            accepted = Some(RefusalCode::InvalidRequest);
            break;
        }
    }
    (
        accepted.unwrap_or(RefusalCode::InvalidRequest),
        outbound.window().unacked(),
    )
}

fn ack_frame_for(attach: u8, stream_id: u32, seq: u64) -> StreamFrame {
    StreamFrame {
        id: RequestId::new([attach; 16]),
        stream_id,
        seq,
        kind: StreamKind::Ack,
        payload: None,
        rows: None,
        columns: None,
    }
}

fn ack_frame(stream_id: u32, seq: u64) -> StreamFrame {
    StreamFrame {
        id: RequestId::new([9; 16]),
        stream_id,
        seq,
        kind: StreamKind::Ack,
        payload: None,
        rows: None,
        columns: None,
    }
}

#[test]
fn an_acknowledgement_needs_its_own_open_frame_and_no_payload() {
    // A malformed acknowledgement is refused and frees nothing.
    let mut padded = ack_frame(TERMINAL_STREAM, 0);
    padded.payload = Some(encode_stream_payload(b"0"));
    let (code, unacked) = acked_frames(4, vec![padded]);
    assert_eq!(code, RefusalCode::InvalidRequest);
    assert_eq!(unacked, 4, "a malformed acknowledgement frees no frame");

    // A duplicate and a stale acknowledgement are refused; only the frames at
    // or below a valid sequence are freed, once.
    assert_eq!(acked_frames(4, vec![ack_frame(TERMINAL_STREAM, 3)]).1, 0);
    let (code, unacked) = acked_frames(
        4,
        vec![ack_frame(TERMINAL_STREAM, 3), ack_frame(TERMINAL_STREAM, 3)],
    );
    assert_eq!(code, RefusalCode::InvalidRequest);
    assert_eq!(unacked, 0, "the duplicate freed nothing further");
    let (code, unacked) = acked_frames(
        4,
        vec![ack_frame(TERMINAL_STREAM, 3), ack_frame(TERMINAL_STREAM, 1)],
    );
    assert_eq!(code, RefusalCode::InvalidRequest);
    assert_eq!(unacked, 0, "a stale acknowledgement frees nothing further");

    // An acknowledgement for a stream this connection never published, or for
    // a resumed position it has not published itself, is refused.
    let (code, unacked) = acked_frames(4, vec![ack_frame(7, 0)]);
    assert_eq!(code, RefusalCode::InvalidRequest);
    assert_eq!(unacked, 4);
    let mut outbound = Outbound::new();
    outbound.resume(TERMINAL_STREAM, 40);
    assert!(
        !outbound.acknowledge(TERMINAL_STREAM, 39),
        "a resumed position is not a frame this connection published"
    );
    assert_eq!(outbound.window().unacked(), 0);
}

#[test]
fn an_acknowledgement_releases_its_own_streams_frames_between_other_streams() {
    let mut outbound = Outbound::new();
    let id = RequestId::new([4; 16]);
    // Two streams interleave on one connection: the acknowledgement window is
    // shared, but each stream's frames stay its own.
    for _ in 0..8 {
        assert!(outbound
            .push_chunk(id, 0, StreamKind::Stdout, b"a")
            .is_empty());
        assert!(outbound
            .push_chunk(id, 1, StreamKind::Stderr, b"b")
            .is_empty());
    }
    let _ = outbound.take();
    assert_eq!(outbound.window().unacked(), 16);
    assert!(outbound.acknowledge(1, 7));
    assert_eq!(
        outbound.window().unacked(),
        8,
        "stream 1's frames are released behind stream 0's, not blocked by them"
    );
    assert_eq!(outbound.window().queued(), 8);
    assert!(
        !outbound.acknowledge(1, 7),
        "the duplicate frees nothing and reports it"
    );
    assert_eq!(outbound.window().unacked(), 8);
    // The freed room is usable again, so a full window cannot stay full while
    // a stream's consumption goes unacknowledged by another stream's order.
    for index in 0..8 {
        assert!(outbound
            .push_chunk(id, 1, StreamKind::Stderr, b"c")
            .is_empty());
        assert!(outbound.window().unacked() <= 16, "frame {index}");
    }
    assert_eq!(outbound.window().unacked(), 16);
}

#[test]
fn a_released_attachment_cannot_deliver_input_or_resize() {
    let (_fixture, cx, root, session) = journal("effigy-channel-stale-input");
    let (_run, live) = admitted_run(&cx, &root, session, 80, &[149]);
    let mut host = server(session, &[150]);
    let attachment = run::attach_secondary(&cx, &root, session, live, AttachmentId::new([77; 16]))
        .expect("secondary");
    host.attach = Some(RequestId::new([1; 16]));
    let mut terminal = RecordingTerminal::default();
    // The row ends out of band — a liveness watch releasing a killed client, or
    // the run closing — while this connection still holds its handle.
    let mut released =
        run::resume_secondary(&cx, &root, session, live, AttachmentId::new([77; 16]))
            .expect("reopen the same row");
    released
        .release(&cx, &root, Release::Eof)
        .expect("release the row");
    let refusal = host
        .accept_stream(
            &cx,
            &root,
            &input_frame(1, 0, b"typed"),
            Some(&attachment),
            &mut terminal,
        )
        .expect_err("a released attachment cannot deliver input");
    assert_eq!(refusal.code, RefusalCode::InvalidRequest);
    let refusal = host
        .accept_stream(
            &cx,
            &root,
            &resize_frame(1, 1, 24, 80),
            Some(&attachment),
            &mut terminal,
        )
        .expect_err("a released attachment cannot resize");
    assert_eq!(refusal.code, RefusalCode::InvalidRequest);
    assert!(
        terminal.input.is_empty(),
        "no input reached the terminal after the row ended"
    );
    assert_eq!(terminal.resizes, 0, "no size was recorded");
    assert_eq!(stops(&root, live), 0, "the refusal had no host effect");
}

#[test]
fn replacing_an_attachment_rebinds_its_cursor_and_window() {
    let (_fixture, cx, root, session) = journal("effigy-channel-replace-window");
    let (_run, live) = admitted_run(&cx, &root, session, 81, &[151]);
    // The log holds offsets 0..80; the first attachment reads all of it, and the
    // replacement asks to resume at 40.
    let bytes: Vec<u8> = (0..80u32).map(|n| b'a' + (n % 26) as u8).collect();
    write_segment(&root, live, 1, Stream::Stdout, 0, &bytes);
    let mut host = server(session, &[152, 153]);
    let mut streams = retained_stdout(&root, live);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::Stdout], None),
            // Same connection, same run: a replacement with its own cursor.
            attach_frame(session, live, 2, vec![StreamKind::Stdout], Some(40)),
            // An acknowledgement for the replaced attachment's request must not
            // free anything under the new attachment.
            Frame::Stream(ack_frame_for(1, TERMINAL_STREAM, 0)),
        ]),
        peer(),
    );
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect_err("the replaced attachment's acknowledgement refuses");
    assert_eq!(refusal.code, RefusalCode::InvalidRequest);
    let frames = written_frames(&pipe);
    let published = |attach: u8| -> Vec<u64> {
        stream_frames(&frames)
            .into_iter()
            .filter(|stream| stream.id == RequestId::new([attach; 16]) && stream.kind.is_data())
            .map(|stream| stream.seq)
            .collect()
    };
    assert_eq!(published(1), vec![0], "the first attachment read from 0");
    assert_eq!(
        published(2),
        vec![40],
        "the replacement resumes at its own cursor instead of the previous numbering"
    );
    let opened = frames
        .iter()
        .filter(|frame| {
            matches!(
                frame,
                Frame::Response(Response {
                    outcome: ResponseOutcome::Observed(_),
                    ..
                })
            )
        })
        .count();
    assert_eq!(opened, 2, "both attachments opened: {frames:?}");
}

/// The published data frames of one attach request, in order.
fn published(root_frames: &[Frame], attach: u8) -> Vec<(u64, Vec<u8>)> {
    stream_frames(root_frames)
        .into_iter()
        .filter(|stream| stream.id == RequestId::new([attach; 16]) && stream.kind.is_data())
        .map(|stream| {
            (
                stream.seq,
                decode_stream_payload(stream.payload.as_deref().expect("payload")).expect("base64"),
            )
        })
        .collect()
}

#[test]
fn a_refused_replacement_leaves_the_current_stream_where_it_was() {
    let (_fixture, cx, root, session) = journal("effigy-channel-refused-replacement");
    let (_run, live) = admitted_run(&cx, &root, session, 82, &[155]);
    let bytes: Vec<u8> = (0..80u32).map(|n| b'a' + (n % 26) as u8).collect();
    write_segment(&root, live, 1, Stream::Stdout, 0, &bytes);
    // Fill the run's secondary slots so the replacement's new row is refused
    // after its cursor has been negotiated.
    let mut held = Vec::new();
    for index in 0..7u8 {
        held.push(
            run::attach_secondary(
                &cx,
                &root,
                session,
                live,
                AttachmentId::new([60 + index; 16]),
            )
            .expect("secondary"),
        );
    }
    let mut host = server(session, &[156, 157]);
    let mut streams = retained_stdout(&root, live);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::Stdout], None),
            // The connection's own row is the eighth, so this replacement is
            // refused by the kernel after negotiation.
            attach_frame(session, live, 2, vec![StreamKind::Stdout], Some(40)),
        ]),
        peer(),
    );
    let served = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("the connection keeps serving its working attachment");
    assert_eq!(served.responses, 2);
    assert!(served.closed);
    let frames = written_frames(&pipe);
    assert!(
        frames.iter().any(|frame| matches!(
            frame,
            Frame::Response(Response {
                outcome: ResponseOutcome::Refused(_),
                ..
            })
        )),
        "the replacement was refused: {frames:?}"
    );
    // The working attachment published the retained log once, from 0, and the
    // refused replacement's cursor never moved its reader back to 40.
    let first = published(&frames, 1);
    assert_eq!(
        first.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
        vec![0]
    );
    assert_eq!(first[0].1, bytes);
    assert!(
        published(&frames, 2).is_empty(),
        "the refused replacement published nothing"
    );
    assert_eq!(stops(&root, live), 0);
    drop(held);
}

#[test]
fn a_reconnect_resumes_every_retained_stream_at_its_own_position() {
    let (_fixture, cx, root, session) = journal("effigy-channel-two-streams");
    let (_run, live) = admitted_run(&cx, &root, session, 83, &[158]);
    let stdout_bytes: Vec<u8> = (0..40u32).map(|n| b'a' + (n % 26) as u8).collect();
    // Stderr is shorter and the client consumed far less of it: one cursor for
    // the whole connection would skip the bytes between 5 and 30.
    let stderr_bytes: Vec<u8> = (0..30u32).map(|n| b'A' + (n % 26) as u8).collect();
    write_segment(&root, live, 1, Stream::Stdout, 0, &stdout_bytes);
    write_segment(&root, live, 1, Stream::Stderr, 0, &stderr_bytes);
    let mut host = server(session, &[159, 160]);
    let mut streams = retained_plan(&root, live, vec![(1, Stream::Stdout), (1, Stream::Stderr)]);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            // The client read stdout through 20 and stderr only through 5.
            attach_frame_cursors(
                session,
                live,
                1,
                vec![StreamKind::Stdout, StreamKind::Stderr],
                None,
                BTreeMap::from([(0, 20), (1, 5)]),
            ),
            // A replacement on the same connection moves both streams to their
            // own positions again.
            attach_frame_cursors(
                session,
                live,
                2,
                vec![StreamKind::Stdout, StreamKind::Stderr],
                None,
                BTreeMap::from([(0, 30), (1, 10)]),
            ),
        ]),
        peer(),
    );
    host.serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("served");
    let frames = written_frames(&pipe);
    let one = |attach: u8, stream_id: u32| -> Vec<(u64, Vec<u8>)> {
        stream_frames(&frames)
            .into_iter()
            .filter(|stream| {
                stream.id == RequestId::new([attach; 16])
                    && stream.stream_id == stream_id
                    && stream.kind.is_data()
            })
            .map(|stream| {
                (
                    stream.seq,
                    decode_stream_payload(stream.payload.as_deref().expect("payload"))
                        .expect("base64"),
                )
            })
            .collect()
    };
    assert_eq!(
        one(1, 0),
        vec![(20, stdout_bytes[20..].to_vec())],
        "stdout resumed at its own consumed position"
    );
    assert_eq!(
        one(1, 1),
        vec![(5, stderr_bytes[5..].to_vec())],
        "stderr resumed at its own, shorter position instead of skipping to its end"
    );
    assert_eq!(
        one(2, 0),
        vec![(30, stdout_bytes[30..].to_vec())],
        "the replacement moved stdout to its own cursor"
    );
    assert_eq!(
        one(2, 1),
        vec![(10, stderr_bytes[10..].to_vec())],
        "the replacement moved stderr to its own cursor"
    );
    assert!(
        !stream_frames(&frames)
            .iter()
            .any(|stream| stream.kind.is_data() && stream.seq == 0),
        "no retained stream restarted at zero: {frames:?}"
    );
}

#[test]
fn a_multi_stream_resume_without_per_stream_positions_is_refused() {
    let (_fixture, cx, root, session) = journal("effigy-channel-one-cursor");
    let (_run, live) = admitted_run(&cx, &root, session, 84, &[161]);
    let stdout_bytes: Vec<u8> = (0..40u32).map(|n| b'a' + (n % 26) as u8).collect();
    let stderr_bytes: Vec<u8> = (0..30u32).map(|n| b'A' + (n % 26) as u8).collect();
    write_segment(&root, live, 1, Stream::Stdout, 0, &stdout_bytes);
    write_segment(&root, live, 1, Stream::Stderr, 0, &stderr_bytes);
    let mut host = server(session, &[162]);
    let mut streams = retained_plan(&root, live, vec![(1, Stream::Stdout), (1, Stream::Stderr)]);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            // One cursor cannot describe two independently consumed logs, so
            // the attach refuses instead of skipping or replaying one of them.
            attach_frame(
                session,
                live,
                1,
                vec![StreamKind::Stdout, StreamKind::Stderr],
                Some(20),
            ),
        ]),
        peer(),
    );
    host.serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("served");
    let frames = written_frames(&pipe);
    assert!(
        frames.iter().any(|frame| matches!(
            frame,
            Frame::Response(Response {
                outcome: ResponseOutcome::Refused(Refusal {
                    code: RefusalCode::InvalidRequest,
                    ..
                }),
                ..
            })
        )),
        "an unrepresentable multi-stream resume is refused: {frames:?}"
    );
    assert!(!stream_frames(&frames)
        .iter()
        .any(|stream| stream.kind.is_data()));
    let reader = match Store::read(&observe_context(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("root {other:?}"),
    };
    let secondaries: i64 = reader
        .conn()
        .query_row(
            "SELECT count(*) FROM run_attachments WHERE run_id=?1 AND lifetime='secondary'",
            [live.to_string()],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(secondaries, 0, "a refused resume opened no attachment");
}

#[test]
fn a_resume_without_a_retained_log_is_refused() {
    let (_fixture, cx, root, session) = journal("effigy-channel-no-log");
    let (_run, live) = admitted_run(&cx, &root, session, 85, &[163]);
    // Nothing is captured for this stream, so a later cursor cannot be
    // established: the attach is refused rather than opened as if it resumed.
    let mut host = server(session, &[164]);
    let mut streams = retained_stdout(&root, live);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::Stdout], Some(40)),
        ]),
        peer(),
    );
    host.serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("served");
    let frames = written_frames(&pipe);
    assert!(
        frames.iter().any(|frame| matches!(
            frame,
            Frame::Response(Response {
                outcome: ResponseOutcome::Refused(Refusal {
                    code: RefusalCode::InvalidRequest,
                    ..
                }),
                ..
            })
        )),
        "a cursor with no retained log is refused: {frames:?}"
    );

    // A first read of that same stream still opens: nothing was consumed.
    let mut host = server(session, &[165]);
    let mut streams = retained_stdout(&root, live);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 3, vec![StreamKind::Stdout], None),
        ]),
        peer(),
    );
    host.serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("served");
    assert!(
        written_frames(&pipe).iter().any(|frame| matches!(
            frame,
            Frame::Response(Response {
                outcome: ResponseOutcome::Observed(_),
                ..
            })
        )),
        "a fresh read of an uncaptured stream still opens"
    );
}

/// The recorded state of every attachment row for `run`, by lifetime.
fn attachment_states(root: &StateRoot, run: RunId) -> Vec<(String, String)> {
    let reader = match Store::read(&observe_context(), root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("root {other:?}"),
    };
    let mut statement = reader
        .conn()
        .prepare(
            "SELECT lifetime, state FROM run_attachments WHERE run_id=?1 ORDER BY lifetime, attachment_id",
        )
        .expect("query");
    statement
        .query_map([run.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("rows")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("states")
}

#[test]
fn a_refused_stream_frame_releases_the_secondary_row() {
    let (_fixture, cx, root, session) = journal("effigy-channel-refusal-release");
    let (_run, live) = admitted_run(&cx, &root, session, 90, &[170]);
    let mut host = server(session, &[171]);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::Stdout], None),
            // A terminal frame whose sequence is not the stream's next one.
            Frame::Stream(input_frame(1, 5, b"typed")),
        ]),
        peer(),
    );
    let mut terminal = RecordingTerminal::default();
    // The refusal closes the connection, and the row this connection opened is
    // released with it: a secondary's slot must stay reclaimable.
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut terminal)
        .expect_err("the invalid frame closes the connection");
    assert_eq!(refusal.code, RefusalCode::InvalidRequest);
    let states = attachment_states(&root, live);
    assert_eq!(
        states,
        vec![
            ("owner".to_string(), "attached".to_string()),
            ("secondary".to_string(), "released".to_string()),
        ],
        "the refused connection released its own row and left the owner's alone"
    );
    assert_eq!(stops(&root, live), 0, "a secondary never stops the owner");
    // The reclaimed slot takes a fresh terminal.
    let mut again = run::attach_secondary(&cx, &root, session, live, AttachmentId::new([91; 16]))
        .expect("the released slot is reclaimable");
    again.release(&cx, &root, Release::Eof).expect("release");
}

#[test]
fn an_empty_rotated_segment_still_reports_the_gap() {
    let (_fixture, cx, root, session) = journal("effigy-channel-empty-segment");
    let (_run, live) = admitted_run(&cx, &root, session, 91, &[172]);
    // Retention kept only an empty segment, starting at 100: the bytes the
    // client asks for are gone even though no data follows them.
    write_segment(&root, live, 1, Stream::Stdout, 100, b"");
    let mut host = server(session, &[173]);
    let mut streams = retained_stdout(&root, live);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::Stdout], Some(8)),
        ]),
        peer(),
    );
    host.serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("served");
    let frames = written_frames(&pipe);
    let gap = stream_frames(&frames)
        .into_iter()
        .find(|stream| stream.kind == StreamKind::Gap)
        .expect("the missing range is reported without any data to carry it");
    assert_eq!(gap.gap_range(), Some(StreamGap { from: 8, to: 100 }));
    assert_eq!(gap.seq, 8);
    assert!(
        !stream_frames(&frames)
            .iter()
            .any(|stream| stream.kind.is_data()),
        "an empty segment carries no data"
    );
}

#[test]
fn a_cursor_for_a_stream_the_endpoint_does_not_carry_is_refused() {
    let (_fixture, cx, root, session) = journal("effigy-channel-unknown-cursor");
    let (_run, live) = admitted_run(&cx, &root, session, 92, &[174]);
    let stdout_bytes: Vec<u8> = (0..40u32).map(|n| b'a' + (n % 26) as u8).collect();
    write_segment(&root, live, 1, Stream::Stdout, 0, &stdout_bytes);
    let mut host = server(session, &[175]);
    let mut streams = retained_stdout(&root, live);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            // Stream 999 is not carried by this endpoint: ignoring the request
            // would open as if the client's resume had been served.
            attach_frame_cursors(
                session,
                live,
                1,
                vec![StreamKind::Stdout],
                None,
                BTreeMap::from([(999, 40)]),
            ),
        ]),
        peer(),
    );
    host.serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("served");
    let frames = written_frames(&pipe);
    assert!(
        frames.iter().any(|frame| matches!(
            frame,
            Frame::Response(Response {
                outcome: ResponseOutcome::Refused(Refusal {
                    code: RefusalCode::InvalidRequest,
                    ..
                }),
                ..
            })
        )),
        "an unknown cursor id is refused: {frames:?}"
    );
    assert!(!stream_frames(&frames)
        .iter()
        .any(|stream| stream.kind.is_data()));
    assert!(
        !attachment_states(&root, live)
            .iter()
            .any(|(lifetime, _)| lifetime == "secondary"),
        "a refused cursor opened no attachment"
    );
}

#[test]
fn a_reconnect_resumes_at_the_retained_position_not_at_zero() {
    let (_fixture, cx, root, session) = journal("effigy-channel-resume");
    let (_run, live) = admitted_run(&cx, &root, session, 74, &[137]);
    // The log holds offsets 0..(40 + one frame + 40); the client stopped at 40,
    // so the first two frames cross a frame boundary of their own.
    let bytes: Vec<u8> = (0..(40 + MAX_STREAM_PAYLOAD + 40) as u32)
        .map(|n| b'a' + (n % 26) as u8)
        .collect();
    write_segment(&root, live, 1, Stream::Stdout, 0, &bytes);
    let mut host = server(session, &[138]);
    let mut streams = retained_stdout(&root, live);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::Stdout], Some(40)),
        ]),
        peer(),
    );
    host.serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("served");
    let frames = written_frames(&pipe);
    let data: Vec<&StreamFrame> = stream_frames(&frames)
        .into_iter()
        .filter(|stream| stream.kind == StreamKind::Stdout)
        .collect();
    assert_eq!(
        data.len(),
        2,
        "the retained bytes after the cursor are published once: {frames:?}"
    );
    // Each frame's sequence is its first byte's retained offset, so a chunk
    // boundary never turns the position into an ordinal.
    assert_eq!(
        data.iter().map(|stream| stream.seq).collect::<Vec<_>>(),
        vec![40, 40 + MAX_STREAM_PAYLOAD as u64],
        "output resumes at the retained position the client asked for"
    );
    let published: Vec<u8> = data
        .iter()
        .flat_map(|stream| {
            decode_stream_payload(stream.payload.as_deref().expect("payload")).expect("base64")
        })
        .collect();
    assert_eq!(
        published,
        bytes[40..],
        "the retained bytes from the cursor are exactly what was not read"
    );
    assert!(
        !stream_frames(&frames)
            .iter()
            .any(|stream| stream.kind == StreamKind::Gap),
        "a position the log still holds reports no gap"
    );
    // The endpoint re-checked this connection's own attachment before reading.
    assert!(matches!(
        Store::read(&observe_context(), &root),
        Observed::Verified(_, _)
    ));
}

#[test]
fn a_reconnect_reports_the_bounded_gap_the_retained_log_holds() {
    let (_fixture, cx, root, session) = journal("effigy-channel-gap");
    let (_run, live) = admitted_run(&cx, &root, session, 75, &[139]);
    // Offsets 0..100 rotated away; the kept segment starts at 100.
    write_segment(&root, live, 1, Stream::Stdout, 100, b"resumed");
    let mut host = server(session, &[140]);
    let mut streams = retained_stdout(&root, live);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::Stdout], Some(8)),
        ]),
        peer(),
    );
    host.serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("served");
    let frames = written_frames(&pipe);
    let published = stream_frames(&frames);
    let gap = published
        .iter()
        .position(|stream| stream.kind == StreamKind::Gap)
        .expect("the missing range was reported");
    let data = published
        .iter()
        .position(|stream| stream.kind.is_data())
        .expect("retained output was published");
    assert!(
        gap < data,
        "the gap precedes the data it stands for: {frames:?}"
    );
    let gap = published[gap];
    assert_eq!(gap.gap_range(), Some(StreamGap { from: 8, to: 100 }));
    // Distinct, strictly increasing positions: the gap names the first missing
    // offset and the data starts where the log resumes.
    assert_eq!(
        gap.seq, 8,
        "the gap names the first position it cannot give"
    );
    assert_eq!(published[data].seq, 100);
    assert!(gap.seq < published[data].seq);
    assert_eq!(
        decode_stream_payload(published[data].payload.as_deref().expect("payload"))
            .expect("base64"),
        b"resumed"
    );
}

#[test]
fn a_gap_and_the_data_after_it_keep_one_strictly_increasing_series() {
    let mut outbound = Outbound::new();
    let id = RequestId::new([7; 16]);
    outbound.resume(TERMINAL_STREAM, 8);
    outbound
        .push_retained(id, TERMINAL_STREAM, StreamKind::Stdout, 100, b"ab")
        .expect("retained frame");
    let frames = outbound.take();
    assert_eq!(
        frames.iter().map(|frame| frame.seq).collect::<Vec<_>>(),
        vec![8, 100],
        "a gap and the data that resumes it are distinct positions"
    );
    // A source that rewinds behind what this connection published refuses.
    assert!(outbound
        .push_retained(id, TERMINAL_STREAM, StreamKind::Stdout, 50, b"late")
        .is_err());
}

#[test]
fn a_cursor_past_the_retained_end_is_refused_without_an_attachment() {
    let (_fixture, cx, root, session) = journal("effigy-channel-cursor-future");
    let (_run, live) = admitted_run(&cx, &root, session, 76, &[141]);
    write_segment(&root, live, 1, Stream::Stdout, 0, b"abcde");
    let mut host = server(session, &[142]);
    let mut streams = retained_stdout(&root, live);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            // The log ends at 5, so 40 is a position it cannot resume from.
            attach_frame(session, live, 1, vec![StreamKind::Stdout], Some(40)),
        ]),
        peer(),
    );
    host.serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("served");
    let frames = written_frames(&pipe);
    assert!(
        frames.iter().any(|frame| matches!(
            frame,
            Frame::Response(Response {
                outcome: ResponseOutcome::Refused(Refusal {
                    code: RefusalCode::InvalidRequest,
                    ..
                }),
                ..
            })
        )),
        "a future cursor is refused: {frames:?}"
    );
    assert!(
        !stream_frames(&frames)
            .iter()
            .any(|stream| stream.kind.is_data()),
        "nothing was published as if the cursor resumed"
    );
    let reader = match Store::read(&observe_context(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("root {other:?}"),
    };
    let secondaries: i64 = reader
        .conn()
        .query_row(
            "SELECT count(*) FROM run_attachments WHERE run_id=?1 AND lifetime='secondary'",
            [live.to_string()],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(secondaries, 0, "a refused cursor opened no attachment");
}

#[test]
fn the_retained_endpoint_delivers_input_and_the_newest_size_to_the_owner() {
    let (_fixture, cx, root, session) = journal("effigy-channel-retained-input");
    let (_run, live) = admitted_run(&cx, &root, session, 79, &[147]);
    let mut host = server(session, &[148]);
    // The kernel's own endpoint, reading a real captured log and delivering to
    // the terminal owner exactly as lane B's front end would receive it.
    let mut streams = retained_stdout(&root, live);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::Stdout], None),
            Frame::Stream(input_frame(1, 0, b"typed")),
            Frame::Stream(resize_frame(1, 1, 24, 80)),
            Frame::Stream(resize_frame(1, 2, 30, 100)),
            Frame::Stream(input_frame(1, 3, b"\x04")),
        ]),
        peer(),
    );
    let served = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut streams)
        .expect("served");
    assert_eq!(served.streams, 4);
    assert!(served.closed);
    let terminal = streams.terminal_mut();
    assert_eq!(terminal.input, vec![b"typed".to_vec(), vec![0x04]]);
    assert_eq!(terminal.resizes, 2);
    assert_eq!(
        terminal.size,
        Some(TerminalSize {
            rows: 30,
            columns: 100
        })
    );
    assert_eq!(terminal.attachment_ids.len(), 2);
    // A secondary's EOF never stops the owner's run.
    assert_eq!(stops(&root, live), 0);
}

#[test]
fn a_source_with_nothing_retained_refuses_a_resume_cursor() {
    let (_fixture, cx, root, session) = journal("effigy-channel-nothing-retained");
    let (_run, live) = admitted_run(&cx, &root, session, 78, &[145]);
    let mut host = server(session, &[146]);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::TerminalOutput], Some(40)),
        ]),
        peer(),
    );
    host.serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect("served");
    let frames = written_frames(&pipe);
    assert!(
        frames.iter().any(|frame| matches!(
            frame,
            Frame::Response(Response {
                outcome: ResponseOutcome::Refused(Refusal {
                    code: RefusalCode::InvalidRequest,
                    ..
                }),
                ..
            })
        )),
        "a cursor a source did not retain is refused: {frames:?}"
    );
    assert!(!stream_frames(&frames)
        .iter()
        .any(|stream| stream.kind.is_data()));
}

#[test]
fn a_resize_with_no_terminal_owner_is_refused_not_silently_applied() {
    let (_fixture, cx, root, session) = journal("effigy-channel-resize-no-terminal");
    let (_run, live) = admitted_run(&cx, &root, session, 77, &[143]);
    let mut host = server(session, &[144]);
    let mut pipe = Pipe::new(
        stream(&[
            Frame::Hello(hello()),
            Frame::Authenticate(capability()),
            attach_frame(session, live, 1, vec![StreamKind::Stdout], None),
            Frame::Stream(resize_frame(1, 0, 24, 80)),
        ]),
        peer(),
    );
    let refusal = host
        .serve(&mut pipe, &cx, &observe_context(), &root, &mut NoStreams)
        .expect_err("a host with no terminal cannot resize");
    assert_eq!(refusal.code, RefusalCode::Unavailable);
    assert_eq!(stops(&root, live), 0);
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

#[test]
fn observe_run_identity_wire_is_strict_and_preserves_legacy_list() {
    let legacy = serde_json::json!({"op":"runs:list", "target":null});
    let list: ObserveBody = serde_json::from_value(legacy.clone()).unwrap();
    assert!(list.check().is_ok());
    assert_eq!(serde_json::to_value(&list).unwrap(), legacy);
    assert!(serde_json::from_value::<ObserveBody>(
        serde_json::json!({"op":"runs:list", "target":null, "extra":1})
    )
    .is_err());
    let id = RunId::new(42, [1; 10]).unwrap().to_string();
    let mut status = ObserveBody {
        op: "runs:status".into(),
        target: None,
        run_id: None,
    };
    assert!(status.check().is_err());
    status.run_id = Some("not-an-id".into());
    assert!(status.check().is_err());
    status.run_id = Some(id.clone());
    assert!(status.check().is_ok());
    assert_eq!(serde_json::to_value(&status).unwrap()["run_id"], id);
    status.op = "runs:list".into();
    assert!(status.check().is_err());
}
