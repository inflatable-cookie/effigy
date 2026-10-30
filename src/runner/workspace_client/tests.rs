//! §8.11 workspace-client proofs (effigy#452): the guest dispatch, the pinned
//! context, the client's refusals and replay, and the host side as lane A's
//! real server over in-memory transports.
//!
//! Every host engine seam is a strict fake: resolution, admission and
//! observation record what the guest asked for, and the kernel's own journal
//! records the reservation and the admission. Nothing here starts a container,
//! a supervisor or a real channel listener.

use super::context::{
    context_path, sha256_text, snapshot_path, GuestBinaryIdentity, WorkspaceActivationIdentity,
    WorkspaceBackendDescriptor, WorkspaceContextInput, WorkspaceEnvironmentIdentity,
    WorkspaceHostIdentity, WorkspacePinnedSource, CHANNEL_PROTOCOL_VERSION, COMPOSED_MANIFEST_KIND,
};
use super::*;
use crate::runner::workspace_client::guest_fake::FakeGuest;
use effigy_cli::{parse_command, Command};
use std::collections::{BTreeMap, VecDeque};

use std::io;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const GENERATION: u64 = 3;
const ENVIRONMENT_GENERATION: &str = "4";
const CONTAINER_ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const VM_BOOT: &str = "b007-1";
const SESSION_MARKER: u8 = 200;
const GUEST_CHECKOUT: &str = "/workspace";
const MANIFEST: &[u8] = b"[tasks]\nbuild = \"cargo build\"\nseed = \"cargo run -- seed\"\n";

fn running_binary() -> RunningBinary {
    RunningBinary::observe().expect("the test binary's own identity")
}

fn session_run() -> RunId {
    RunId::new(1, [SESSION_MARKER; 10]).expect("run id")
}

fn container() -> WorkspaceContainerIdentity {
    WorkspaceContainerIdentity {
        container_id: CONTAINER_ID.to_owned(),
        vm_boot_id: VM_BOOT.to_owned(),
        platform: "x86_64".to_owned(),
        libc: "gnu".to_owned(),
    }
}

fn ready_channel() -> context::WorkspaceChannelLocator {
    context::WorkspaceChannelLocator {
        endpoint: Some("unix:///run/effigy/channel".to_owned()),
        server_identity: Some(format!("sha256:{}", server_identity().hex())),
    }
}

fn manifest_pin() -> WorkspacePinnedSource {
    WorkspacePinnedSource {
        kind: COMPOSED_MANIFEST_KIND.to_owned(),
        name: "composed".to_owned(),
        digest: sha256_text(MANIFEST),
        snapshot: Some(snapshot_path(&sha256_text(MANIFEST))),
    }
}

fn workspace_context(
    session: Option<RunId>,
    channel: context::WorkspaceChannelLocator,
) -> WorkspaceContext {
    let binary = running_binary();
    WorkspaceContext::new(WorkspaceContextInput {
        generation: GENERATION,
        host: WorkspaceHostIdentity {
            version: binary.version.clone(),
            protocol: CHANNEL_PROTOCOL_VERSION,
        },
        binary: GuestBinaryIdentity {
            version: binary.version,
            digest: binary.digest,
            protocol: CHANNEL_PROTOCOL_VERSION,
        },
        activation: WorkspaceActivationIdentity {
            host_user: "operator".to_owned(),
            checkout_id: "checkout".to_owned(),
            environment: WorkspaceEnvironmentIdentity {
                resource_id: "env".to_owned(),
                generation: ENVIRONMENT_GENERATION.to_owned(),
            },
            container: container(),
        },
        session_run_id: session.map(|run| run.to_string()),
        session_generation: session.map(|_| 1),
        paths: context::WorkspacePathMap {
            host_checkout: "/Users/operator/app".to_owned(),
            guest_checkout: GUEST_CHECKOUT.to_owned(),
            workdir: GUEST_CHECKOUT.to_owned(),
            user: "app".to_owned(),
            home: "/home/app".to_owned(),
        },
        backend: WorkspaceBackendDescriptor {
            kind: "docker".to_owned(),
            profile: "default".to_owned(),
            runtime: None,
            vm_type: None,
        },
        sources: vec![manifest_pin()],
        routes: Vec::new(),
        channel,
    })
}

fn published(context: &WorkspaceContext) -> PublishedWorkspaceContext {
    PublishedWorkspaceContext {
        generation: context.generation,
        context: context.clone(),
    }
}

/// Publish one context generation, its manifest snapshot and (optionally) a
/// capability into a fake guest, exactly as Prepare would.
fn publish(guest: &mut FakeGuest, context: &WorkspaceContext, capability: Option<&Secret>) {
    guest.put_file(
        context::CONTEXT_CURRENT_PATH,
        context.generation.to_string(),
        0o644,
    );
    guest.put_file(
        &context_path(context.generation),
        context.render().expect("rendered context"),
        0o444,
    );
    guest.put_file(&snapshot_path(&sha256_text(MANIFEST)), MANIFEST, 0o444);
    if let Some(capability) = capability {
        guest.put_file(&client_capability_path(), secret_text(capability), 0o600);
    }
}

/// The hex form a capability is stored and presented in.
fn secret_text(secret: &Secret) -> String {
    serde_json::to_value(secret)
        .expect("a secret renders")
        .as_str()
        .expect("a secret renders as text")
        .to_owned()
}

fn capability() -> Secret {
    Secret::new([0xab; 32])
}

#[derive(Default)]
struct FakeCheckout {
    files: BTreeMap<String, Vec<u8>>,
    reads: usize,
}

impl FakeCheckout {
    fn with_file(relative: &str, bytes: &[u8]) -> Self {
        let mut files = BTreeMap::new();
        files.insert(relative.to_owned(), bytes.to_vec());
        Self { files, reads: 0 }
    }
}

impl CheckoutReader for FakeCheckout {
    fn read_checkout_file(&mut self, relative: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        self.reads += 1;
        Ok(self.files.get(relative).cloned())
    }
}

#[derive(Debug)]
struct ObservedContainer(WorkspaceContainerIdentity);

impl GuestIdentity for ObservedContainer {
    fn container(&mut self) -> Result<WorkspaceContainerIdentity, RunnerError> {
        Ok(self.0.clone())
    }
}

/// A proof that a local read never opens a connection.
struct NoChannel;

impl ChannelOpener for NoChannel {
    fn open(
        &mut self,
        _: &VerifiedWorkspaceContext,
        _: &LiveGuestRun,
        _: GuestAuth,
    ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
        panic!("a local read must not open the workspace channel")
    }
}

struct RefusingChannel {
    code: &'static str,
    message: String,
    opened: usize,
}

impl ChannelOpener for RefusingChannel {
    fn open(
        &mut self,
        _: &VerifiedWorkspaceContext,
        _: &LiveGuestRun,
        _: GuestAuth,
    ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
        self.opened += 1;
        Err(ChannelError::Refused {
            code: self.code,
            message: self.message.clone(),
        })
    }
}

fn parsed(words: &[&str]) -> Command {
    parse_command(
        words
            .iter()
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>(),
    )
    .expect("a parsable command")
}

#[test]
fn a_pinned_task_list_is_read_from_the_context_without_a_channel() {
    let context = workspace_context(Some(session_run()), ready_channel());
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, None);
    let before = guest.tree_digest();
    let mut checkout = FakeCheckout::default();
    let mut identity = ObservedContainer(container());

    let rendered = dispatch_in(
        &parsed(&["tasks"]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut identity,
        &mut NoChannel,
        Path::new(GUEST_CHECKOUT),
    )
    .expect("the pinned task list");

    assert_eq!(rendered, "build\nseed");
    assert_eq!(guest.tree_digest(), before, "a local read writes nothing");
    assert_eq!(checkout.reads, 0, "no checkout read for a pinned snapshot");
}

