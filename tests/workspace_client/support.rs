use effigy_kernel::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, EffectContext, Limits, ObserveContext,
    OperationContext, Placement,
};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{
    CheckoutId, CheckoutKind, Digest, ExecutionTarget, Generation, InvocationSpec,
    ResolvedInvocation, ResourceId, Route, RunId, Surface, TaskId,
};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::Invocation;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{
    AdmissionTicket, BeginOpts, Effect, Fence, Reconciled, RequestId, Run, Session,
    SettlementAdapter,
};
use effigy_kernel::store::Store;
use effigy_kernel::workspace_channel::{self as channel, *};
use effigy_kernel::workspace_session::SessionRef;
use effigy_test_support::process::{FixtureCommandExt, OwnedChild};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub fn context(placement: Placement) -> EffectContext {
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
        placement,
    )
    .effects()
    .expect("test effect context")
}

pub fn cleanup(cx: &EffectContext) -> CleanupContext {
    cx.operation()
        .cleanup(Duration::from_secs(10))
        .expect("cleanup budget")
}

pub fn id(byte: u8) -> RunId {
    RunId::new(1, [byte; 10]).expect("fixed test run identity")
}

pub fn checkout() -> CheckoutId {
    CheckoutId::new(CheckoutKind::EphemeralClone, [44; 16])
}

pub fn task(name: &str) -> TaskId {
    TaskId::new(checkout(), "", Surface::Published, name, None).expect("test task")
}