#[test]
fn a_stale_generation_refuses_before_any_read() {
    let context = workspace_context(Some(session_run()), ready_channel());
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, None);
    let before = guest.tree_digest();
    let mut stale = published(&context);
    stale.generation = GENERATION - 1;
    let mut checkout = FakeCheckout::default();
    let mut identity = ObservedContainer(container());

    let error = dispatch_in(
        &parsed(&["tasks"]),
        &stale,
        &mut guest,
        &mut checkout,
        &mut identity,
        &mut NoChannel,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("a stale generation refuses");

    assert!(error.to_string().contains("stale"), "{error}");
    assert_eq!(guest.tree_digest(), before);
}

#[test]
fn an_unprepared_workspace_is_not_a_guest_and_prepares_nothing() {
    let mut guest = FakeGuest::new();
    let before = guest.tree_digest();
    assert!(matches!(
        detect(&mut guest).expect("detection"),
        GuestPlacement::Host
    ));
    assert_eq!(guest.tree_digest(), before, "hybrid mode writes nothing");
}

#[test]
fn a_container_guest_classifies_its_commands_by_coverage() {
    assert_eq!(
        classify(&parsed(&["tasks"])),
        GuestOperation::Local(LocalRead::Tasks)
    );
    assert_eq!(
        classify(&parsed(&["version"])),
        GuestOperation::Local(LocalRead::Version)
    );
    assert_eq!(
        classify(&parsed(&["doctor"])),
        GuestOperation::Observe {
            op: "doctor".to_owned(),
            target: Target::Host,
        }
    );
    assert_eq!(
        classify(&parsed(&["container", "status"])),
        GuestOperation::Observe {
            op: "container:status".to_owned(),
            target: Target::Host,
        }
    );
    assert_eq!(
        classify(&parsed(&["container", "down"])),
        GuestOperation::Invoke {
            selector: Selector::Operation("container:down".to_owned()),
            target: Target::Host,
        }
    );
    assert_eq!(
        classify(&parsed(&["runs"])),
        GuestOperation::Observe {
            op: "runs:list".to_owned(),
            target: Target::Host,
        }
    );
    assert_eq!(
        classify(&parsed(&["build"])),
        GuestOperation::Invoke {
            selector: Selector::Task("build".to_owned()),
            target: Target::Host,
        }
    );
}

#[test]
fn the_checkout_relative_directory_never_escapes_the_mount() {
    let paths = workspace_context(Some(session_run()), ready_channel()).paths;
    assert_eq!(
        checkout_relative(&paths, Path::new("/workspace/packages/app")).expect("relative"),
        "packages/app"
    );
    assert_eq!(
        checkout_relative(&paths, Path::new("/workspace")).expect("relative"),
        "."
    );
    assert!(checkout_relative(&paths, Path::new("/elsewhere")).is_err());
    assert!(checkout_relative(&paths, Path::new("/workspace/../etc")).is_err());
}

#[test]
fn a_guest_without_a_container_observation_refuses_instead_of_trusting_the_context() {
    let error = UnobservedContainer
        .container()
        .expect_err("no guest-local observation is merged");
    assert!(error.to_string().contains("observation"), "{error}");
}

#[test]
fn an_unqualified_transport_refuses_without_reaching_guest_state() {
    let context = workspace_context(
        Some(session_run()),
        context::WorkspaceChannelLocator::default(),
    );
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let before = guest.tree_digest();
    let mut checkout = FakeCheckout::default();
    let mut identity = ObservedContainer(container());
    let mut opener = PinnedChannelTransport {
        backend: "docker".to_owned(),
        budget: CHANNEL_OPEN_BUDGET,
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut identity,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("an unqualified channel refuses");

    assert!(
        error
            .to_string()
            .contains("no host authority channel is qualified"),
        "{error}"
    );
    assert_eq!(guest.tree_digest(), before, "a refusal writes nothing");
    assert_eq!(checkout.reads, 0, "a refusal never falls back to a read");
}

#[test]
fn a_host_refusal_returns_an_error_and_never_falls_back() {
    let context = workspace_context(Some(session_run()), ready_channel());
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let before = guest.tree_digest();
    let mut checkout = FakeCheckout::with_file("effigy.toml", b"[tasks]\nbuild = \"\"\n");
    let mut identity = ObservedContainer(container());
    let mut opener = RefusingChannel {
        code: "unauthenticated",
        message: "this capability was not issued by this session".to_owned(),
        opened: 0,
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut identity,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("the host refused");

    assert!(error.to_string().contains("unauthenticated"), "{error}");
    assert_eq!(opener.opened, 1);
    assert_eq!(guest.tree_digest(), before);
    assert_eq!(checkout.reads, 0, "no guest fallback executed");
}

#[test]
fn a_version_mismatch_refuses_before_the_connection() {
    let mut context = workspace_context(Some(session_run()), ready_channel());
    context.binary.version = "0.0.0-other".to_owned();
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let mut checkout = FakeCheckout::default();
    let mut identity = ObservedContainer(container());
    let mut opener = RefusingChannel {
        code: "unavailable",
        message: "the opener must never be reached".to_owned(),
        opened: 0,
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut identity,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("the binary is not the pinned one");

    assert!(error.to_string().contains("version_mismatch"), "{error}");
    assert_eq!(opener.opened, 0, "no credential crossed the connection");
}

#[test]
fn an_identity_mismatch_refuses_before_the_connection() {
    let context = workspace_context(Some(session_run()), ready_channel());
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let mut checkout = FakeCheckout::default();
    let mut identity = ObservedContainer(WorkspaceContainerIdentity {
        container_id: "another-container".to_owned(),
        ..container()
    });
    let mut opener = RefusingChannel {
        code: "unavailable",
        message: "the opener must never be reached".to_owned(),
        opened: 0,
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut identity,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("another container");

    assert!(error.to_string().contains("identity_mismatch"), "{error}");
    assert_eq!(opener.opened, 0);
}

#[test]
fn a_direct_terminal_without_a_session_generation_refuses_locally() {
    let context = workspace_context(Some(session_run()), ready_channel());
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, None);
    guest.put_file(
        ENROL_CREDENTIAL_PATH,
        format!("environment_generation={ENVIRONMENT_GENERATION}\ncontainer_id={CONTAINER_ID}\ncredential={}\n", secret_text(&Secret::new([0x11; 32]))),
        0o600,
    );

    let error = load_auth(&mut guest, &context).expect_err("no session generation is published");

    assert!(error.to_string().contains("session generation"), "{error}");
}

// ---------------------------------------------------------------------------
// The client against lane A's real server.
// ---------------------------------------------------------------------------

use effigy_kernel::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, EffectContext, Limits, ObserveContext,
    OperationContext,
};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{
    CheckoutId, CheckoutKind, ExecutionTarget, InvocationSpec, ResolvedInvocation, ResourceId,
    Route, Surface, TaskId,
};
use effigy_kernel::observe::{AbsenceProof, Observed};
use effigy_kernel::outcome::Invocation;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{BeginOpts, Effect, Reconciled, Run, SettlementAdapter};
use effigy_kernel::workspace_channel as host;

type Served = Arc<Mutex<Vec<Result<host::Served, host::Refusal>>>>;

#[derive(Debug, Default)]
struct Calls {
    resolved: usize,
    selector: Option<String>,
    cwd: Option<String>,
    admitted: Vec<RunId>,
    observed: Vec<String>,
}

type SharedCalls = Arc<Mutex<Calls>>;

fn limits() -> Limits {
    Limits {
        capture_tail: 1024,
        log_rotate: 4096,
        log_keep: 2,
        line_max: 512,
        queue_bytes: 2048,
        probe_concurrency: 2,
    }
}

fn effects() -> EffectContext {
    OperationContext::new(
        Deadline::At(std::time::Instant::now() + Duration::from_secs(60)),
        CancelToken::default(),
        limits(),
        EffectClass::Mutate,
        effigy_kernel::ctx::Placement::Host,
    )
    .effects()
    .expect("an effect context")
}

fn observation() -> ObserveContext {
    OperationContext::new(
        Deadline::At(std::time::Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        limits(),
        EffectClass::Observe,
        effigy_kernel::ctx::Placement::Host,
    )
    .observe()
    .expect("an observation context")
}

fn cleanup(cx: &EffectContext) -> CleanupContext {
    cx.operation()
        .cleanup(Duration::from_secs(10))
        .expect("cleanup")
}

fn session_task(name: &str) -> TaskId {
    TaskId::new(
        CheckoutId::new(CheckoutKind::EphemeralClone, [7; 16]),
        "",
        Surface::Published,
        name,
        None,
    )
    .expect("task id")
}

fn resolved(owner: ResourceId) -> ResolvedInvocation {
    ResolvedInvocation::new(InvocationSpec {
        task: session_task("workspace-client"),
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner,
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: effigy_kernel::ctx::Placement::Host,
        provenance: BTreeMap::new(),
    })
    .expect("resolved invocation")
}

fn next_argv() -> Invocation {
    Invocation::validated(vec!["run".into()], |args| args == ["run"]).expect("invocation")
}

#[derive(Debug)]
struct NoEffects;

impl SettlementAdapter for NoEffects {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<effigy_kernel::run::Fence> {
        Observed::Absent(AbsenceProof::NoGateInode)
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Absent(AbsenceProof::NoGateInode)
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        Observed::Absent(AbsenceProof::NoGateInode)
    }
}

fn live_run() -> LiveGuestRun {
    let context = workspace_context(Some(session_run()), ready_channel());
    let mut identity = ObservedContainer(container());
    live_guest_run(&context, GENERATION, &mut identity).expect("live run claims")
}

fn verified() -> VerifiedWorkspaceContext {
    workspace_context(Some(session_run()), ready_channel())
        .verify(&live_run())
        .expect("a verified context")
}

fn server_identity() -> Digest {
    Digest::of(b"pinned workspace host")
}

#[derive(Debug)]
struct CountingResolver {
    calls: SharedCalls,
    owner: ResourceId,
}

impl host::Resolver for CountingResolver {
    fn resolve(
        &mut self,
        _: &EffectContext,
        body: &InvocationBody,
    ) -> Result<ResolvedInvocation, host::Refusal> {
        let mut calls = self.calls.lock().expect("calls");
        calls.resolved += 1;
        calls.selector = Some(body.selector.text().to_owned());
        calls.cwd = Some(body.cwd.clone());
        Ok(resolved(self.owner.clone()))
    }
}

/// The admitted runs stay open for the proof: an admission whose owner gate
/// and process identity can no longer be established is `Unknown` by design,
/// and this fixture is the host that keeps them.
struct CountingAdmitter<'a> {
    calls: SharedCalls,
    root: &'a StateRoot,
    runs: Vec<Run<'a>>,
}

impl host::Admission for CountingAdmitter<'_> {
    fn admit(
        &mut self,
        cx: &EffectContext,
        ticket: &effigy_kernel::run::AdmissionTicket,
        resolved: &ResolvedInvocation,
    ) -> Result<(), host::Refusal> {
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
        .map_err(|error| host::Refusal::forbidden(error.to_string()))?;
        self.runs.push(run);
        self.calls
            .lock()
            .expect("calls")
            .admitted
            .push(ticket.run());
        Ok(())
    }
}

#[derive(Debug)]
struct CountingObserver {
    calls: SharedCalls,
}

impl host::Observer for CountingObserver {
    fn observe(
        &mut self,
        _: &ObserveContext,
        body: &ObserveBody,
    ) -> Result<serde_json::Value, host::Refusal> {
        self.calls
            .lock()
            .expect("calls")
            .observed
            .push(body.op.clone());
        Ok(serde_json::json!({ "op": body.op }))
    }
}

#[derive(Debug)]
struct RunIds(VecDeque<RunId>);

impl host::RunIds for RunIds {
    fn next(&mut self) -> Result<RunId, host::Refusal> {
        self.0
            .pop_front()
            .ok_or_else(|| host::Refusal::unavailable("the proof ran out of run ids"))
    }
}

fn negotiation() -> host::Negotiation {
    let binary = running_binary();
    host::Negotiation {
        context_generation: Generation::new(GENERATION),
        session_run_id: session_run(),
        artifact: host::HostArtifact {
            effigy_version: binary.version,
            binary_digest: parse_digest(&binary.digest).expect("the running digest"),
        },
    }
}

fn fence() -> host::SessionFence {
    host::SessionFence {
        context_generation: Generation::new(GENERATION),
        environment_generation: Generation::new(
            ENVIRONMENT_GENERATION
                .parse()
                .expect("environment generation"),
        ),
        session: effigy_kernel::run::Session {
            run: session_run(),
            generation: Generation::new(1),
        },
        container_id: CONTAINER_ID.to_owned(),
        vm_boot_id: VM_BOOT.to_owned(),
    }
}

fn grant_binding() -> host::GrantBinding {
    host::GrantBinding {
        host_user: "operator".to_owned(),
        checkout: "checkout".to_owned(),
        session: effigy_kernel::run::Session {
            run: session_run(),
            generation: Generation::new(1),
        },
        environment_generation: Generation::new(
            ENVIRONMENT_GENERATION
                .parse()
                .expect("environment generation"),
        ),
        container_id: CONTAINER_ID.to_owned(),
        vm_boot_id: VM_BOOT.to_owned(),
    }
}

/// One host, serving the scripted connections in order. Every engine seam is a
/// counting fake, and the session run is a real journal row so the kernel's own
/// reservation and admission records are exercised.
fn serve_host(
    connections: Vec<UnixStream>,
    calls: SharedCalls,
    admitted: &[u8],
) -> (Served, std::thread::JoinHandle<()>) {
    let served: Served = Arc::new(Mutex::new(Vec::new()));
    let served_in_thread = served.clone();
    let run_ids = admitted
        .iter()
        .map(|marker| RunId::new(1, [*marker; 10]).expect("run id"))
        .collect::<VecDeque<_>>();
    let handle = std::thread::spawn(move || {
        let fixture = effigy_test_support::Fixture::new("workspace-client").expect("fixture");
        let cx = effects();
        let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
        let owner = session_task("session").resource();
        let run = Run::begin(
            &cx,
            &root,
            &resolved(owner),
            BeginOpts {
                id: session_run(),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next_argv(),
                custody: None,
            },
            &cleanup(&cx),
            &mut NoEffects,
        )
        .expect("session run");
        drop(run);
        let observe = observation();
        let mut grants = host::GrantBook::new();
        grants.issue(
            capability(),
            grant_binding(),
            host::session_operations(),
            [Target::Host].into_iter().collect(),
        );
        let mut server = host::Server::new(
            negotiation(),
            fence(),
            server_identity(),
            grants,
            host::Dispatcher::new(
                CountingAdmitter {
                    calls: calls.clone(),
                    root: &root,
                    runs: Vec::new(),
                },
                CountingResolver {
                    calls: calls.clone(),
                    owner: session_task("guest-request").resource(),
                },
                CountingObserver {
                    calls: calls.clone(),
                },
                RunIds(run_ids),
            ),
        );
        for connection in connections {
            let mut transport = PeerTransport::new(connection);
            let result = server.serve(&mut transport, &cx, &observe, &root, &mut host::NoStreams);
            served_in_thread.lock().expect("served").push(result);
        }
    });
    (served, handle)
}

/// One in-memory connection. The identity TLS would have verified is the
/// pinned one, so the host's pin check is exercised on every connection.
struct PeerTransport {
    stream: UnixStream,
    pinned: Digest,
}

impl PeerTransport {
    fn new(stream: UnixStream) -> Self {
        // A peer that already closed cannot take a timeout; the connection then
        // reads end of stream rather than hanging, which is what the proof
        // wants anyway.
        let _ = stream.set_read_timeout(Some(Duration::from_secs(20)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(20)));
        Self {
            stream,
            pinned: server_identity(),
        }
    }
}

impl Transport for PeerTransport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        use std::io::Read as _;
        (&self.stream).read(buf)
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write as _;
        (&self.stream).write_all(bytes)
    }

    fn verified_peer(&self) -> Option<host::ServerIdentity> {
        Some(host::ServerIdentity {
            digest: self.pinned.clone(),
        })
    }
}