pub fn resolved(name: &str) -> ResolvedInvocation {
    ResolvedInvocation::new(InvocationSpec {
        task: task(name),
        route: Route::Task,
        argv: vec!["safe-argument".into()],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner: task(name).resource(),
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .expect("host-resolved invocation")
}

pub fn opts(run: RunId) -> BeginOpts {
    BeginOpts {
        id: run,
        wait: Wait::FailFast,
        acknowledge: vec![],
        custody: None,
        next: Invocation::validated(vec!["recover".into()], |args| args == ["recover"])
            .expect("fixed recovery invocation"),
    }
}

/// No fake engine has planned an effect. Any attempted cleanup effect is a bug,
/// including a process signal, container stop, or a state/vault operation.
pub struct NoEffects;
impl SettlementAdapter for NoEffects {
    fn observe(&mut self, effect: &Effect, _: &CleanupContext) -> Observed<Fence> {
        panic!("unexpected cleanup observation: {effect:?}")
    }
    fn release(&mut self, effect: &Effect, _: &CleanupContext) -> Observed<()> {
        panic!("unexpected cleanup release: {effect:?}")
    }
    fn reconcile(&mut self, effect: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        panic!("unexpected cleanup reconciliation: {effect:?}")
    }
}

/// Everything lives below a fresh fixture. The sentinel child belongs to the
/// harness, never to an admitted run; even owner loss must leave it running.
pub struct Host {
    pub cx: EffectContext,
    pub root: StateRoot,
    pub fixture: effigy_test_support::Fixture,
    sentinel: RefCell<OwnedChild>,
}
impl Host {
    pub fn new() -> Self {
        let fixture = effigy_test_support::Fixture::new("workspace-matrix-").expect("fixture");
        let cx = context(Placement::Host);
        let root = StateRoot::effects(&cx, fixture.path()).expect("private state root");
        std::fs::write(
            fixture.path().join("foreign-container"),
            b"foreign-container:boot:17",
        )
        .expect("fake foreign container sentinel");
        let sentinel = Command::new("/bin/sleep")
            .arg("180")
            .current_dir(fixture.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn_owned()
            .expect("foreign process sentinel");
        Self {
            cx,
            root,
            fixture,
            sentinel: RefCell::new(sentinel),
        }
    }
    pub fn observe(&self) -> ObserveContext {
        self.cx.operation().observe().expect("observation budget")
    }
    pub fn session(&self) -> (Run<'_>, SessionRef) {
        let environment = ResourceId::environment(checkout(), "matrix").expect("environment");
        let mut store = Store::open(&self.cx, &self.root).expect("private store");
        let generation = store
            .txn(&self.cx, |txn| {
                txn.resource(&environment, "environment")?;
                let generation = txn.next_generation(&environment)?;
                Ok(generation)
            })
            .expect("fake active environment");
        // Only this fresh fixture's environment-engine fact is seeded directly.
        let database =
            rusqlite::Connection::open(self.root.path().join(effigy_kernel::store::DB_NAME))
                .expect("fixture database");
        database
            .execute(
                "UPDATE resources SET state='active',last_applied=?2 WHERE name=?1",
                rusqlite::params![environment.name().to_string(), generation.get().to_string()],
            )
            .expect("seed active fake environment");
        SessionRef::admit(
            &self.cx,
            &self.root,
            &resolved("session"),
            opts(id(200)),
            &cleanup(&self.cx),
            &mut NoEffects,
            &Fence {
                resource: environment,
                generation,
            },
            effigy_kernel::supervise::Placement::Detached,
        )
        .expect("real Detached session")
    }
    pub fn assert_foreign(&self) {
        assert!(
            self.sentinel
                .borrow_mut()
                .try_wait()
                .expect("sentinel status")
                .is_none(),
            "cleanup stopped the foreign process sentinel"
        );
        assert_eq!(
            std::fs::read(self.fixture.path().join("foreign-container")).expect("sentinel"),
            b"foreign-container:boot:17",
            "cleanup changed the foreign container sentinel"
        );
    }
    pub fn stops(&self, run: RunId) -> usize {
        let count = self
            .database()
            .query_row(
                "SELECT count(*) FROM requests WHERE run_id=?1 AND state='pending'",
                [run.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .expect("stop requests");
        usize::try_from(count).expect("nonnegative bounded request count")
    }
    pub fn database(&self) -> rusqlite::Connection {
        rusqlite::Connection::open_with_flags(
            self.root.path().join(effigy_kernel::store::DB_NAME),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("read-only fixture database")
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let child = self.sentinel.get_mut();
        let pid = child.id();
        // Only the process we started. OwnedChild also guards its process group.
        let _ = child.kill();
        let result = child.wait();
        eprintln!("workspace matrix sentinel pid={pid} exit={result:?}");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Resolve(InvocationBody),
    Entropy,
    Admit(RunId, serde_json::Value),
    Observe(ObserveBody),
}

#[derive(Default)]
pub struct Ledger {
    expected: VecDeque<Call>,
    pub calls: Vec<Call>,
}
impl Ledger {
    pub fn expect(&mut self, calls: impl IntoIterator<Item = Call>) {
        self.expected.extend(calls);
    }
    fn record(&mut self, call: Call) {
        assert_eq!(
            self.expected.pop_front().as_ref(),
            Some(&call),
            "unexpected host request/effect; strict fake refuses unscripted calls"
        );
        self.calls.push(call);
    }
    pub fn done(&self) {
        assert!(
            self.expected.is_empty(),
            "a planned host assertion was not reached"
        );
    }
}
pub type SharedLedger = Rc<RefCell<Ledger>>;
pub type Runs<'a> = Rc<RefCell<Vec<Run<'a>>>>;

pub struct Engine<'a> {
    pub host: &'a Host,
    pub ledger: SharedLedger,
    pub runs: Runs<'a>,
}
impl Admission for Engine<'_> {
    fn admitted_fence(&self, ticket: &AdmissionTicket) -> Option<effigy_kernel::run::Fence> {
        self.runs
            .borrow()
            .iter()
            .find(|run| run.id() == ticket.run())
            .map(|run| run.fence().clone())
    }
    fn admit(
        &mut self,
        cx: &EffectContext,
        ticket: &AdmissionTicket,
        plan: &ResolvedInvocation,
    ) -> Result<(), Refusal> {
        let captured =
            serde_json::to_value(plan).expect("capture complete host invocation including argv");
        assert!(
            !captured.to_string().contains(&"ab".repeat(32)),
            "capability leaked into admitted argv or plan"
        );
        self.ledger
            .borrow_mut()
            .record(Call::Admit(ticket.run(), captured));
        let run = Run::begin_admitted(
            cx,
            &self.host.root,
            plan,
            opts(ticket.run()),
            &cleanup(cx),
            &mut NoEffects,
            ticket.clone(),
        )
        .map_err(|_| Refusal::unavailable("fake engine admission failed"))?;
        self.runs.borrow_mut().push(run);
        Ok(())
    }
}
pub struct Resolve(pub SharedLedger);
impl channel::Resolver for Resolve {
    fn resolve(
        &mut self,
        _: &EffectContext,
        body: &InvocationBody,
    ) -> Result<ResolvedInvocation, Refusal> {
        self.0.borrow_mut().record(Call::Resolve(body.clone()));
        Ok(resolved("child"))
    }
}
pub struct Observe(pub SharedLedger);
impl Observer for Observe {
    fn observe(
        &mut self,
        _: &ObserveContext,
        body: &ObserveBody,
    ) -> Result<serde_json::Value, Refusal> {
        self.0.borrow_mut().record(Call::Observe(body.clone()));
        Ok(serde_json::json!({"schema":"effigy.command.v1", "result":{"status":"observed"}}))
    }
}
pub struct Ids(pub SharedLedger);
impl RunIds for Ids {
    fn next(&mut self) -> Result<RunId, Refusal> {
        self.0.borrow_mut().record(Call::Entropy);
        Ok(id(10))
    }
}
pub type TestServer<'a> = Server<Engine<'a>, Resolve, Observe, Ids>;
pub fn capability() -> Secret {
    Secret::new([0xab; 32])
}
pub fn pin() -> Digest {
    Digest::of(b"matrix pinned TLS identity")
}
pub fn hello(session: Session) -> Hello {
    Hello {
        capabilities: Vec::new(),
        schema: SCHEMA.into(),
        protocol_versions: vec![1],
        effigy_version: "0.15.0".into(),
        binary_digest: Digest::of(b"matrix pinned artifact"),
        context_generation: Generation::new(3),
        session_run_id: session.run,
    }
}
pub fn server<'a>(
    host: &'a Host,
    session: Session,
    ledger: SharedLedger,
    runs: Runs<'a>,
    operations: BTreeSet<Method>,
) -> TestServer<'a> {
    let hello = hello(session);
    let mut grants = GrantBook::new();
    grants.issue(
        capability(),
        GrantBinding {
            host_user: "operator".into(),
            checkout: "fixture".into(),
            session,
            environment_generation: Generation::new(4),
            container_id: "full-container-id".into(),
            vm_boot_id: "boot-1".into(),
        },
        operations,
        [Target::Host].into_iter().collect(),
    );
    Server::new(
        Negotiation {
            context_generation: hello.context_generation,
            session_run_id: session.run,
            artifact: HostArtifact {
                effigy_version: hello.effigy_version,
                binary_digest: hello.binary_digest,
            },
        },
        SessionFence {
            context_generation: Generation::new(3),
            environment_generation: Generation::new(4),
            session,
            container_id: "full-container-id".into(),
            vm_boot_id: "boot-1".into(),
        },
        pin(),
        grants,
        Dispatcher::new(
            Engine {
                host,
                ledger: ledger.clone(),
                runs,
            },
            Resolve(ledger.clone()),
            Observe(ledger.clone()),
            Ids(ledger),
        ),
    )
}
pub fn body() -> InvocationBody {
    InvocationBody {
        selector: Selector::Task("build".into()),
        cwd: "packages/app".into(),
        target: Target::Host,
        args: vec!["--plan".into()],
        output: OutputMode::Json,
    }
}
pub fn request(session: Session, byte: u8, body: RequestBody) -> Request {
    Request {
        id: RequestId::new([byte; 16]),
        context_generation: Generation::new(3),
        environment_generation: Generation::new(4),
        session_run_id: session.run,
        container_id: "full-container-id".into(),
        vm_boot_id: "boot-1".into(),
        body,
    }
}
pub fn authenticated(session: Session) -> Vec<Frame> {
    vec![
        Frame::Hello(hello(session)),
        Frame::Authenticate(capability()),
    ]
}

type ResponseObserver = Box<dyn FnMut(&Response)>;
/// Partial reads are normal. Captures raw requests and every host response.
pub struct Pipe {
    pub input: Vec<u8>,
    pub read: usize,
    pub output: Vec<u8>,
    pub peer: Option<ServerIdentity>,
    pub on_response: Option<ResponseObserver>,
}
impl Pipe {
    pub fn new(frames: &[Frame]) -> Self {
        let mut pipe = Self {
            input: vec![],
            read: 0,
            output: vec![],
            peer: Some(ServerIdentity { digest: pin() }),
            on_response: None,
        };
        for frame in frames {
            pipe.raw(&encode_frame(frame).expect("fixture wire frame"));
        }
        pipe
    }
    pub fn raw(&mut self, bytes: &[u8]) {
        self.input
            .extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        self.input.extend_from_slice(bytes);
    }
    pub fn frames(&self) -> Vec<Frame> {
        let mut pipe = Self {
            input: self.output.clone(),
            read: 0,
            output: vec![],
            peer: None,
            on_response: None,
        };
        let mut frames = vec![];
        while let Some(bytes) = read_frame(&mut pipe).expect("complete response prefix/body") {
            frames.push(decode_frame(&bytes).expect("valid response frame"));
        }
        frames
    }
}
impl Transport for Pipe {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let size = bytes.len().min(3).min(self.input.len() - self.read);
        bytes[..size].copy_from_slice(&self.input[self.read..self.read + size]);
        self.read += size;
        Ok(size)
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.output.extend_from_slice(bytes);
        if let Some(observer) = &mut self.on_response {
            let frame = decode_frame(bytes.get(4..).expect("framed host write"))
                .expect("host wrote a valid frame");
            if let Frame::Response(response) = frame {
                observer(&response);
            }
        }
        Ok(())
    }
    fn verified_peer(&self) -> Option<ServerIdentity> {
        self.peer.clone()
    }
}
pub fn refused(pipe: &Pipe, code: RefusalCode) {
    assert!(
        pipe.frames().iter().any(|frame| matches!(frame,
        Frame::Refuse(r) | Frame::Response(Response { outcome: ResponseOutcome::Refused(r), .. })
            if r.code == code)),
        "intended refusal assertion not reached: expected {code:?}, got {:?}",
        pipe.frames()
    );
}
pub fn settle(host: &Host, runs: &Runs<'_>) {
    for run in runs.borrow_mut().drain(..).rev() {
        run.settle(&cleanup(&host.cx), &mut NoEffects)
            .expect("settle only owned fake run");
        host.assert_foreign();
    }
}