fn connection_pair() -> (UnixStream, UnixStream) {
    UnixStream::pair().expect("a socket pair")
}

/// A transport that answers the negotiation and then goes silent: the host
/// never sees this connection, so whether it acted is `Unknown`.
struct MuteTransport {
    input: Vec<u8>,
    cursor: usize,
    written: Arc<Mutex<Vec<u8>>>,
    peer: Option<Digest>,
}

impl MuteTransport {
    fn answering_negotiation() -> Self {
        Self::answering_negotiation_with_peer(server_identity())
    }

    /// A peer that verifies as `peer`, so a mispinned connection can be proved
    /// to send nothing at all.
    fn answering_negotiation_with_peer(peer: Digest) -> Self {
        let hello_ok = host::encode_frame(&host::Frame::HelloOk(host::HelloOk {
            protocol: host::PROTOCOL_V1,
            effigy_version: running_binary().version,
        }))
        .expect("encoded hello_ok");
        let mut input = Vec::new();
        input.extend_from_slice(&(hello_ok.len() as u32).to_be_bytes());
        input.extend_from_slice(&hello_ok);
        Self {
            input,
            cursor: 0,
            written: Arc::new(Mutex::new(Vec::new())),
            peer: Some(peer),
        }
    }
}

impl Transport for MuteTransport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.input.len() - self.cursor;
        if left == 0 {
            return Ok(0);
        }
        let take = left.min(buf.len());
        buf[..take].copy_from_slice(&self.input[self.cursor..self.cursor + take]);
        self.cursor += take;
        Ok(take)
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.written
            .lock()
            .expect("written")
            .extend_from_slice(bytes);
        Ok(())
    }

    fn verified_peer(&self) -> Option<host::ServerIdentity> {
        self.peer
            .clone()
            .map(|digest| host::ServerIdentity { digest })
    }
}

#[test]
fn a_guest_authority_request_reaches_host_resolution_and_admission() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let (client_side, host_side) = connection_pair();
    let (served, handle) = serve_host(vec![host_side], calls.clone(), &[9]);
    let context = workspace_context(Some(session_run()), ready_channel());
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let mut checkout = FakeCheckout::default();
    let mut identity = ObservedContainer(container());
    let mut opener = PeerChannel {
        connections: vec![client_side],
    };

    let rendered = dispatch_in(
        &parsed(&["build"]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut identity,
        &mut opener,
        Path::new("/workspace/packages/app"),
    )
    .expect("the host admitted the request");

    let expected = RunId::new(1, [9; 10]).expect("run id");
    assert_eq!(
        rendered,
        format!("admitted run {expected} on the workspace host")
    );
    {
        let calls = calls.lock().expect("calls");
        assert_eq!(calls.resolved, 1, "the host resolved the guest's request");
        assert_eq!(calls.selector.as_deref(), Some("build"));
        assert_eq!(calls.cwd.as_deref(), Some("packages/app"));
        assert_eq!(calls.admitted.len(), 1, "the host recorded one admission");
    }
    drop(opener);
    handle.join().expect("host thread");
    let served = served.lock().expect("served");
    let outcome = served[0].as_ref().expect("a served connection");
    assert_eq!(outcome.responses, 1);
}

/// The production channel opener's shape: one connection per open, handing the
/// client the transport the host side of the pair speaks to.
struct PeerChannel {
    connections: Vec<UnixStream>,
}

impl ChannelOpener for PeerChannel {
    fn open(
        &mut self,
        verified: &VerifiedWorkspaceContext,
        live: &LiveGuestRun,
        auth: GuestAuth,
    ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
        if self.connections.is_empty() {
            return Err(ChannelError::local("the proof ran out of connections"));
        }
        let connection = self.connections.remove(0);
        let client = WorkspaceClient::connect(
            PeerTransport::new(connection),
            verified.clone(),
            live.clone(),
            auth,
        )?;
        Ok(Box::new(client))
    }
}

#[test]
fn a_wrong_capability_refuses_before_any_effect() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let (client_side, host_side) = connection_pair();
    let (served, handle) = serve_host(vec![host_side], calls.clone(), &[9]);
    let context = workspace_context(Some(session_run()), ready_channel());
    let mut guest = FakeGuest::new();
    // A capability this host never issued.
    publish(&mut guest, &context, Some(&Secret::new([0xcd; 32])));
    let mut checkout = FakeCheckout::default();
    let mut identity = ObservedContainer(container());
    let mut opener = PeerChannel {
        connections: vec![client_side],
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut identity,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("an unissued capability refuses");

    assert!(error.to_string().contains("unauthenticated"), "{error}");
    let calls = calls.lock().expect("calls");
    assert_eq!(calls.resolved, 0, "no resolution");
    assert!(calls.admitted.is_empty(), "no admission");
    drop(calls);
    drop(opener);
    handle.join().expect("host thread");
    let served = served.lock().expect("served");
    // The host refused the connection during authentication, so it never
    // entered the request half: no response, no resolution, no admission.
    match &served[0] {
        Err(refusal) => assert_eq!(refusal.code, host::RefusalCode::Unauthenticated),
        Ok(served) => assert_eq!(served.responses, 0),
    }
}

#[test]
fn a_repeated_intent_is_answered_from_the_hosts_record() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let (first_client, first_host) = connection_pair();
    let (second_client, second_host) = connection_pair();
    let (served, handle) = serve_host(vec![first_host, second_host], calls.clone(), &[9]);
    let intent = Intent::new(RequestBody::Invoke(InvocationBody {
        selector: Selector::Task("build".to_owned()),
        cwd: "packages/app".to_owned(),
        target: Target::Host,
        args: vec![],
        output: ChannelOutputMode::Text,
    }));

    let mut first = WorkspaceClient::connect(
        PeerTransport::new(first_client),
        verified(),
        live_run(),
        GuestAuth::Capability(capability()),
    )
    .expect("the first connection");
    let admitted = first.send(&intent).expect("the host admitted");
    drop(first);
    let ChannelOutcome::Accepted { run, .. } = admitted else {
        panic!("a fresh invocation is admitted");
    };

    // A reconnect replays the same intent under the same id: the host answers
    // from the record it wrote, and never admits a second run.
    let mut second = WorkspaceClient::connect(
        PeerTransport::new(second_client),
        verified(),
        live_run(),
        GuestAuth::Capability(capability()),
    )
    .expect("the second connection");
    let replayed = second.send(&intent).expect("the host replayed");
    assert_eq!(replayed, admitted);
    assert_eq!(
        replayed,
        ChannelOutcome::Accepted {
            run,
            placement: RunPlacement::Host,
        }
    );
    drop(second);
    handle.join().expect("host thread");
    let served = served.lock().expect("served");
    assert_eq!(served.len(), 2);
    assert_eq!(
        calls.lock().expect("calls").admitted,
        vec![run],
        "one record, one run"
    );
}

#[test]
fn an_unanswered_attempt_is_unknown_and_replayable() {
    let mut client = WorkspaceClient::connect(
        MuteTransport::answering_negotiation(),
        verified(),
        live_run(),
        GuestAuth::Capability(capability()),
    )
    .expect("the negotiation answered");
    let intent = Intent::new(RequestBody::Observe(ObserveBody {
        op: "doctor".to_owned(),
        target: Some(Target::Host),
    }));

    let error = client.send(&intent).expect_err("the host went silent");

    match error {
        ChannelError::Unknown { replayable, .. } => {
            assert!(replayable, "the same id answers from the record");
        }
        other => panic!("an unanswered request is Unknown, not {other:?}"),
    }
}

#[test]
fn a_secondary_terminal_cannot_detach_the_owners_run() {
    let transport = MuteTransport::answering_negotiation();
    let written = transport.written.clone();
    let mut client = WorkspaceClient::connect(
        transport,
        verified(),
        live_run(),
        GuestAuth::Capability(capability()),
    )
    .expect("the negotiation answered");
    let before = written.lock().expect("written").len();

    let error = client
        .detach(session_run())
        .expect_err("an unadmitted run cannot be detached");

    match error {
        ChannelError::Refused { code, .. } => assert_eq!(code, "forbidden"),
        other => panic!("a secondary release is refused locally, not {other:?}"),
    }
    assert_eq!(
        written.lock().expect("written").len(),
        before,
        "a secondary terminal writes no control frame"
    );
}

#[test]
fn an_observation_reaches_the_hosts_observe_engine() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let (client_side, host_side) = connection_pair();
    let (served, handle) = serve_host(vec![host_side], calls.clone(), &[]);
    let context = workspace_context(Some(session_run()), ready_channel());
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let mut checkout = FakeCheckout::default();
    let mut identity = ObservedContainer(container());
    let mut opener = PeerChannel {
        connections: vec![client_side],
    };

    let rendered = dispatch_in(
        &parsed(&["doctor"]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut identity,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect("the host observed");

    assert!(rendered.contains("doctor"), "{rendered}");
    assert_eq!(
        calls.lock().expect("calls").observed,
        vec!["doctor".to_owned()],
        "the host ran it under its own observe engine"
    );
    drop(opener);
    handle.join().expect("host thread");
    let served = served.lock().expect("served");
    assert_eq!(
        served[0].as_ref().expect("a served connection").responses,
        1
    );
}

// ---------------------------------------------------------------------------
// The pinned host identity, checked before anything is written (§8.11).
// ---------------------------------------------------------------------------

fn mispinned() -> Digest {
    Digest::of(b"a host this context did not pin")
}

#[test]
fn an_unusable_pinned_locator_refuses_as_unavailable() {
    let mut context = workspace_context(Some(session_run()), ready_channel());
    // A ready locator whose pin is not a digest: the production transport
    // refuses it before dialling, and the guest reports unavailable.
    context.channel.server_identity = Some("sha256:not-a-digest".to_owned());
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let before = guest.tree_digest();
    let mut checkout = FakeCheckout::default();
    let mut identity = ObservedContainer(container());
    let mut opener = PinnedChannelTransport {
        backend: "docker".to_owned(),
        budget: CHANNEL_OPEN_BUDGET,
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut identity,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("an unusable locator refuses");

    assert!(error.to_string().contains("unavailable"), "{error}");
    assert_eq!(guest.tree_digest(), before);
}

#[test]
fn a_mispinned_peer_receives_no_hello_and_no_credential() {
    let mut transport = MuteTransport::answering_negotiation_with_peer(mispinned());
    let written = transport.written.clone();
    let error = WorkspaceClient::connect(
        transport,
        verified(),
        live_run(),
        // A real capability is present: it must never reach the wire.
        GuestAuth::Capability(capability()),
    )
    .expect_err("an unpinned peer has no session");

    match error {
        ChannelError::Refused { code, .. } => assert_eq!(code, "identity_mismatch"),
        other => panic!("a mispinned peer is an identity refusal, not {other:?}"),
    }
    assert!(
        written.lock().expect("written").is_empty(),
        "a mispinned peer receives no hello and no credential bytes"
    );
}

#[test]
fn a_mispinned_peer_refuses_the_guest_dispatch_before_any_effect() {
    let context = workspace_context(Some(session_run()), ready_channel());
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let before = guest.tree_digest();
    let mut checkout = FakeCheckout::default();
    let mut identity = ObservedContainer(container());
    let written = Arc::new(Mutex::new(Vec::new()));
    let mut opener = ScriptedPeerChannel {
        peer: mispinned(),
        written: written.clone(),
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut identity,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("the peer is not the pinned host");

    assert!(error.to_string().contains("identity_mismatch"), "{error}");
    assert!(written.lock().expect("written").is_empty());
    assert_eq!(guest.tree_digest(), before, "a pin refusal writes nothing");
}

/// Opens a connection to a peer that verifies as `peer`, for the pin proof.
struct ScriptedPeerChannel {
    peer: Digest,
    written: Arc<Mutex<Vec<u8>>>,
}

impl ChannelOpener for ScriptedPeerChannel {
    fn open(
        &mut self,
        verified: &VerifiedWorkspaceContext,
        live: &LiveGuestRun,
        auth: GuestAuth,
    ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
        let mut transport = MuteTransport::answering_negotiation_with_peer(self.peer.clone());
        transport.written = self.written.clone();
        let client = WorkspaceClient::connect(transport, verified.clone(), live.clone(), auth)?;
        Ok(Box::new(client))
    }
}

// ---------------------------------------------------------------------------
// Attach, cursor and replay against the real host dispatcher.
// ---------------------------------------------------------------------------

fn attach_view(outcome: &ChannelOutcome) -> host::AttachView {
    match outcome {
        ChannelOutcome::Attached(view) => view.clone(),
        other => panic!("an attach answers with the host's typed view, not {other:?}"),
    }
}

#[test]
fn an_attach_intent_replays_to_the_same_attachment() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let (client_side, host_side) = connection_pair();
    let (_served, handle) = serve_host(vec![host_side], calls.clone(), &[9]);

    // The owning connection admits the run and then opens its own attachment.
    let mut owner = WorkspaceClient::connect(
        PeerTransport::new(client_side),
        verified(),
        live_run(),
        GuestAuth::Capability(capability()),
    )
    .expect("the owning connection");
    let invoke = Intent::new(RequestBody::Invoke(InvocationBody {
        selector: Selector::Task("build".to_owned()),
        cwd: "packages/app".to_owned(),
        target: Target::Host,
        args: vec![],
        output: ChannelOutputMode::Text,
    }));
    let ChannelOutcome::Accepted { run, .. } = owner.send(&invoke).expect("admitted") else {
        panic!("a fresh invocation is admitted");
    };

    let attach = Intent::attach(run, vec![StreamKind::Stdout], Default::default());
    let first = attach_view(&owner.send(&attach).expect("attached"));
    assert_eq!(first.run, run);
    assert_eq!(
        first.cursor, 0,
        "the attachment starts at the host's beginning"
    );
    // The same intent is the same attachment, never a second one: the request
    // id is the attachment's identity, so a lost response is resumed rather
    // than allocated twice (§8.11).
    let second = attach_view(&owner.send(&attach).expect("replayed"));
    assert_eq!(second.attachment, first.attachment);
    drop(owner);
    handle.join().expect("host thread");
    assert_eq!(
        calls.lock().expect("calls").admitted,
        vec![run],
        "an attach and its replay admit nothing"
    );
}

#[test]
fn an_unresumable_cursor_is_a_typed_refusal_with_no_fallback() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let (owner_client, owner_host) = connection_pair();
    let (cursor_client, cursor_host) = connection_pair();
    let (_served, handle) = serve_host(vec![owner_host, cursor_host], calls.clone(), &[9]);

    let mut owner = WorkspaceClient::connect(
        PeerTransport::new(owner_client),
        verified(),
        live_run(),
        GuestAuth::Capability(capability()),
    )
    .expect("the owning connection");
    let invoke = Intent::new(RequestBody::Invoke(InvocationBody {
        selector: Selector::Task("build".to_owned()),
        cwd: "packages/app".to_owned(),
        target: Target::Host,
        args: vec![],
        output: ChannelOutputMode::Text,
    }));
    let ChannelOutcome::Accepted { run, .. } = owner.send(&invoke).expect("admitted") else {
        panic!("a fresh invocation is admitted");
    };
    drop(owner);

    let mut cursors = std::collections::BTreeMap::new();
    cursors.insert(1u32, 0u64);
    let mut terminal = WorkspaceClient::connect(
        PeerTransport::new(cursor_client),
        verified(),
        live_run(),
        GuestAuth::Capability(capability()),
    )
    .expect("the attaching terminal");
    let error = terminal
        .send(&Intent::attach(run, vec![StreamKind::Stdout], cursors))
        .expect_err("the host cannot resume a stream that wrote nothing");

    match error {
        ChannelError::Refused { code, .. } => assert_eq!(code, "invalid_request"),
        other => panic!("an unresumable cursor is a typed refusal, not {other:?}"),
    }
    drop(terminal);
    handle.join().expect("host thread");
    // The refusal recorded no second run and stopped nothing.
    assert_eq!(calls.lock().expect("calls").admitted, vec![run]);
}

// ---------------------------------------------------------------------------
// The frozen activation handoff: adopt the owned channel, retain it, settle it.
// ---------------------------------------------------------------------------

use super::serving::{AdoptError, OwnedWorkspaceChannel, ServingOwner};
use crate::runner::workspace_transport::qualification::{
    Backend, ChannelAliasPlan, ChannelPreparation, GuestChannelProbe, GuestDialEvidence,
    PreparedHostChannel, ResolvedAlias,
};
use effigy_kernel::workspace_session::SessionRef;
use std::net::{SocketAddr, TcpStream};

/// The guest half of `prepare_channel`'s qualification. The fixture models the
/// private fixture network: the TLS dial is real (pin checked in the
/// handshake) and sends no application bytes.
struct PrivateGuest {
    target: WorkspaceContainerIdentity,
    peer: SocketAddr,
}

impl GuestChannelProbe for PrivateGuest {
    fn resolve(
        &mut self,
        _: &WorkspaceContainerIdentity,
        alias: &str,
        _: Duration,
    ) -> Observed<ResolvedAlias> {
        assert_eq!(alias, "effigy-host");
        Observed::Verified(
            ResolvedAlias {
                target: self.target.clone(),
                address: self.peer.ip(),
            },
            serving_evidence(),
        )
    }

    fn dial(
        &mut self,
        _: &WorkspaceContainerIdentity,
        endpoint: &str,
        pin: &Digest,
        budget: Duration,
    ) -> Observed<GuestDialEvidence> {
        let port = endpoint
            .strip_prefix("effigy-host:")
            .and_then(|port| port.parse::<u16>().ok())
            .expect("the guest dials the published alias");
        self.peer.set_port(port);
        match crate::runner::workspace_transport::TlsChannelTransport::connect(
            self.peer,
            pin.clone(),
            budget,
        ) {
            Ok(transport) => {
                let verified = transport.verified_peer().expect("a pinned peer").digest;
                drop(transport);
                Observed::Verified(
                    GuestDialEvidence {
                        target: self.target.clone(),
                        peer: self.peer,
                        pin: verified,
                    },
                    serving_evidence(),
                )
            }
            Err(_) => Observed::Unknown(effigy_kernel::observe::UnknownReason::Unparsable(
                "guest TLS dial refused".into(),
            )),
        }
    }
}

/// Prepare the exact channel the environment publishes, on this run.
fn prepare_owned_channel(
    supervisor: &mut SessionSupervisor<'_>,
    session: &SessionRef,
) -> PreparedHostChannel {
    let target = container();
    let plan = ChannelAliasPlan::observed(Backend::LinuxDocker, "default", None, None)
        .expect("an observed alias plan");
    let mut guest = PrivateGuest {
        target: target.clone(),
        peer: "127.0.0.1:0".parse().expect("peer"),
    };
    crate::runner::workspace_transport::qualification::prepare_channel(
        ChannelPreparation {
            container_mode: true,
            plan: &plan,
            target: &target,
            session,
            supervisor: "452-supervisor",
        },
        supervisor,
        &mut guest,
        Duration::from_secs(2),
    )
    .expect("channel preparation")
    .expect("container mode prepares a channel")
}

fn serving_server_for(
    calls: SharedCalls,
    root: &StateRoot,
) -> host::Server<CountingAdmitter<'_>, CountingResolver, CountingObserver, RunIds> {
    host::Server::new(
        negotiation(),
        fence(),
        server_identity(),
        grants_for(capability()),
        host::Dispatcher::new(
            CountingAdmitter {
                calls: calls.clone(),
                root,
                runs: Vec::new(),
            },
            CountingResolver {
                calls: calls.clone(),
                owner: session_task("guest-request").resource(),
            },
            CountingObserver {
                calls: calls.clone(),
            },
            RunIds(VecDeque::new()),
        ),
    )
}

#[test]
fn the_owned_channel_is_adopted_retained_and_settled() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let endpoint: SocketAddr = prepared
            .locator
            .channel()
            .endpoint
            .as_deref()
            .and_then(|endpoint| endpoint.strip_prefix("effigy-host:"))
            .and_then(|port| port.parse::<u16>().ok())
            .and_then(|port| Some(SocketAddr::from(([127, 0, 0, 1], port))))
            .expect("a published alias endpoint");

        let owner = ServingOwner::adopt_owned(
            OwnedWorkspaceChannel {
                supervisor,
                session,
                channel: prepared,
                effect: Arc::new(cx.clone()),
                observe: Arc::new(observe.clone()),
                root,
            },
            serving_server_for(calls.clone(), root),
        )
        .expect("the frozen channel is adopted");

        // The owner retains the launch and the advertised locator; nothing
        // served until it takes a turn.
        assert!(owner.launch().is_some(), "the listener is retained");
        assert!(
            owner.locator().is_some(),
            "the published locator is retained"
        );
        assert!(owner.key_held(), "the signing key is retained");
        assert!(
            !owner.serving(),
            "no connection is dispatched before a turn"
        );

        let mut owner = owner;
        let served = owner
            .serve_pending(&mut host::NoStreams)
            .expect("a turn with nothing waiting");
        assert!(served.is_none(), "nothing was waiting");
        assert!(owner.serving(), "the adopted run serves the channel");

        // Settlement stops the listener, wipes the key and settles the run.
        let cleanup = cleanup(cx);
        let outcome = owner.settle(&cleanup, &mut NoEffects);
        assert!(outcome.is_ok(), "the session settles: {outcome:?}");
        assert!(
            TcpStream::connect(endpoint).is_err(),
            "no listener outlives the settled owner"
        );
    });
}

#[test]
fn a_channel_published_for_another_session_is_never_adopted() {
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let endpoint: SocketAddr = prepared
            .locator
            .channel()
            .endpoint
            .as_deref()
            .and_then(|endpoint| endpoint.strip_prefix("effigy-host:"))
            .and_then(|port| port.parse::<u16>().ok())
            .map(|port| SocketAddr::from(([127, 0, 0, 1], port)))
            .expect("a published alias endpoint");
        // Another session's identity: the same environment, a different run.
        let other = SessionRef::new(
            RunId::new(1, [201; 10]).expect("run id"),
            session.fence().clone(),
            session.environment().clone(),
            session.environment_generation(),
        );

        let error = ServingOwner::adopt_owned(
            OwnedWorkspaceChannel {
                supervisor,
                session: other,
                channel: prepared,
                effect: Arc::new(cx.clone()),
                observe: Arc::new(observe.clone()),
                root,
            },
            serving_server_for(Arc::new(Mutex::new(Calls::default())), root),
        )
        .expect_err("a channel published for another session is refused");

        assert!(matches!(error, AdoptError::WrongSession), "{error:?}");
        assert!(
            TcpStream::connect(endpoint).is_err(),
            "a refused adoption does not keep the other session's listener"
        );
    });
}

// ---------------------------------------------------------------------------
// The serving owner: adopt, serve, accept, dispatch, release.
// ---------------------------------------------------------------------------

use super::serving::ChannelAccept;
use crate::runner::workspace_transport::listener::SessionTlsListener;
use effigy_kernel::observe::Evidence;
use effigy_kernel::workspace_session::{ServingProbe, SessionLaunch, SessionSupervisor};

fn serving_evidence() -> Evidence {
    Evidence::new("452 serving owner proof".to_owned(), None).expect("evidence")
}

/// A probe that answers the qualification handshake. The real pinned handshake
/// is the transport's own proof (#504); this fixture only needs a launch that is
/// qualified enough to publish.
struct VerifiedProbe;

impl ServingProbe for VerifiedProbe {
    fn pinned_handshake(&mut self, _: SocketAddr, _: &Digest, _: Duration) -> Observed<()> {
        Observed::Verified((), serving_evidence())
    }
}

/// A session run with its environment active and the session's own custody
/// record, as the host supervisor holds it after the E10 handoff.
fn with_admitted_session<T>(
    test: impl for<'a> FnOnce(
        Run<'a>,
        SessionRef,
        &'a EffectContext,
        &'a StateRoot,
        &'a ObserveContext,
    ) -> T,
) -> T {
    let fixture = effigy_test_support::Fixture::new("workspace-serving").expect("fixture");
    let cx = effects();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    let checkout = CheckoutId::new(CheckoutKind::EphemeralClone, [37; 16]);
    let environment = ResourceId::environment(checkout, "stack").expect("environment");
    let task = TaskId::new(checkout, "", Surface::Published, "workspace", None).expect("task");
    let invocation = ResolvedInvocation::new(InvocationSpec {
        owner: task.resource(),
        task: task.clone(),
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: effigy_kernel::ctx::Placement::Host,
        provenance: BTreeMap::new(),
    })
    .expect("invocation");
    let cleanup_context = cleanup(&cx);
    // The environment row must be active before a session may borrow it.
    let mut environment_spec = invocation.spec().clone();
    environment_spec.owner = environment.clone();
    let environment_invocation = ResolvedInvocation::new(environment_spec).expect("invocation");
    Run::begin(
        &cx,
        &root,
        &environment_invocation,
        BeginOpts {
            id: RunId::new(1, [36; 10]).expect("run id"),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next_argv(),
            custody: None,
        },
        &cleanup_context,
        &mut NoEffects,
    )
    .expect("environment run")
    .settle(&cleanup_context, &mut NoEffects)
    .expect("settled environment");
    let connection =
        rusqlite::Connection::open(root.path().join(effigy_kernel::store::DB_NAME)).expect("store");
    connection
        .execute(
            "UPDATE resources SET state='active',last_applied='1' WHERE name=?1",
            [environment.name().to_string()],
        )
        .expect("active environment");
    drop(connection);
    let (run, session) = SessionRef::admit(
        &cx,
        &root,
        &invocation,
        BeginOpts {
            id: session_run(),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next_argv(),
            custody: None,
        },
        &cleanup_context,
        &mut NoEffects,
        &effigy_kernel::run::Fence {
            resource: environment,
            generation: Generation::new(1),
        },
        effigy_kernel::supervise::Placement::Detached,
    )
    .expect("session admitted");
    test(run, session, &cx, &root, &observe)
}

/// The same fixture, already adopted by an independent supervisor.
fn with_serving_session<T>(
    test: impl for<'a> FnOnce(
        SessionSupervisor<'a>,
        SessionRef,
        &'a EffectContext,
        &'a StateRoot,
        &'a ObserveContext,
    ) -> T,
) -> T {
    with_admitted_session(|run, session, cx, root, observe| {
        let supervisor = SessionSupervisor::adopt(run, "452-supervisor").expect("adopted");
        test(supervisor, session, cx, root, observe)
    })
}

/// A transport whose bytes are scripted: it answers nothing on its own, so the
/// loop's dispatch is what the proof observes.
struct ScriptedTransport {
    input: Vec<u8>,
    cursor: usize,
    written: Arc<Mutex<Vec<u8>>>,
}

impl Transport for ScriptedTransport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.input.len() - self.cursor;
        if left == 0 {
            return Ok(0);
        }
        let take = left.min(buf.len());
        buf[..take].copy_from_slice(&self.input[self.cursor..self.cursor + take]);
        self.cursor += take;
        Ok(take)
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.written
            .lock()
            .expect("written")
            .extend_from_slice(bytes);
        Ok(())
    }
    fn verified_peer(&self) -> Option<host::ServerIdentity> {
        Some(host::ServerIdentity {
            digest: server_identity(),
        })
    }
}

fn framed(frames: &[host::Frame]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for frame in frames {
        let body = host::encode_frame(frame).expect("encoded frame");
        bytes.extend_from_slice(&(body.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&body);
    }
    bytes
}

/// Every frame the host wrote, decoded in order.
fn decoded(bytes: &[u8]) -> Vec<host::Frame> {
    let mut frames = Vec::new();
    let mut cursor = 0;
    while cursor + 4 <= bytes.len() {
        let mut prefix = [0u8; 4];
        prefix.copy_from_slice(&bytes[cursor..cursor + 4]);
        let length = u32::from_be_bytes(prefix) as usize;
        cursor += 4;
        frames.push(host::decode_frame(&bytes[cursor..cursor + length]).expect("a valid frame"));
        cursor += length;
    }
    assert_eq!(cursor, bytes.len(), "trailing bytes on the wire");
    frames
}

fn grants_for(capability: Secret) -> host::GrantBook {
    let mut grants = host::GrantBook::new();
    grants.issue(
        capability,
        grant_binding(),
        host::session_operations(),
        [Target::Host].into_iter().collect(),
    );
    grants
}

/// The client half of a served connection: negotiate and authenticate, then end
/// the stream, so the host's connection turn closes cleanly.
fn serving_connection() -> (ScriptedTransport, Arc<Mutex<Vec<u8>>>) {
    let binary = running_binary();
    let written = Arc::new(Mutex::new(Vec::new()));
    let input = framed(&[
        host::Frame::Hello(host::Hello {
            schema: host::SCHEMA.to_owned(),
            protocol_versions: vec![host::PROTOCOL_V1],
            effigy_version: binary.version,
            binary_digest: parse_digest(&binary.digest).expect("digest"),
            context_generation: Generation::new(GENERATION),
            session_run_id: session_run(),
        }),
        host::Frame::Authenticate(capability()),
    ]);
    (
        ScriptedTransport {
            input,
            cursor: 0,
            written: written.clone(),
        },
        written,
    )
}

/// An acceptor that refuses to hand over a connection before the session run
/// serves the channel, so the owner's ordering is what the proof measures.
struct ScriptedAccept {
    connections: VecDeque<ScriptedTransport>,
    accepted: usize,
}

impl ChannelAccept for ScriptedAccept {
    type Transport = ScriptedTransport;
    fn accept(
        &mut self,
        launch: &SessionLaunch,
        run: &Run<'_>,
    ) -> io::Result<Option<ScriptedTransport>> {
        assert!(
            launch.serving(run),
            "a connection was accepted before the session run served the channel"
        );
        self.accepted += 1;
        Ok(self.connections.pop_front())
    }
}

#[test]
fn a_connection_is_served_only_after_the_session_run_serves() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let (listener, key, _acceptor) = SessionTlsListener::bind(
            SocketAddr::from(([127u8, 0, 0, 1], 0)),
            Duration::from_secs(1),
        )
        .expect("bound session listener");
        let mut launch = SessionLaunch::launch(
            &mut supervisor,
            &session,
            "452-supervisor",
            key,
            Box::new(listener),
        )
        .expect("session listener custody");
        launch
            .qualify(&mut VerifiedProbe, Duration::from_secs(1))
            .expect("qualified");
        launch
            .publish(&supervisor)
            .expect("a qualified listener publishes its locator");
        let endpoint = launch.endpoint().expect("bound endpoint");
        assert!(
            !launch.serving(&supervisor),
            "a published launch does not serve until its owner says so"
        );

        let (connection, written) = serving_connection();
        let acceptor = ScriptedAccept {
            connections: VecDeque::from(vec![connection]),
            accepted: 0,
        };
        let server = host::Server::new(
            negotiation(),
            fence(),
            server_identity(),
            grants_for(capability()),
            host::Dispatcher::new(
                CountingAdmitter {
                    calls: calls.clone(),
                    root,
                    runs: Vec::new(),
                },
                CountingResolver {
                    calls: calls.clone(),
                    owner: session_task("guest-request").resource(),
                },
                CountingObserver {
                    calls: calls.clone(),
                },
                RunIds(VecDeque::new()),
            ),
        );
        let mut owner = ServingOwner::new(
            supervisor,
            launch,
            acceptor,
            server,
            Arc::new(cx.clone()),
            Arc::new(observe.clone()),
            root,
        );
        assert!(
            !owner.serving(),
            "no connection is dispatched before the owner serves"
        );

        let served = owner
            .serve_pending(&mut host::NoStreams)
            .expect("the owner served its connection")
            .expect("a connection was waiting");
        assert_eq!(served.responses, 0);
        assert!(owner.serving(), "the owner serves after its first turn");
        assert_eq!(owner.served(), 1);

        // The connection's negotiation and credential were answered through the
        // dispatcher this owner feeds.
        let frames = decoded(&written.lock().expect("written"));
        assert!(
            matches!(frames.first(), Some(host::Frame::HelloOk(_))),
            "the served connection negotiated"
        );

        // The listener lives only while the owner holds its custody.
        assert!(TcpStream::connect(endpoint).is_ok());
        owner.stop();
        assert!(
            !owner.key_held(),
            "the session key is wiped with the owner's custody"
        );
        assert!(
            TcpStream::connect(endpoint).is_err(),
            "no listener outlives its owner"
        );
    });
}

#[test]
fn dropping_the_owner_closes_the_socket_it_held() {
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let (listener, key, _acceptor) = SessionTlsListener::bind(
            SocketAddr::from(([127u8, 0, 0, 1], 0)),
            Duration::from_secs(1),
        )
        .expect("bound session listener");
        let mut launch = SessionLaunch::launch(
            &mut supervisor,
            &session,
            "452-supervisor",
            key,
            Box::new(listener),
        )
        .expect("session listener custody");
        launch
            .qualify(&mut VerifiedProbe, Duration::from_secs(1))
            .expect("qualified");
        launch.publish(&supervisor).expect("published");
        let endpoint = launch.endpoint().expect("bound endpoint");
        let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
        let server = host::Server::new(
            negotiation(),
            fence(),
            server_identity(),
            grants_for(capability()),
            host::Dispatcher::new(
                CountingAdmitter {
                    calls: calls.clone(),
                    root,
                    runs: Vec::new(),
                },
                CountingResolver {
                    calls: calls.clone(),
                    owner: session_task("guest-request").resource(),
                },
                CountingObserver {
                    calls: calls.clone(),
                },
                RunIds(VecDeque::new()),
            ),
        );
        let owner = ServingOwner::new(
            supervisor,
            launch,
            ScriptedAccept {
                connections: VecDeque::new(),
                accepted: 0,
            },
            server,
            Arc::new(cx.clone()),
            Arc::new(observe.clone()),
            root,
        );
        assert!(TcpStream::connect(endpoint).is_ok(), "the listener is live");
        drop(owner);
        assert!(
            TcpStream::connect(endpoint).is_err(),
            "dropping the owner closes the socket and wipes the key"
        );
    });
}

// ---------------------------------------------------------------------------
// Durable session custody: retained before the close, released in it.
// ---------------------------------------------------------------------------

fn custody_rows(root: &StateRoot, run: RunId) -> Vec<String> {
    let connection =
        rusqlite::Connection::open(root.path().join(effigy_kernel::store::DB_NAME)).expect("store");
    let mut query = connection
        .prepare("SELECT supervisor FROM workspace_session_custody WHERE run_id=?1")
        .expect("prepare");
    let rows = query
        .query_map([run.to_string()], |row| row.get::<_, String>(0))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("rows");
    rows
}

#[test]
fn a_session_keeps_its_custody_until_the_close_and_loses_it_there() {
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let session_run_id = session.run();
        // Adopting the run is what records the durable custody: the session is
        // an owner, and the kernel can check that against the journal.
        assert_eq!(
            custody_rows(root, session_run_id),
            vec!["452-supervisor".to_owned()],
            "an adopted session records its supervisor"
        );
        assert!(
            supervisor.session_owner().is_ok(),
            "the run that holds the row is the session's owner"
        );

        // Serving and stopping release the listener and the key, but the run is
        // not closed yet: a crash here must leave the recorded custody, so a
        // successor still finds the owner instead of silently losing it.
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let owner = ServingOwner::adopt_owned(
            OwnedWorkspaceChannel {
                supervisor,
                session,
                channel: prepared,
                effect: Arc::new(cx.clone()),
                observe: Arc::new(observe.clone()),
                root,
            },
            serving_server_for(Arc::new(Mutex::new(Calls::default())), root),
        )
        .expect("adopted");
        let mut owner = owner;
        owner.stop();
        assert!(!owner.key_held(), "stopping releases the key");
        assert_eq!(
            custody_rows(root, session_run_id),
            vec!["452-supervisor".to_owned()],
            "a crash before the close retains the recorded custody"
        );

        // The close transaction releases it, once, in the same commit that
        // closes the run.
        let cleanup_context = cleanup(cx);
        owner
            .settle(&cleanup_context, &mut NoEffects)
            .expect("settled");
        assert!(
            custody_rows(root, session_run_id).is_empty(),
            "the close releases the session's custody exactly once"
        );
        // Re-reading the same journal cannot resurrect it.
        assert!(custody_rows(root, session_run_id).is_empty());
    });
}
