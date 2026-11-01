//! §8.11 workspace-client proofs (effigy#452): the guest dispatch, the pinned
//! context, the client's refusals and replay, and the host side as lane A's
//! real server over in-memory transports.
//!
//! Every host engine seam is a strict fake: resolution, admission and
//! observation record what the guest asked for, and the kernel's own journal
//! records the reservation and the admission. Nothing here starts a container,
//! a supervisor or a real channel listener.

use super::context::v2::{PublishedGuestContext, WorkspaceContextV2};
use super::context::{
    context_path, sha256_text, snapshot_path, WorkspacePinnedSource, CHANNEL_PROTOCOL_VERSION,
    COMPOSED_MANIFEST_KIND,
};
use super::*;
use crate::runner::workspace_client::guest_fake::FakeGuest;
use crate::runner::workspace_client::identity::{
    GuestIdentity, GuestIdentityRefusal, GuestSelfObservation,
};
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
pub(super) const GUEST_CHECKOUT: &str = "/workspace";
const MANIFEST: &[u8] = b"[tasks]\nbuild = \"cargo build\"\nseed = \"cargo run -- seed\"\n";

fn running_binary() -> RunningBinary {
    RunningBinary::observe().expect("the test binary's own identity")
}

pub(super) fn session_run() -> RunId {
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

const CHANNEL_ENDPOINT: &str = "effigy-host:47121";
const CHANNEL_PIN_NOTE: &str = "pinned workspace host";

/// The enrolment credential body Prepare writes 0600, and the digest the strict
/// document pins for it.
fn enrolment_body() -> String {
    format!(
        "environment_generation={ENVIRONMENT_GENERATION}\ncontainer_id={CONTAINER_ID}\ncredential={}\n",
        secret_text(&Secret::new([0x11; 32]))
    )
}

fn enrolment_path() -> String {
    format!("/run/effigy/objects/enrol/{GENERATION}")
}

fn manifest_pin() -> WorkspacePinnedSource {
    WorkspacePinnedSource {
        kind: COMPOSED_MANIFEST_KIND.to_owned(),
        name: "composed".to_owned(),
        digest: sha256_text(MANIFEST),
        snapshot: Some(snapshot_path(&sha256_text(MANIFEST))),
    }
}

/// The strict v2 document a container-mode workspace publishes, built from the
/// same host facts `WorkspaceContextV2::prepared` renders and cross-parsed.
pub(super) fn workspace_document(
    session: Option<RunId>,
    pin: Option<String>,
) -> WorkspaceContextV2 {
    let binary = running_binary();
    let run = session.unwrap_or_else(session_run);
    let credential = enrolment_body();
    WorkspaceContextV2::parse(
        &serde_json::to_vec(&serde_json::json!({
            "schema": context::v2::SCHEMA,
            "context_generation": GENERATION,
            "environment_generation": ENVIRONMENT_GENERATION.parse::<u64>().expect("generation"),
            "session": { "run_id": run.to_string(), "generation": 1 },
            "artifact_digest": binary.digest,
            "binary_path": context::binary_path(&binary.digest),
            "container_id": CONTAINER_ID,
            "vm_boot_id": VM_BOOT,
            "platform": "x86_64",
            "libc": "gnu",
            "credential": {
                "generation": GENERATION,
                "path": enrolment_path(),
                "digest": sha256_text(credential.as_bytes()),
            },
            "locator": {
                "endpoint": CHANNEL_ENDPOINT,
                "pin": pin.unwrap_or_else(|| format!("sha256:{}", server_identity().hex())),
            },
            "selection_digest": Digest::of(b"selection").hex(),
            "version": binary.version,
            "protocol": CHANNEL_PROTOCOL_VERSION,
            "host_user": "operator",
            "checkout_id": "checkout",
            "environment_resource": "env",
            "backend": "docker",
            "profile": "default",
            "runtime": null,
            "vm_type": null,
            "paths": {
                "host_checkout": "/Users/operator/app",
                "guest_checkout": GUEST_CHECKOUT,
                "workdir": GUEST_CHECKOUT,
                "user": "app",
                "home": "/home/app",
            },
            "sources": [manifest_pin()],
            "routes": [],
        }))
        .expect("serialized document"),
    )
    .expect("a complete strict v2 document")
}

fn guest_observation() -> GuestSelfObservation {
    GuestSelfObservation {
        container_id: CONTAINER_ID.to_owned(),
        vm_boot_id: VM_BOOT.to_owned(),
        platform: "x86_64".to_owned(),
        libc: "gnu".to_owned(),
    }
}

/// The strict document already matched against this guest's own observation.
fn published(document: &WorkspaceContextV2) -> PublishedGuestContext {
    PublishedGuestContext {
        generation: document.context_generation,
        document: document.clone(),
        observation: guest_observation(),
    }
}

/// Publish one context generation, its manifest snapshot and (optionally) a
/// capability into a fake guest, exactly as Prepare would.
pub(super) fn publish(
    guest: &mut FakeGuest,
    document: &WorkspaceContextV2,
    capability: Option<&Secret>,
) {
    guest.put_file(
        context::CONTEXT_CURRENT_PATH,
        document.context_generation.to_string(),
        0o644,
    );
    guest.put_file(
        &context_path(document.context_generation),
        document.render().expect("rendered context"),
        0o444,
    );
    guest.put_file(&snapshot_path(&sha256_text(MANIFEST)), MANIFEST, 0o444);
    if let Some(capability) = capability {
        let stored = StoredCapability {
            generation: Generation::new(document.session.generation),
            secret: capability.clone(),
        };
        // A dispatch under test presents the capability the way a terminal would,
        // so this process declares the client the fixture publishes for (an entry
        // exports exactly this per terminal).
        std::env::set_var("EFFIGY_WORKSPACE_CLIENT_ID", TEST_CLIENT);
        guest.put_file(
            &client_capability_path(TEST_CLIENT),
            render_capability(&stored).as_bytes(),
            0o600,
        );
    }
}

/// One terminal's own name in the capability-storage proofs.
const TEST_CLIENT: &str = "test-terminal";

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

/// One scripted guest-local observation: the production reader's shape, so a
/// proof can present a matching or a disagreeing container without touching
/// `/proc`.
struct ObservedIt(Option<GuestSelfObservation>);

impl GuestIdentity for ObservedIt {
    fn observe(&mut self) -> Result<GuestSelfObservation, GuestIdentityRefusal> {
        self.0.clone().ok_or(GuestIdentityRefusal::Unavailable)
    }
}

/// A proof that a local read never opens a connection.
struct NoChannel;

impl ChannelOpener for NoChannel {
    fn open(
        &mut self,
        _: &VerifiedWorkspaceContext,
        _: &LiveGuestRun,
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
    ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
        self.opened += 1;
        Err(ChannelError::Refused {
            code: self.code,
            message: self.message.clone(),
        })
    }
}

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

fn parsed(words: &[&str]) -> Command {
    parse_command(argv(words)).expect("a parsable command")
}

#[test]
fn a_pinned_task_list_is_read_from_the_context_without_a_channel() {
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, None);
    let before = guest.tree_digest();
    let mut checkout = FakeCheckout::default();

    let rendered = dispatch_in(
        &parsed(&["tasks"]),
        &argv(&["tasks"]),
        &published(&context),
        &mut guest,
        &mut checkout,
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
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, None);
    let before = guest.tree_digest();
    let mut stale = published(&context);
    stale.generation = GENERATION - 1;
    let mut checkout = FakeCheckout::default();

    let error = dispatch_in(
        &parsed(&["tasks"]),
        &argv(&["tasks"]),
        &stale,
        &mut guest,
        &mut checkout,
        &mut NoChannel,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("a stale generation refuses");

    assert!(error.to_string().contains("stale"), "{error}");
    assert_eq!(guest.tree_digest(), before);
}

#[test]
fn guest_local_reads_are_answered_from_the_strict_v2_document() {
    let document = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &document, None);
    let before = guest.tree_digest();
    let mut checkout = FakeCheckout::default();
    let mut reads = GuestLocalReads {
        context: &document,
        guest: &mut guest,
        checkout: &mut checkout,
    };

    let version = reads
        .read(&LocalRead::Version, GENERATION)
        .expect("the pinned version");
    assert_eq!(
        version,
        LocalReadOutcome::Version {
            host_version: document.version.clone(),
            guest_version: document.version.clone(),
            digest: document.artifact_digest.clone(),
            protocol: CHANNEL_PROTOCOL_VERSION,
            context_generation: GENERATION,
        }
    );
    assert_eq!(
        reads
            .read(&LocalRead::Sources, GENERATION)
            .expect("sources"),
        LocalReadOutcome::Sources(document.sources.clone())
    );
    assert_eq!(
        reads.read(&LocalRead::Tasks, GENERATION).expect("tasks"),
        LocalReadOutcome::Tasks(vec!["build".to_owned(), "seed".to_owned()])
    );
    // A local read admits nothing, enrols nothing and writes nothing.
    assert_eq!(guest.tree_digest(), before);
    assert_eq!(checkout.reads, 0);
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
            run_id: None,
            op: "doctor".to_owned(),
            target: Target::Host,
        }
    );
    assert_eq!(
        classify(&parsed(&["container", "status"])),
        GuestOperation::Observe {
            run_id: None,
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
            run_id: None,
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
    let document = workspace_document(Some(session_run()), None);
    let paths = document.paths.clone();
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
fn detect_selects_the_strict_document_only_after_the_guest_observes_its_own_container() {
    let document = workspace_document(Some(session_run()), None);
    for observed in [
        None,
        Some(GuestSelfObservation {
            container_id: "b2".repeat(32),
            ..guest_observation()
        }),
        Some(GuestSelfObservation {
            vm_boot_id: "another-boot".to_owned(),
            ..guest_observation()
        }),
    ] {
        let mut guest = FakeGuest::new();
        publish(&mut guest, &document, None);
        let error = detect_with(&mut guest, &mut ObservedIt(observed)).expect_err("unproven");
        assert!(
            error.to_string().contains("identity") || error.to_string().contains("unproven"),
            "{error}"
        );
    }
    let mut guest = FakeGuest::new();
    publish(&mut guest, &document, None);
    let placement =
        detect_with(&mut guest, &mut ObservedIt(Some(guest_observation()))).expect("detect");
    match placement {
        GuestPlacement::Guest(published) => {
            assert_eq!(published.generation, GENERATION);
            assert_eq!(published.document, document);
            assert_eq!(published.observation, guest_observation());
        }
        GuestPlacement::Host => panic!("a published strict document is a guest"),
    }
}

#[test]
fn a_v1_document_is_refused_instead_of_being_reinterpreted_as_v2() {
    let document = workspace_document(Some(session_run()), None);
    let mut bytes = serde_json::to_value(&*document).expect("json");
    bytes["generation"] = GENERATION.into();
    let mut guest = FakeGuest::new();
    guest.put_file(context::CONTEXT_CURRENT_PATH, GENERATION.to_string(), 0o644);
    guest.put_file(
        &context_path(GENERATION),
        serde_json::to_vec(&bytes).expect("bytes"),
        0o444,
    );
    let error =
        detect_with(&mut guest, &mut ObservedIt(Some(guest_observation()))).expect_err("v1");
    assert!(error.to_string().contains("strict full v2"), "{error}");
}

#[test]
fn a_context_whose_locator_is_not_a_qualified_alias_refuses_before_the_connection() {
    let mut context = workspace_document(Some(session_run()), None);
    context.0.locator.endpoint = "unix:///run/effigy/channel".to_owned();
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let before = guest.tree_digest();
    let mut checkout = FakeCheckout::default();
    let mut opener = RefusingChannel {
        code: "unavailable",
        message: "the opener must never be reached".to_owned(),
        opened: 0,
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &[],
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("an unqualified locator refuses");
    assert_eq!(opener.opened, 0, "no connection was attempted");

    assert!(error.to_string().contains("unavailable"), "{error}");
    assert!(
        !error.to_string().contains("effigy-host:47121"),
        "the guest reported its own sentence: {error}"
    );
    assert_eq!(guest.tree_digest(), before, "a refusal writes nothing");
    assert_eq!(checkout.reads, 0, "a refusal never falls back to a read");
}

#[test]
fn a_host_refusal_returns_an_error_and_never_falls_back() {
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let before = guest.tree_digest();
    let mut checkout = FakeCheckout::with_file("effigy.toml", b"[tasks]\nbuild = \"\"\n");
    let mut opener = RefusingChannel {
        code: "unauthenticated",
        message: "this capability was not issued by this session".to_owned(),
        opened: 0,
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &[],
        &published(&context),
        &mut guest,
        &mut checkout,
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
    let mut context = workspace_document(Some(session_run()), None);
    context.0.version = "0.0.0-other".to_owned();
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let mut checkout = FakeCheckout::default();
    let mut opener = RefusingChannel {
        code: "unavailable",
        message: "the opener must never be reached".to_owned(),
        opened: 0,
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &[],
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("the binary is not the pinned one");

    assert!(error.to_string().contains("version_mismatch"), "{error}");
    assert_eq!(opener.opened, 0, "no credential crossed the connection");
}

#[test]
fn an_identity_mismatch_refuses_before_the_connection() {
    let context = workspace_document(Some(session_run()), None);
    let mut mismatched = published(&context);
    mismatched.observation = GuestSelfObservation {
        container_id: "c3".repeat(32),
        ..guest_observation()
    };
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let mut checkout = FakeCheckout::default();
    let mut opener = RefusingChannel {
        code: "unavailable",
        message: "the opener must never be reached".to_owned(),
        opened: 0,
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &[],
        &mismatched,
        &mut guest,
        &mut checkout,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("another container");

    assert!(error.to_string().contains("identity_mismatch"), "{error}");
    assert_eq!(opener.opened, 0);
}

#[test]
fn a_direct_terminal_enrols_with_the_published_session_generation_and_credential() {
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, None);
    guest.put_file(&enrolment_path(), enrolment_body(), 0o600);

    let auth = load_auth(&mut guest, &context, TEST_CLIENT).expect("the published credential");
    match auth {
        GuestAuth::Enrolment {
            session,
            credential: _,
        } => {
            assert_eq!(session.run, session_run());
            assert_eq!(session.generation, Generation::new(1));
        }
        other => panic!("a direct terminal enrols, not {other:?}"),
    }

    // Bytes the document did not pin are refused instead of presented.
    guest.put_file(&enrolment_path(), b"credential=not-the-staged-one\n", 0o600);
    let error = load_auth(&mut guest, &context, TEST_CLIENT).expect_err("a replaced credential");
    assert!(error.to_string().contains("context pins"), "{error}");
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

pub(super) fn cleanup(cx: &EffectContext) -> CleanupContext {
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

/// #507's supervision proofs settle the same session with the same adapter.
#[derive(Debug)]
pub(super) struct NoEffects;

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
    let document = workspace_document(Some(session_run()), None);
    live_guest_run(&published(&document), running_binary())
}

fn verified() -> VerifiedWorkspaceContext {
    let document = workspace_document(Some(session_run()), None);
    document
        .verify(&mut ObservedIt(Some(guest_observation())), &live_run())
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
    fn admitted_fence(
        &self,
        ticket: &effigy_kernel::run::AdmissionTicket,
    ) -> Option<effigy_kernel::run::Fence> {
        self.runs
            .iter()
            .find(|run| run.id() == ticket.run())
            .map(|run| run.fence().clone())
    }
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
            capabilities: vec![host::ChannelCapability::ObserveQueryV1],
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
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let mut checkout = FakeCheckout::default();
    let mut opener = PeerChannel {
        connections: vec![client_side],
    };

    // A task invocation now takes the Level 1 forwarding path (it streams to a
    // terminal outcome), so this seam proof uses the other authority-bearing
    // request shape the channel carries: a named host operation. The Level 1
    // command policy narrows which operations a guest may send, not how an
    // admitted request reaches the host.
    let rendered = dispatch_in(
        &parsed(&["container", "down"]),
        &["container".into(), "down".into()],
        &published(&context),
        &mut guest,
        &mut checkout,
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
        assert_eq!(calls.selector.as_deref(), Some("container:down"));
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
    ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
        if self.connections.is_empty() {
            return Err(ChannelError::local("the proof ran out of connections"));
        }
        let connection = self.connections.remove(0);
        let client = WorkspaceClient::connect(
            PeerTransport::new(connection),
            verified.clone(),
            live.clone(),
        )?;
        Ok(Box::new(client))
    }
}

#[test]
fn a_wrong_capability_refuses_before_any_effect() {
    // Coalesce these writes as socket buffering can, so the request write
    // completes before the host rejects authentication and closes. A failed
    // request write has a different, indeterminate outcome tested separately.
    struct CoalescedAuthentication(PeerTransport, Vec<u8>);
    impl Transport for CoalescedAuthentication {
        fn verified_peer(&self) -> Option<host::ServerIdentity> {
            self.0.verified_peer()
        }
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if !self.1.is_empty() {
                self.0.write_all(&std::mem::take(&mut self.1))?;
            }
            self.0.read(bytes)
        }
        fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
            if matches!(host::decode_frame(&bytes[4..]), Ok(Frame::Authenticate(_))) {
                self.1.extend_from_slice(bytes);
                Ok(())
            } else if self.1.is_empty() {
                self.0.write_all(bytes)
            } else {
                self.1.extend_from_slice(bytes);
                self.0.write_all(&std::mem::take(&mut self.1))
            }
        }
    }
    struct CoalescedPeer(Option<UnixStream>);
    impl ChannelOpener for CoalescedPeer {
        fn open(
            &mut self,
            verified: &VerifiedWorkspaceContext,
            live: &LiveGuestRun,
        ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
            WorkspaceClient::connect(
                CoalescedAuthentication(PeerTransport::new(self.0.take().unwrap()), Vec::new()),
                verified.clone(),
                live.clone(),
            )
            .map(|client| Box::new(client) as Box<dyn ChannelRequest>)
        }
    }
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let (client_side, host_side) = connection_pair();
    let (served, handle) = serve_host(vec![host_side], calls.clone(), &[9]);
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    // A capability this host never issued.
    publish(&mut guest, &context, Some(&Secret::new([0xcd; 32])));
    let mut checkout = FakeCheckout::default();
    let mut opener = CoalescedPeer(Some(client_side));

    let error = dispatch_in(
        &parsed(&["build"]),
        &[],
        &published(&context),
        &mut guest,
        &mut checkout,
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

    let mut first =
        WorkspaceClient::connect(PeerTransport::new(first_client), verified(), live_run())
            .expect("the first connection");
    first
        .authenticate(GuestAuth::Capability(capability()))
        .expect("authenticated");
    let admitted = first.send(&intent).expect("the host admitted");
    drop(first);
    let ChannelOutcome::Accepted { run, .. } = admitted else {
        panic!("a fresh invocation is admitted");
    };

    // A reconnect replays the same intent under the same id: the host answers
    // from the record it wrote, and never admits a second run.
    let mut second =
        WorkspaceClient::connect(PeerTransport::new(second_client), verified(), live_run())
            .expect("the second connection");
    second
        .authenticate(GuestAuth::Capability(capability()))
        .expect("authenticated");
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
fn an_unanswered_attempt_is_unknown_and_never_resends() {
    let mut client = WorkspaceClient::connect(
        MuteTransport::answering_negotiation(),
        verified(),
        live_run(),
    )
    .expect("the negotiation answered");
    client
        .authenticate(GuestAuth::Capability(capability()))
        .expect("authenticated");
    let intent = Intent::new(RequestBody::Observe(ObserveBody {
        query: None,
        run_id: None,
        op: "doctor".to_owned(),
        target: Some(Target::Host),
    }));

    let error = client.send(&intent).expect_err("the host went silent");

    match error {
        ChannelError::Unknown { .. } => {}
        other => panic!("an unanswered request is Unknown, not {other:?}"),
    }
}

#[test]
fn a_secondary_terminal_cannot_detach_the_owners_run() {
    let transport = MuteTransport::answering_negotiation();
    let written = transport.written.clone();
    let mut client = WorkspaceClient::connect(transport, verified(), live_run())
        .expect("the negotiation answered");
    client
        .authenticate(GuestAuth::Capability(capability()))
        .expect("authenticated");
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
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let mut checkout = FakeCheckout::default();
    let mut opener = PeerChannel {
        connections: vec![client_side],
    };

    let rendered = dispatch_in(
        &parsed(&["doctor"]),
        &argv(&["doctor"]),
        &published(&context),
        &mut guest,
        &mut checkout,
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

#[test]
fn guest_run_status_reaches_real_host_dispatch() {
    let id = RunId::new(42, [0x42; 10]).expect("run id");
    let (client_side, host_side) = connection_pair();
    let handle = std::thread::spawn(move || {
        let fixture = effigy_test_support::Fixture::new("workspace-status").expect("fixture");
        let cx = effects();
        let root = StateRoot::effects(&cx, fixture.path()).expect("root");
        let owner = session_task("status").resource();
        let run = Run::begin(
            &cx,
            &root,
            &resolved(owner),
            BeginOpts {
                id,
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next_argv(),
                custody: None,
            },
            &cleanup(&cx),
            &mut NoEffects,
        )
        .expect("run");
        let calls = Arc::new(Mutex::new(Calls::default()));
        let mut grants = host::GrantBook::new();
        grants.issue(
            capability(),
            grant_binding(),
            host::session_operations(),
            [Target::Host].into_iter().collect(),
        );
        let checkout = CheckoutId::new(CheckoutKind::EphemeralClone, [7; 16]).text();
        let observer = super::host_dispatch::HostDispatch::for_serving(
            fixture.path().to_owned(),
            root.clone(),
            &checkout,
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
                    calls,
                    owner: session_task("guest-request").resource(),
                },
                observer,
                RunIds(VecDeque::new()),
            ),
        );
        let result = server.serve(
            &mut PeerTransport::new(host_side),
            &cx,
            &observation(),
            &root,
            &mut host::NoStreams,
        );
        drop(run);
        result.expect("served status");
    });
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let mut checkout = FakeCheckout::default();
    let mut opener = PeerChannel {
        connections: vec![client_side],
    };
    let run_id = id.to_string();
    let output = dispatch_in(
        &parsed(&["run", "status", &run_id]),
        &argv(&["run", "status", &run_id]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect("status from host");
    assert!(output.contains(&run_id), "{output}");
    assert!(output.contains("running"), "{output}");
    handle.join().expect("host thread");
}

#[test]
fn run_status_refuses_stale_context_before_dial() {
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let before = guest.tree_digest();
    let mut stale = published(&context);
    stale.generation = GENERATION - 1;
    let mut checkout = FakeCheckout::default();
    let id = session_run().to_string();
    let error = dispatch_in(
        &parsed(&["run", "status", &id]),
        &argv(&["run", "status", &id]),
        &stale,
        &mut guest,
        &mut checkout,
        &mut NoChannel,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("stale status refuses");
    assert!(error.to_string().contains("stale"), "{error}");
    assert_eq!(guest.tree_digest(), before);
}

#[test]
fn run_status_with_unissued_capability_never_reaches_observer() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let (client_side, host_side) = connection_pair();
    let (served, handle) = serve_host(vec![host_side], calls.clone(), &[]);
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&Secret::new([0x91; 32])));
    let mut checkout = FakeCheckout::default();
    let mut opener = PeerChannel {
        connections: vec![client_side],
    };
    let id = session_run().to_string();
    dispatch_in(
        &parsed(&["run", "status", &id]),
        &argv(&["run", "status", &id]),
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("unissued capability refuses");
    drop(opener);
    handle.join().expect("host thread");
    assert!(calls.lock().expect("calls").observed.is_empty());
    let served = served.lock().expect("served");
    assert!(
        served[0].is_err(),
        "authentication must refuse before observation"
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
    let mut context = workspace_document(Some(session_run()), None);
    // A ready locator whose pin is not a digest: the production transport
    // refuses it before dialling, and the guest reports unavailable.
    context.0.locator.pin = "sha256:not-a-digest".to_owned();
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let before = guest.tree_digest();
    let mut checkout = FakeCheckout::default();
    let mut opener = PinnedChannelTransport {
        backend: "docker".to_owned(),
        budget: CHANNEL_OPEN_BUDGET,
    };

    let error = dispatch_in(
        &parsed(&["build"]),
        &[],
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("an unusable locator refuses");

    assert!(error.to_string().contains("unavailable"), "{error}");
    assert_eq!(guest.tree_digest(), before);
}

#[test]
fn a_mispinned_peer_receives_no_hello_and_no_credential() {
    let transport = MuteTransport::answering_negotiation_with_peer(mispinned());
    let written = transport.written.clone();
    // A real capability is present in guest storage: a mispinned peer must
    // never cause it to be read, let alone sent.
    let error = WorkspaceClient::connect(transport, verified(), live_run())
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
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let mut checkout = FakeCheckout::default();
    let written = Arc::new(Mutex::new(Vec::new()));
    let mut opener = ScriptedPeerChannel {
        peer: mispinned(),
        written: written.clone(),
    };

    // A real enrolment credential is staged: a mispinned peer must never cause
    // it to be read, because the pin is checked first.
    let mut guarded = NoCredentialReads { inner: guest };
    guarded
        .inner
        .put_file(&enrolment_path(), enrolment_body(), 0o600);
    let before = guarded.inner.tree_digest();

    let error = dispatch_in(
        &parsed(&["build"]),
        &[],
        &published(&context),
        &mut guarded,
        &mut checkout,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .expect_err("the peer is not the pinned host");

    assert!(error.to_string().contains("identity_mismatch"), "{error}");
    assert!(written.lock().expect("written").is_empty());
    assert_eq!(
        guarded.inner.tree_digest(),
        before,
        "a pin refusal writes nothing"
    );
}

/// A guest whose enrolment credential read is a hard failure: the proof that a
/// mispinned peer never reaches the credential at all.
struct NoCredentialReads {
    inner: FakeGuest,
}

impl GuestReader for NoCredentialReads {
    fn read_file(&mut self, path: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        assert_ne!(
            path,
            enrolment_path(),
            "the credential was read before the pinned peer verified"
        );
        self.inner.read_file(path)
    }

    fn file_sha256(&mut self, path: &str) -> Result<Option<String>, RunnerError> {
        self.inner.file_sha256(path)
    }
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
    ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
        let mut transport = MuteTransport::answering_negotiation_with_peer(self.peer.clone());
        transport.written = self.written.clone();
        let client = WorkspaceClient::connect(transport, verified.clone(), live.clone())?;
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
    let mut owner =
        WorkspaceClient::connect(PeerTransport::new(client_side), verified(), live_run())
            .expect("the owning connection");
    owner
        .authenticate(GuestAuth::Capability(capability()))
        .expect("authenticated");
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
    // This definitive answer permits a known-safe replay on the same
    // connection. An indeterminate delivery is fenced instead.
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

    let mut owner =
        WorkspaceClient::connect(PeerTransport::new(owner_client), verified(), live_run())
            .expect("the owning connection");
    owner
        .authenticate(GuestAuth::Capability(capability()))
        .expect("authenticated");
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
    let mut terminal =
        WorkspaceClient::connect(PeerTransport::new(cursor_client), verified(), live_run())
            .expect("the attaching terminal");
    terminal
        .authenticate(GuestAuth::Capability(capability()))
        .expect("authenticated");
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
pub(super) fn prepare_owned_channel(
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
            .map(|port| SocketAddr::from(([127, 0, 0, 1], port)))
            .expect("a published alias endpoint");

        let owner = ServingOwner::adopt_owned(
            OwnedWorkspaceChannel {
                supervisor,
                session,
                serving: super::ServingFacts::fixture(
                    &prepared.locator,
                    Path::new(GUEST_CHECKOUT).to_path_buf(),
                ),
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
                serving: super::ServingFacts::fixture(
                    &prepared.locator,
                    Path::new(GUEST_CHECKOUT).to_path_buf(),
                ),
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
pub(super) fn with_serving_session<T>(
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
            capabilities: Vec::new(),
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
            session.clone(),
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
            session.clone(),
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
                serving: super::ServingFacts::fixture(
                    &prepared.locator,
                    Path::new(GUEST_CHECKOUT).to_path_buf(),
                ),
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

#[test]
fn the_production_owner_builds_its_server_only_from_matching_validated_facts() {
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let endpoint = prepared.launch.endpoint().expect("listener");
        // A pin the publication did not validate refuses before serving.
        let mut wrong = super::ServingFacts::fixture(
            &prepared.locator,
            Path::new(GUEST_CHECKOUT).to_path_buf(),
        );
        wrong.pin = Digest([1; 32]);
        let refused = ServingOwner::adopt_production(OwnedWorkspaceChannel {
            supervisor,
            session,
            serving: wrong,
            channel: prepared,
            effect: Arc::new(cx.clone()),
            observe: Arc::new(observe.clone()),
            root,
        });
        let Err((AdoptError::Facts, mut owned)) = refused else {
            panic!("mismatched pin must refuse")
        };
        assert!(
            std::net::TcpStream::connect(endpoint).is_ok(),
            "refusal retains custody for explicit settlement"
        );
        owned.channel.launch.stop();
        owned
            .supervisor
            .settle(&cleanup(cx), &mut NoEffects)
            .expect("settle the same session");
        assert!(
            std::net::TcpStream::connect(endpoint).is_err(),
            "settlement closes the listener"
        );
    });

    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let facts = super::ServingFacts::fixture(
            &prepared.locator,
            Path::new(GUEST_CHECKOUT).to_path_buf(),
        );
        let mut owner = ServingOwner::adopt_production(OwnedWorkspaceChannel {
            supervisor,
            session,
            serving: facts,
            channel: prepared,
            effect: Arc::new(cx.clone()),
            observe: Arc::new(observe.clone()),
            root,
        })
        .expect("the validated facts and prepared channel agree");
        assert!(
            owner.launch().is_some(),
            "the production owner retains the listener"
        );
        assert!(owner.key_held(), "the production owner retains the key");
        assert!(
            !owner.serving(),
            "a channel is served only after the adopted session run serves"
        );
        owner.stop();
        assert!(!owner.key_held(), "stopping the owner wipes the key");
    });
}

// ---------------------------------------------------------------------------
// The scoped, per-client capability handoff (§8.8, §8.11; effigy#507).
// ---------------------------------------------------------------------------

/// Every terminal has its own capability path, so no client can present or
/// overwrite another's authority.
#[test]
fn a_capability_belongs_to_one_named_client() {
    assert_ne!(
        client_capability_path("terminal-a"),
        client_capability_path("terminal-b")
    );
    assert!(
        !client_capability_path("terminal-a").ends_with("/capability"),
        "there is no shared capability file"
    );
    assert_eq!(
        client_capability_path("terminal-a"),
        format!("{CLIENT_ROOT}/capabilities/terminal-a")
    );
}

/// Storage is 0600 inside a 0700 directory, replaces only that client's own
/// record, and reads back as the same capability for the same generation.
#[test]
fn a_stored_capability_is_private_replaceable_and_readable() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("a private fixture root");
    let first = StoredCapability {
        generation: Generation::new(1),
        secret: capability(),
    };
    store_capability(root.path(), "terminal-a", &first).expect("stored");
    let path = root.path().join("capabilities/terminal-a");
    let mode = std::fs::metadata(&path)
        .expect("the record exists")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "a capability is owner-only");
    let directory = std::fs::metadata(root.path().join("capabilities"))
        .expect("the directory exists")
        .permissions()
        .mode();
    assert_eq!(
        directory & 0o777,
        0o700,
        "the capability directory is private"
    );

    let text = std::fs::read_to_string(&path).expect("the record is readable");
    let stored = parse_capability(&text).expect("the record parses");
    assert_eq!(stored.generation, Generation::new(1));
    assert!(
        stored.secret.matches(&capability()),
        "the stored capability is the one that was written"
    );

    // A successor generation replaces this client's own record and leaves the
    // other client's alone.
    let other = StoredCapability {
        generation: Generation::new(1),
        secret: Secret::new([0xcd; 32]),
    };
    store_capability(root.path(), "terminal-b", &other).expect("stored");
    let successor = StoredCapability {
        generation: Generation::new(2),
        secret: Secret::new([0xef; 32]),
    };
    store_capability(root.path(), "terminal-a", &successor).expect("replaced");
    let replaced =
        parse_capability(&std::fs::read_to_string(&path).expect("the record is readable"))
            .expect("the record parses");
    assert_eq!(replaced.generation, Generation::new(2));
    let untouched = parse_capability(
        &std::fs::read_to_string(root.path().join("capabilities/terminal-b"))
            .expect("the other record is readable"),
    )
    .expect("the other record parses");
    assert!(
        untouched.secret.matches(&other.secret),
        "another client's capability is never replaced"
    );
}

/// A capability from another generation is never presented: the host revokes it
/// with its generation, so the terminal enrols again instead of replaying it.
#[test]
fn a_stale_capability_is_not_presented() {
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, None);
    guest.put_file(&enrolment_path(), enrolment_body(), 0o600);
    // A capability the previous generation issued this client.
    guest.put_file(
        &client_capability_path(TEST_CLIENT),
        render_capability(&StoredCapability {
            generation: Generation::new(context.session.generation - 1),
            secret: capability(),
        })
        .as_bytes(),
        0o600,
    );

    match load_auth(&mut guest, &context, TEST_CLIENT).expect("a fallback exists") {
        GuestAuth::Enrolment { session, .. } => {
            assert_eq!(
                session.generation,
                Generation::new(context.session.generation)
            )
        }
        other => panic!("a stale capability is never presented, got {other:?}"),
    }

    // The same capability at the context's own generation is presented.
    guest.put_file(
        &client_capability_path(TEST_CLIENT),
        render_capability(&StoredCapability {
            generation: Generation::new(context.session.generation),
            secret: capability(),
        })
        .as_bytes(),
        0o600,
    );
    match load_auth(&mut guest, &context, TEST_CLIENT).expect("the capability is presented") {
        GuestAuth::Capability(presented) => {
            assert!(presented.matches(&capability()), "the stored capability")
        }
        other => panic!("a current capability is presented, got {other:?}"),
    }

    // A record that is not a capability refuses instead of being guessed at.
    guest.put_file(
        &client_capability_path(TEST_CLIENT),
        b"capability=not-a-capability\n",
        0o600,
    );
    let error =
        load_auth(&mut guest, &context, TEST_CLIENT).expect_err("an unreadable record refuses");
    assert!(error.to_string().contains("is not a capability"), "{error}");
}

/// A capability is authority-bearing from the moment the host issues it, so it
/// comes from the system entropy source or not at all: no clock or pid fallback.
#[test]
fn a_capability_is_never_minted_from_a_weaker_secret() {
    let refused = super::capability_entropy(|_| Err(std::io::Error::other("no entropy device")))
        .expect_err("an unavailable entropy source issues no capability");
    assert!(
        refused.to_string().contains("entropy"),
        "the refusal names the missing source: {refused}"
    );

    // A source that fills the buffer is used as-is, and no bytes are invented.
    let minted = super::capability_entropy(|bytes| {
        bytes.fill(0x5c);
        Ok(())
    })
    .expect("a full read mints the capability");
    assert_eq!(minted, [0x5c; 32]);
}

/// Two independent real pseudo-terminals resolve two client identities, and one
/// terminal resolves one. This is the isolation the capability store depends on;
/// the full process harness belongs to the terminal owner (#485), so this proof
/// exercises the identity derivation over real pty device numbers.
#[test]
fn two_real_pseudo_terminals_resolve_two_client_identities() {
    use std::os::fd::{FromRawFd, OwnedFd};

    let open = || -> (OwnedFd, OwnedFd) {
        let mut master: libc::c_int = -1;
        let mut slave: libc::c_int = -1;
        // SAFETY: openpty opens both descriptors and writes them to the out
        // parameters; no termios or winsize is supplied or read.
        let result = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(result, 0, "a pseudo-terminal is available");
        // SAFETY: both descriptors were opened above and are owned from here on.
        (unsafe { OwnedFd::from_raw_fd(master) }, unsafe {
            OwnedFd::from_raw_fd(slave)
        })
    };
    let device = |fd: &OwnedFd| -> u64 {
        use std::os::fd::AsRawFd;
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        // SAFETY: fstat writes into the zeroed struct for the borrowed descriptor.
        assert_eq!(unsafe { libc::fstat(fd.as_raw_fd(), &mut stat) }, 0);
        assert_ne!(stat.st_rdev, 0, "a pty slave is a terminal device");
        stat.st_rdev as u64
    };

    let (_master_a, slave_a) = open();
    let (_master_b, slave_b) = open();
    let first =
        super::terminal_client_identity(Some(device(&slave_a)), 4242).expect("a terminal identity");
    let same =
        super::terminal_client_identity(Some(device(&slave_a)), 4242).expect("a terminal identity");
    let second =
        super::terminal_client_identity(Some(device(&slave_b)), 4242).expect("a terminal identity");
    assert_eq!(first, same, "one terminal keeps one client identity");
    assert_ne!(
        first, second,
        "two independent pseudo-terminals are two clients: {first} vs {second}"
    );
    assert!(
        first.starts_with("tty-"),
        "a terminal client is identified by its device: {first}"
    );
    // The capability handoff itself: each identity names its own record, so two
    // terminal clients can never read or replace one file.
    let first_path = super::client_capability_path(&first);
    let second_path = super::client_capability_path(&second);
    assert_ne!(
        first_path, second_path,
        "two terminal clients have two capability records"
    );
    let root = tempfile::tempdir().expect("a private fixture root");
    for (path, secret) in [(&first, 0xa1u8), (&second, 0xb2u8)] {
        super::store_capability(
            root.path(),
            path,
            &super::StoredCapability {
                generation: Generation::new(1),
                secret: Secret::new([secret; 32]),
            },
        )
        .expect("the terminal's capability is stored");
    }
    let stored = std::fs::read_dir(root.path().join("capabilities"))
        .expect("the capability directory exists")
        .count();
    assert_eq!(stored, 2, "each terminal client keeps its own record");
}

// ---------------------------------------------------------------------------
// The workspace terminal owner (effigy#485): the session/run-bound `Terminal`,
// its retained stream adapter, and its refusals through the real channel loop.
//
// Every connection below is the kernel's own `Server::serve` over a
// `ScriptedTransport`, with the real journal behind it: the run is admitted by
// the kernel's reservation path, its attachment rows are the kernel's, and the
// terminal frames go through the kernel's validation first. A terminal is
// opened once its run is admitted (a run's terminal writer belongs to an
// admitted run), and the connection that types into it attaches afterwards on
// the same server. Each owner drives its writer from its own pump, so delivery
// and retirement are observed by waiting rather than by assuming a connection
// turn. The writers are production-shaped fakes: bounded, non-blocking and
// externally observable. The live child-PTY binding is effigy#519.
// ---------------------------------------------------------------------------

use super::terminal::{TerminalCustody, TerminalRefusal, TerminalWriter};
use effigy_kernel::custody::Stream;
use effigy_kernel::root::Family;
use effigy_kernel::run::{self, AttachmentId, Client, Release, Session};
use effigy_kernel::workspace_channel::{StreamStart, Streams, TerminalSize};

/// The fake writer's own record: what it took, what it applied, and whether it
/// was retired. `capacity` is the test's knob for a child that stops reading and
/// then starts again.
#[derive(Debug)]
struct PtyLog {
    written: Vec<u8>,
    sizes: Vec<TerminalSize>,
    retired: usize,
    capacity: usize,
    /// Whether the resize handle accepts a geometry at all. Tests close and
    /// reopen it to place a resize effect either side of a lifetime change.
    resize_open: bool,
    /// Every geometry the owner offered this handle, accepted or refused, in
    /// order. A proof uses it to observe that a frame was processed without
    /// depending on how many turns the pump took.
    resize_attempts: Vec<TerminalSize>,
}

impl Default for PtyLog {
    fn default() -> Self {
        Self {
            written: Vec::new(),
            sizes: Vec::new(),
            retired: 0,
            capacity: usize::MAX,
            resize_open: true,
            resize_attempts: Vec::new(),
        }
    }
}

/// A one-shot action a fake writer runs from inside an effect, so a proof can
/// change the owning attachment's lifetime between two of them.
struct PtyHook(Box<dyn FnMut() + Send>);

impl std::fmt::Debug for PtyHook {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PtyHook")
    }
}

/// A production-shaped fake terminal writer: bounded and non-blocking, and it
/// records exactly what a child PTY master would report. It is not a real child.
#[derive(Debug)]
struct FakePty {
    log: Arc<Mutex<PtyLog>>,
    /// Bytes one write call accepts.
    batch: usize,
    /// Runs once, after the first write effect returns.
    after_first_write: Option<PtyHook>,
}

impl FakePty {
    fn new(log: Arc<Mutex<PtyLog>>) -> Self {
        Self {
            log,
            batch: usize::MAX,
            after_first_write: None,
        }
    }

    /// A writer that accepts `batch` bytes per call and holds `capacity` in
    /// total, as a child whose terminal buffer fills does.
    fn bounded(self, batch: usize, capacity: usize) -> Self {
        self.log.lock().expect("pty log").capacity = capacity;
        Self { batch, ..self }
    }

    /// Run `hook` once, after the first write effect returns: the proof that the
    /// owner's lifetime is read again before the *next* partial write.
    fn after_first_write(mut self, hook: impl FnMut() + Send + 'static) -> Self {
        self.after_first_write = Some(PtyHook(Box::new(hook)));
        self
    }
}

impl TerminalWriter for FakePty {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut log = self.log.lock().expect("pty log");
        let room = log
            .capacity
            .saturating_sub(log.written.len())
            .min(self.batch)
            .min(bytes.len());
        if room == 0 {
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        log.written.extend_from_slice(&bytes[..room]);
        drop(log);
        if let Some(mut hook) = self.after_first_write.take() {
            (hook.0)();
        }
        Ok(room)
    }

    fn resize(&mut self, size: TerminalSize) -> io::Result<()> {
        let mut log = self.log.lock().expect("pty log");
        log.resize_attempts.push(size);
        if !log.resize_open {
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        log.sizes.push(size);
        Ok(())
    }

    fn retire(&mut self) -> io::Result<()> {
        self.log.lock().expect("pty log").retired += 1;
        Ok(())
    }
}

/// Wait for the writer's own pump to reach `done`, then give up loudly. Delivery
/// and retirement happen on the pump's cadence, never on a connection's.
fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn terminal_session() -> Session {
    Session {
        run: session_run(),
        generation: Generation::new(1),
    }
}

/// The session's own journal row, as the host supervisor holds it before it
/// serves: the kernel's reservation and attachment paths need it.
fn terminal_session_row(cx: &EffectContext, root: &StateRoot) {
    let owner = session_task("session").resource();
    let run = Run::begin(
        cx,
        root,
        &resolved(owner),
        BeginOpts {
            id: session_run(),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next_argv(),
            custody: None,
        },
        &cleanup(cx),
        &mut NoEffects,
    )
    .expect("session run");
    drop(run);
}

fn terminal_grants(capability: Secret) -> host::GrantBook {
    let mut grants = host::GrantBook::new();
    grants.issue(
        capability,
        grant_binding(),
        host::session_operations(),
        [Target::Host].into_iter().collect(),
    );
    grants
}

/// One connection's host server over the kernel's real journal, with the runs it
/// admits named by `admitted`. Each admitted run owns its own task resource, so
/// two runs never contend for one gate.
fn terminal_server<'a>(
    root: &'a StateRoot,
    calls: SharedCalls,
    admitted: &[u8],
    capability: Secret,
) -> host::Server<CountingAdmitter<'a>, CountingResolver, CountingObserver, RunIds> {
    terminal_server_owned(root, calls, admitted, capability, "guest-request")
}

fn terminal_server_owned<'a>(
    root: &'a StateRoot,
    calls: SharedCalls,
    admitted: &[u8],
    capability: Secret,
    owner: &str,
) -> host::Server<CountingAdmitter<'a>, CountingResolver, CountingObserver, RunIds> {
    host::Server::new(
        negotiation(),
        fence(),
        server_identity(),
        terminal_grants(capability),
        host::Dispatcher::new(
            CountingAdmitter {
                calls: calls.clone(),
                root,
                runs: Vec::new(),
            },
            CountingResolver {
                calls: calls.clone(),
                owner: session_task(owner).resource(),
            },
            CountingObserver {
                calls: calls.clone(),
            },
            RunIds(
                admitted
                    .iter()
                    .map(|marker| RunId::new(1, [*marker; 10]).expect("run id"))
                    .collect(),
            ),
        ),
    )
}

/// The same server for a session admitted by the merged #507 fixture: the run
/// it admits is named by `admitted`.
fn serving_server_admitting<'a>(
    calls: SharedCalls,
    root: &'a StateRoot,
    admitted: &[u8],
) -> host::Server<CountingAdmitter<'a>, CountingResolver, CountingObserver, RunIds> {
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
            RunIds(
                admitted
                    .iter()
                    .map(|marker| RunId::new(1, [*marker; 10]).expect("run id"))
                    .collect(),
            ),
        ),
    )
}

fn terminal_hello(capability: Secret) -> Vec<host::Frame> {
    let binary = running_binary();
    vec![
        host::Frame::Hello(host::Hello {
            capabilities: Vec::new(),
            schema: host::SCHEMA.to_owned(),
            protocol_versions: vec![host::PROTOCOL_V1],
            effigy_version: binary.version,
            binary_digest: parse_digest(&binary.digest).expect("digest"),
            context_generation: Generation::new(GENERATION),
            session_run_id: session_run(),
        }),
        host::Frame::Authenticate(capability),
    ]
}

fn terminal_request(id: RequestId, body: RequestBody) -> host::Frame {
    host::Frame::Request(host::Request {
        id,
        context_generation: Generation::new(GENERATION),
        environment_generation: Generation::new(
            ENVIRONMENT_GENERATION
                .parse()
                .expect("environment generation"),
        ),
        session_run_id: session_run(),
        container_id: CONTAINER_ID.to_owned(),
        vm_boot_id: VM_BOOT.to_owned(),
        body,
    })
}

fn terminal_invoke(id: RequestId) -> host::Frame {
    terminal_request(
        id,
        RequestBody::Invoke(InvocationBody {
            selector: Selector::Task("build".to_owned()),
            cwd: "packages/app".to_owned(),
            target: Target::Host,
            args: vec![],
            output: ChannelOutputMode::Text,
        }),
    )
}

fn terminal_attach(id: RequestId, run: RunId, cursor: Option<u64>) -> host::Frame {
    terminal_request(
        id,
        RequestBody::Attach(AttachBody {
            run,
            streams: vec![StreamKind::TerminalOutput],
            cursor,
            cursors: None,
        }),
    )
}

fn typed(id: RequestId, seq: u64, bytes: &[u8]) -> host::Frame {
    host::Frame::Stream(host::StreamFrame {
        id,
        stream_id: host::TERMINAL_STREAM,
        seq,
        kind: StreamKind::TerminalInput,
        payload: Some(host::encode_stream_payload(bytes)),
        rows: None,
        columns: None,
    })
}

fn sized(id: RequestId, seq: u64, rows: u16, columns: u16) -> host::Frame {
    host::Frame::Stream(host::StreamFrame {
        id,
        stream_id: host::TERMINAL_STREAM,
        seq,
        kind: StreamKind::Resize,
        payload: None,
        rows: Some(rows),
        columns: Some(columns),
    })
}

fn size(rows: u16, columns: u16) -> TerminalSize {
    TerminalSize { rows, columns }
}

fn scripted(frames: &[host::Frame]) -> (ScriptedTransport, Arc<Mutex<Vec<u8>>>) {
    let written = Arc::new(Mutex::new(Vec::new()));
    (
        ScriptedTransport {
            input: framed(frames),
            cursor: 0,
            written: written.clone(),
        },
        written,
    )
}

/// Admit the session's run over a connection of its own, so its terminal can be
/// opened before the client that types into it attaches. The kernel's dispatcher
/// remembers the runs its server admitted, so the next connection attaches as
/// the owning client.
fn admit_over_channel<'a>(
    server: &mut host::Server<CountingAdmitter<'a>, CountingResolver, CountingObserver, RunIds>,
    cx: &EffectContext,
    observe: &ObserveContext,
    root: &'a StateRoot,
    capability: Secret,
) -> host::Served {
    let frames = {
        let mut frames = terminal_hello(capability);
        frames.push(terminal_invoke(RequestId::new([1; 16])));
        frames
    };
    let (mut transport, _written) = scripted(&frames);
    server
        .serve(&mut transport, cx, observe, root, &mut host::NoStreams)
        .expect("the admitting connection is served")
}

fn pending_stops(root: &StateRoot, run: RunId) -> usize {
    let database = rusqlite::Connection::open_with_flags(
        root.path().join(effigy_kernel::store::DB_NAME),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("read-only journal");
    let count: i64 = database
        .query_row(
            "SELECT count(*) FROM requests WHERE run_id=?1 AND state='pending'",
            [run.to_string()],
            |row| row.get(0),
        )
        .expect("pending stop requests");
    usize::try_from(count).expect("nonnegative request count")
}

/// The run's own captured segment, at custody's real name and private
/// permissions: the retained reader is the only thing that reads it.
fn captured_stdout(root: &StateRoot, run: RunId, base: u64, bytes: &[u8]) {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let directory = root
        .path()
        .join(Family::Runs.dir_name())
        .join(run.to_string());
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .expect("captured log directory");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(directory.join(format!("1.stdout.{base}.log")))
        .expect("captured segment");
    std::io::Write::write_all(&mut file, bytes).expect("captured bytes");
}

fn stream_frames(frames: &[host::Frame], id: RequestId) -> Vec<host::StreamFrame> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            host::Frame::Stream(stream) if stream.id == id => Some(stream.clone()),
            _ => None,
        })
        .collect()
}

fn written(log: &Arc<Mutex<PtyLog>>) -> Vec<u8> {
    log.lock().expect("pty log").written.clone()
}

fn sizes(log: &Arc<Mutex<PtyLog>>) -> Vec<TerminalSize> {
    log.lock().expect("pty log").sizes.clone()
}

// Test-only writer registration after the same connection's Invoke has
// admitted the run, before its Attach negotiates the stream. The connection
// stays live throughout; no owner attachment is transferred across EOF.
struct OpenAfterAdmission {
    custody: TerminalCustody,
    effect: EffectContext,
    writer: Option<Box<dyn TerminalWriter>>,
}

struct CloseOwnerWrite(UnixStream);
impl Drop for CloseOwnerWrite {
    fn drop(&mut self) {
        let _ = self.0.shutdown(std::net::Shutdown::Write);
    }
}

#[test]
fn owner_connection_guard_closes_the_peer_on_panic() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let (client, mut peer) = connection_pair();
    peer.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("bounded peer read");
    let saw_eof = AtomicBool::new(false);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::thread::scope(|scope| {
            let _close = CloseOwnerWrite(client.try_clone().expect("shutdown guard"));
            scope.spawn(|| {
                use std::io::Read as _;
                let mut byte = [0u8; 1];
                saw_eof.store(
                    peer.read(&mut byte).is_ok_and(|count| count == 0),
                    Ordering::SeqCst,
                );
            });
            panic!("exercise scoped teardown");
        });
    }));
    assert!(outcome.is_err());
    assert!(saw_eof.load(Ordering::SeqCst));
}

#[test]
fn owner_fixture_write_timeout_bounds_an_undrained_peer() {
    let (_client, host_side) = connection_pair();
    let mut transport = PeerTransport::new(host_side);
    transport
        .stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .expect("bounded host write");
    let error = transport
        .write_all(&vec![0; 16 * 1024 * 1024])
        .expect_err("undrained peer fills the socket");
    assert!(matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    ));
}

impl host::Streams for OpenAfterAdmission {
    fn next_chunk(&mut self, attachment: Option<&run::Attachment>) -> Option<host::StreamChunk> {
        self.custody.next_chunk(attachment)
    }

    fn resume(&mut self, run: RunId, cursors: &host::Cursors) -> Option<host::StreamStart> {
        if let Some(writer) = self.writer.take() {
            self.custody
                .open(&self.effect, run, vec![(1, Stream::Stdout)], writer)
                .ok()?;
        }
        self.custody.resume(run, cursors)
    }

    fn start(&mut self, attachment: &run::Attachment, start: &host::StreamStart) {
        self.custody.start(attachment, start);
    }

    fn input(&mut self, attachment: &run::Attachment, bytes: &[u8]) -> Result<(), host::Refusal> {
        self.custody.input(attachment, bytes)
    }

    fn resize(
        &mut self,
        attachment: &run::Attachment,
        size: TerminalSize,
    ) -> Result<(), host::Refusal> {
        self.custody.resize(attachment, size)
    }
}

#[test]
fn terminal_input_and_resize_reach_the_runs_own_writer() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let mut server = terminal_server(&root, calls.clone(), &[9], capability());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    // Step gate: hold delivery until the final request reaches the handle.
    // Latest-wins permits coalescing; intermediate geometries need not land.
    log.lock().expect("pty log").resize_open = false;

    let attach = RequestId::new([2; 16]);
    let frames = {
        let mut frames = terminal_hello(capability());
        frames.push(terminal_invoke(RequestId::new([1; 16])));
        frames.push(terminal_attach(attach, run, None));
        frames.push(typed(attach, 0, b"typed input"));
        frames.push(sized(attach, 1, 24, 80));
        frames.push(sized(attach, 2, 50, 120));
        frames
    };
    let (mut client, host_side) = live_connection(&frames);
    client
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("bounded response read");
    host_side
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("bounded owner read");
    host_side
        .set_write_timeout(Some(Duration::from_secs(30)))
        .expect("bounded owner write");
    // All fixture I/O and wire construction precede the ready barrier. The
    // serving observation budget must not tick while setup is still running.
    let ready = std::sync::Barrier::new(2);
    let (opened, writer_ready) = std::sync::mpsc::sync_channel(0);
    let writer = Box::new(FakePty::new(log.clone()));
    let (served, mut custody, wire) = std::thread::scope(|scope| {
        // This guard is dropped before the scope joins its threads, including
        // when an assertion panics, so the exact peer cannot wait forever.
        let _close = CloseOwnerWrite(client.try_clone().expect("owner shutdown guard"));
        let serving = scope.spawn(|| {
            let mut transport = PeerTransport::new(host_side);
            ready.wait();
            let observe = observation();
            let mut custody = OpenAfterAdmission {
                custody: TerminalCustody::new(terminal_session(), observe.clone(), root.clone()),
                effect: cx.clone(),
                writer: Some(writer),
            };
            opened.send(()).expect("the test awaits its writer");
            drop(opened);
            let served = server
                .serve(&mut transport, &cx, &observe, &root, &mut custody)
                .expect("the owner's connection is served");
            (served, custody.custody)
        });
        ready.wait();
        writer_ready.recv().expect("the writer is ready");
        wait_until(
            "the final requested size to reach the closed handle",
            || log.lock().expect("pty log").resize_attempts.last() == Some(&size(50, 120)),
        );
        assert!(
            sizes(&log).is_empty(),
            "the closed handle delivered nothing"
        );
        log.lock().expect("pty log").resize_open = true;
        wait_until("the writer to take the input and final geometry", || {
            written(&log) == b"typed input" && sizes(&log).last() == Some(&size(50, 120))
        });
        client
            .shutdown(std::net::Shutdown::Write)
            .expect("owner EOF");
        let (served, custody) = serving.join().expect("the serving thread");
        let mut wire = Vec::new();
        std::io::Read::read_to_end(&mut client, &mut wire).expect("owner responses");
        (served, custody, wire)
    });

    assert_eq!(served.responses, 2, "invoke and attach were answered");
    let frames = decoded(&wire);
    assert!(frames.iter().any(|frame| matches!(frame, host::Frame::Response(host::Response { outcome: host::ResponseOutcome::Accepted(accepted), .. }) if accepted.run == run)));
    assert!(frames.iter().any(|frame| match frame {
        host::Frame::Response(host::Response {
            outcome: host::ResponseOutcome::Observed(value),
            ..
        }) => serde_json::from_value::<host::AttachView>(value.clone())
            .is_ok_and(|view| view.run == run && view.client == run::Client::Owner),
        _ => false,
    }));
    assert_eq!(
        calls.lock().expect("calls").admitted,
        vec![run],
        "the run was admitted by the kernel's own reservation path"
    );
    // After joining, the delivery history is stable. Either valid subsequence
    // ends at the final size, with no older geometry delivered after it.
    let delivered = sizes(&log);
    assert!(
        delivered == vec![size(50, 120)] || delivered == vec![size(24, 80), size(50, 120)],
        "resize delivery must be an ordered subsequence ending at the final size: {delivered:?}"
    );
    let terminal = custody.terminal_mut(run).expect("the run's terminal");
    assert!(
        terminal.owner().is_some(),
        "the terminal bound the run's owning attachment at the attach"
    );
    assert_eq!(terminal.delivered(), 11);
    assert_eq!(terminal.pending(), 0, "the writer took every byte");
    assert_eq!(terminal.applied(), Some(size(50, 120)));
}

#[test]
fn retained_output_resumes_at_the_negotiated_cursor() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let _owner = live_admission(&cx, &observation(), &root, calls.clone());
    captured_stdout(&root, run, 0, b"hello");

    let first = RequestId::new([2; 16]);
    let second = RequestId::new([3; 16]);
    // This server retains the owning run. Reader connections are secondary:
    // their EOF releases only their row, so the owner survives the reconnect.
    let mut reader = terminal_server(&root, calls.clone(), &[], capability());
    let first_frames = {
        let mut frames = terminal_hello(capability());
        frames.push(terminal_attach(first, run, None));
        frames
    };
    let second_frames = {
        let mut frames = terminal_hello(capability());
        frames.push(terminal_attach(second, run, Some(3)));
        frames
    };
    // Each serve starts at a fresh script's first byte; a connection that has
    // reached EOF is never presented as the reconnected peer.
    let (mut first_transport, first_wire) = scripted(&first_frames);
    let (mut second_transport, second_wire) = scripted(&second_frames);
    let writer = Box::new(FakePty::new(Arc::new(Mutex::new(PtyLog::default()))));
    let ready = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let serving = scope.spawn(|| {
            ready.wait();
            let observe = observation();
            let mut custody =
                TerminalCustody::new(terminal_session(), observe.clone(), root.clone());
            custody
                .open(&cx, run, vec![(1, Stream::Stdout)], writer)
                .expect("the run's terminal opens");
            reader
                .serve(&mut first_transport, &cx, &observe, &root, &mut custody)
                .expect("the fresh connection is served");
            reader
                .serve(&mut second_transport, &cx, &observe, &root, &mut custody)
                .expect("the reconnected peer is served");
        });
        ready.wait();
        serving.join().expect("the serving thread");
    });

    assert_eq!(
        pending_stops(&root, run),
        0,
        "reader EOF never stopped the owner"
    );
    let fresh = stream_frames(&decoded(&first_wire.lock().expect("first wire")), first);
    let resumed = stream_frames(&decoded(&second_wire.lock().expect("second wire")), second);
    assert_eq!(fresh.len(), 1);
    assert_eq!(fresh[0].seq, 0, "output starts at the log's own position");
    assert_eq!(
        host::decode_stream_payload(fresh[0].payload.as_deref().expect("payload")).expect("base64"),
        b"hello"
    );
    assert_eq!(resumed.len(), 1);
    assert_eq!(
        resumed[0].seq, 3,
        "a reconnect resumes at the negotiated cursor, never at zero"
    );
    assert_eq!(
        host::decode_stream_payload(resumed[0].payload.as_deref().expect("payload"))
            .expect("base64"),
        b"lo",
        "the client sees exactly what it has not read"
    );
    _owner.assert_live();
}

#[test]
fn a_rotated_prefix_is_reported_as_a_bounded_gap_not_as_complete_output() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let mut server = terminal_server(&root, calls.clone(), &[9], capability());
    let _owner = live_admission(&cx, &observe, &root, calls.clone());
    // The client consumed 40 bytes, and the log has rotated to 100: the range it
    // never saw is gone and must be reported, not skipped silently.
    captured_stdout(&root, run, 100, b"current");

    let attach = RequestId::new([2; 16]);
    let frames = {
        let mut frames = terminal_hello(capability());
        frames.push(terminal_attach(attach, run, Some(40)));
        frames
    };
    let (mut client, host_side) = live_connection(&frames);
    client
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("the peer's bounded reads");
    // Setup is complete: admission, retained bytes and all wire frames exist.
    // Start the publication budget at this step gate, not before fixture I/O.
    host_side
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("bounded reader read");
    host_side
        .set_write_timeout(Some(Duration::from_secs(30)))
        .expect("bounded reader write");
    let observe = observation();
    let mut custody = TerminalCustody::new(terminal_session(), observe.clone(), root.clone());
    custody
        .open(
            &cx,
            run,
            vec![(1, Stream::Stdout)],
            Box::new(FakePty::new(Arc::new(Mutex::new(PtyLog::default())))),
        )
        .expect("the run's terminal opens");
    let output = std::thread::scope(|scope| {
        let _close = CloseOwnerWrite(client.try_clone().expect("owner shutdown guard"));
        let serving = scope.spawn(|| {
            let mut transport = PeerTransport::new(host_side);
            server.serve(&mut transport, &cx, &observe, &root, &mut custody)
        });
        // Receiving both frames is the EOF gate. The peer cannot disappear
        // before the retained reader has published the gap and its data.
        let mut output = Vec::new();
        while output.len() < 2 {
            use std::io::Read as _;
            let mut prefix = [0; 4];
            client.read_exact(&mut prefix).expect("a frame prefix");
            let length = u32::from_be_bytes(prefix) as usize;
            assert!(length <= host::MAX_FRAME_BYTES, "a bounded wire frame");
            let mut body = vec![0; length];
            client.read_exact(&mut body).expect("a complete frame");
            match host::decode_frame(&body).expect("a valid frame") {
                host::Frame::Stream(stream) if stream.id == attach => output.push(stream),
                host::Frame::Refuse(refusal) => panic!("publication refused: {refusal:?}"),
                _ => {}
            }
        }
        client
            .shutdown(std::net::Shutdown::Write)
            .expect("EOF after the gap and retained data");
        let mut tail = Vec::new();
        std::io::Read::read_to_end(&mut client, &mut tail).expect("the remaining wire frames");
        output.extend(stream_frames(&decoded(&tail), attach));
        drop(client);
        serving
            .join()
            .expect("the serving thread")
            .expect("the rotated read serves its gap");
        output
    });
    assert_eq!(output.len(), 2, "exactly a gap and its retained data");
    assert_eq!(
        output[0].gap_range(),
        Some(host::StreamGap { from: 40, to: 100 }),
        "the missing range is named by its own positions"
    );
    assert_eq!(output[1].seq, 100);
    assert_eq!(
        host::decode_stream_payload(output[1].payload.as_deref().expect("payload"))
            .expect("base64"),
        b"current"
    );
    _owner.assert_live();
}

#[test]
fn input_is_bounded_and_refuses_past_its_bound() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let mut server = terminal_server(&root, calls.clone(), &[9], capability());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    // A writer that takes four bytes and then reports a full buffer, as a child
    // that is not reading its terminal does.
    let mut custody = OpenAfterAdmission {
        custody: TerminalCustody::new(terminal_session(), observe.clone(), root.clone()),
        effect: cx.clone(),
        writer: Some(Box::new(FakePty::new(log.clone()).bounded(4, 4))),
    };

    let attach = RequestId::new([2; 16]);
    let burst = vec![7u8; host::MAX_STREAM_PAYLOAD];
    let frames = {
        let mut frames = terminal_hello(capability());
        frames.push(terminal_invoke(RequestId::new([1; 16])));
        frames.push(terminal_attach(attach, run, None));
        frames.push(typed(attach, 0, &burst));
        frames.push(typed(attach, 1, &burst));
        // The owner already holds two frames' worth: this one must refuse.
        frames.push(typed(attach, 2, b"overflow"));
        frames
    };
    let (mut transport, wire) = scripted(&frames);
    let result = server.serve(&mut transport, &cx, &observe, &root, &mut custody);
    wait_until("the writer to take the four bytes it accepts", || {
        written(&log).len() == 4
    });
    let terminal = custody
        .custody
        .terminal_mut(run)
        .expect("the run's terminal");
    let pending = terminal.pending();
    let refused = terminal.refused();

    assert!(
        matches!(
            result,
            Err(host::Refusal {
                code: host::RefusalCode::Unavailable,
                ..
            })
        ),
        "input past the bound is a refusal, not an unbounded queue: {result:?}"
    );
    assert_eq!(
        pending,
        super::terminal::MAX_PENDING_INPUT - 4,
        "the owner holds exactly its bound, never more"
    );
    assert_eq!(refused, 1, "the overflow was refused once, with no effect");
    let frames = decoded(&wire.lock().expect("written"));
    assert!(
        frames.iter().any(|frame| matches!(
            frame,
            host::Frame::Refuse(host::Refusal {
                code: host::RefusalCode::Unavailable,
                ..
            })
        )),
        "the client was told why its input did not arrive: {frames:?}"
    );
}

#[test]
fn backpressured_input_drains_while_the_connection_is_idle() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let mut server = terminal_server(&root, calls.clone(), &[9], capability());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    // Four bytes in, then a full terminal buffer.
    let mut custody = OpenAfterAdmission {
        custody: TerminalCustody::new(terminal_session(), observe.clone(), root.clone()),
        effect: cx.clone(),
        writer: Some(Box::new(FakePty::new(log.clone()).bounded(4, 4))),
    };

    let attach = RequestId::new([2; 16]);
    let frames = {
        let mut frames = terminal_hello(capability());
        frames.push(terminal_invoke(RequestId::new([1; 16])));
        frames.push(terminal_attach(attach, run, None));
        frames.push(typed(attach, 0, b"typed input"));
        frames
    };
    // A live connection whose frames are all already written: the host consumes
    // them and then blocks on a quiet socket, which is what "the connection is
    // idle" means. The client end stays open until the proof is done.
    let (mut client, host_side) = connection_pair();
    client
        .set_write_timeout(Some(Duration::from_secs(30)))
        .expect("bounded fixture write");
    {
        use std::io::Write as _;
        client
            .write_all(&framed(&frames))
            .expect("the connection's frames");
        client.flush().expect("the connection's frames");
    }
    host_side
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("bounded owner read");
    host_side
        .set_write_timeout(Some(Duration::from_secs(30)))
        .expect("bounded owner write");
    std::thread::scope(|scope| {
        let _close = CloseOwnerWrite(client.try_clone().expect("owner shutdown guard"));
        let serving = scope.spawn(|| {
            let mut transport = PeerTransport::new(host_side);
            server.serve(&mut transport, &cx, &observe, &root, &mut custody)
        });
        // The writer takes what it can and then reports a full buffer.
        wait_until("the writer to take what it accepts", || {
            written(&log) == b"type"
        });
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(
            written(&log),
            b"type",
            "the writer is full: no further byte can be written yet"
        );
        // The child starts reading again. No frame, pump call or connection
        // traffic is needed: the writer's own pump drains what it queued.
        log.lock().expect("pty log").capacity = usize::MAX;
        wait_until("the queued input to drain while idle", || {
            written(&log) == b"typed input"
        });
        // Only now does the client go away.
        client
            .shutdown(std::net::Shutdown::Write)
            .expect("owner EOF");
        drop(client);
        serving
            .join()
            .expect("the serving thread")
            .expect("the idle connection is served");
    });
}

#[test]
fn a_burst_of_sizes_keeps_only_the_newest_geometry() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let mut server = terminal_server(&root, calls.clone(), &[9], capability());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    // The resize handle is closed for the whole burst, as a terminal whose
    // resize path is momentarily unavailable is. Attempt counts cannot change
    // the result: while the handle is closed the owner holds at most one size,
    // and the newest one is the only size that can ever be applied.
    log.lock().expect("pty log").resize_open = false;
    let mut custody = OpenAfterAdmission {
        custody: TerminalCustody::new(terminal_session(), observe.clone(), root.clone()),
        effect: cx.clone(),
        writer: Some(Box::new(FakePty::new(log.clone()))),
    };

    let attach = RequestId::new([2; 16]);
    let frames = {
        let mut frames = terminal_hello(capability());
        frames.push(terminal_invoke(RequestId::new([1; 16])));
        frames.push(terminal_attach(attach, run, None));
        frames.push(sized(attach, 0, 24, 80));
        frames.push(sized(attach, 1, 50, 120));
        frames.push(sized(attach, 2, 60, 140));
        frames
    };
    // The connection stays live while the proof runs, so the writer's own pump
    // is alive too: the burst, the closed handle and the reopened handle are one
    // terminal's whole life, not a race against the owner's end of stream.
    let (client, host_side) = live_connection(&frames);
    host_side
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("bounded owner read");
    host_side
        .set_write_timeout(Some(Duration::from_secs(30)))
        .expect("bounded owner write");
    std::thread::scope(|scope| {
        let _close = CloseOwnerWrite(client.try_clone().expect("owner shutdown guard"));
        let serving = scope.spawn(|| {
            let mut transport = PeerTransport::new(host_side);
            server.serve(&mut transport, &cx, &observe, &root, &mut custody)
        });
        // The newest size being attempted proves the third frame was processed
        // and that the owner holds exactly that size; the closed handle applied
        // nothing, however many turns the pump took.
        wait_until("the newest size to be attempted", || {
            log.lock().expect("pty log").resize_attempts.last() == Some(&size(60, 140))
        });
        assert!(
            sizes(&log).is_empty(),
            "a closed resize handle applied a size: {:?}",
            sizes(&log)
        );
        // Opening the handle applies the newest size, and only the newest: the
        // two earlier sizes were superseded while it was closed.
        log.lock().expect("pty log").resize_open = true;
        wait_until("the newest geometry to be applied", || {
            sizes(&log).last() == Some(&size(60, 140))
        });
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(
            sizes(&log),
            vec![size(60, 140)],
            "exactly the newest size reached the writer"
        );
        client
            .shutdown(std::net::Shutdown::Write)
            .expect("owner EOF");
        drop(client);
        serving
            .join()
            .expect("the serving thread")
            .expect("the burst connection is served");
    });
}

#[test]
fn a_second_terminal_never_types_into_the_owners_writer() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let _owner = live_admission(&cx, &observe, &root, calls.clone());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    let mut custody = TerminalCustody::new(terminal_session(), observe.clone(), root.clone());
    custody
        .open(
            &cx,
            run,
            vec![(1, Stream::Stdout)],
            Box::new(FakePty::new(log.clone())),
        )
        .expect("the run's terminal opens");

    let owner = run::resume_owner(&cx, &root, terminal_session(), run).expect("owner attachment");
    custody.start(&owner, &StreamStart::from([(host::TERMINAL_STREAM, 0)]));
    custody.input(&owner, b"first").expect("the owner types");
    wait_until("the owner's first bytes", || written(&log) == b"first");

    let mut secondary = run::attach_secondary(
        &cx,
        &root,
        terminal_session(),
        run,
        AttachmentId::new([55; 16]),
    )
    .expect("a second terminal attaches");
    assert_eq!(secondary.client(), Client::Secondary);

    // Reversible failing safety assertion: the terminal refuses a frame from an
    // attachment it does not own, with zero bytes and zero resize, and accepts
    // the very same frame from the owning attachment.
    let refusal = custody
        .input(&secondary, b"forged")
        .expect_err("a second terminal never types into the owner's writer");
    assert_eq!(refusal.code, host::RefusalCode::Forbidden);
    custody
        .resize(&secondary, size(10, 10))
        .expect_err("a second terminal never resizes the owner's writer");
    assert_eq!(written(&log), b"first", "a refused frame wrote nothing");
    assert!(sizes(&log).is_empty(), "a refused frame resized nothing");
    custody
        .input(&owner, b" second")
        .expect("the owning attachment still types");
    wait_until("the owner's second frame", || {
        written(&log) == b"first second"
    });

    // A secondary detach ends that terminal only: the run is still live and the
    // owner's writer still takes input.
    secondary
        .release(&cx, &root, Release::Detach)
        .expect("the second terminal detaches");
    assert_eq!(
        pending_stops(&root, run),
        0,
        "a secondary detach never stops the owning run"
    );
    custody
        .input(&owner, b" third")
        .expect("the owner's terminal survives the secondary's detach");
    wait_until("the owner's third frame", || {
        written(&log) == b"first second third"
    });
    _owner.assert_live();
}

#[test]
fn an_attachment_that_did_not_bind_the_writer_never_types_or_resizes() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let _owner = live_admission(&cx, &observe, &root, calls.clone());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    let mut custody = TerminalCustody::new(terminal_session(), observe.clone(), root.clone());
    custody
        .open(
            &cx,
            run,
            vec![(1, Stream::Stdout)],
            Box::new(FakePty::new(log.clone())),
        )
        .expect("the run's terminal opens");

    let owner = run::resume_owner(&cx, &root, terminal_session(), run).expect("owner attachment");
    custody.start(&owner, &StreamStart::from([(host::TERMINAL_STREAM, 0)]));
    custody.input(&owner, b"live").expect("the owner types");
    wait_until("the owner's first bytes", || written(&log) == b"live");

    // A terminal that attached and detached, and a second live terminal of the
    // same run: neither is the attachment that bound this writer, so both
    // refuse. (The bound row going stale, and the kernel replacing a bound
    // attachment, are proven by their own tests below.)
    let mut stale = run::attach_secondary(
        &cx,
        &root,
        terminal_session(),
        run,
        AttachmentId::new([55; 16]),
    )
    .expect("a second terminal attaches");
    stale
        .release(&cx, &root, Release::Detach)
        .expect("it detaches");
    let refusal = custody
        .input(&stale, b"stale")
        .expect_err("a stale attachment never types");
    assert_eq!(refusal.code, host::RefusalCode::Forbidden);
    custody
        .resize(&stale, size(30, 30))
        .expect_err("a stale attachment never resizes");

    // A different live attachment of the same run: the writer's binding is not
    // replaced under it, so the second terminal is refused too.
    let replaced = run::attach_secondary(
        &cx,
        &root,
        terminal_session(),
        run,
        AttachmentId::new([56; 16]),
    )
    .expect("a replacement attachment attaches");
    let refusal = custody
        .input(&replaced, b"replaced")
        .expect_err("a replaced attachment never types");
    assert_eq!(refusal.code, host::RefusalCode::Forbidden);
    custody
        .resize(&replaced, size(40, 40))
        .expect_err("a replaced attachment never resizes");
    assert_eq!(written(&log), b"live", "no refused frame wrote anything");
    assert!(sizes(&log).is_empty(), "no refused frame resized anything");

    // Reversible: the same bytes through the owning attachment are accepted.
    custody
        .input(&owner, b" and live")
        .expect("the owning attachment types");
    wait_until("the owner's next frame", || {
        written(&log) == b"live and live"
    });
    _owner.assert_live();
}

#[test]
fn a_replaced_attachment_is_refused_and_the_bound_one_still_types() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let _owner = live_admission(&cx, &observe, &root, calls.clone());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    let mut custody = TerminalCustody::new(terminal_session(), observe.clone(), root.clone());
    custody
        .open(
            &cx,
            run,
            vec![(1, Stream::Stdout)],
            Box::new(FakePty::new(log.clone())),
        )
        .expect("the run's terminal opens");
    // The writer is bound to the run's owning attachment, which stays attached
    // for the whole proof.
    let owning = run::resume_owner(&cx, &root, terminal_session(), run).expect("owning attachment");
    custody.start(&owning, &StreamStart::from([(host::TERMINAL_STREAM, 0)]));

    // A second terminal attaches on its own client and the kernel replaces it
    // with a new request id: that is the kernel's own replacement path.
    let mut other_server = terminal_server(&root, calls.clone(), &[9], capability());
    let replacement = RequestId::new([4; 16]);
    let frames = {
        let mut frames = terminal_hello(capability());
        frames.push(terminal_attach(RequestId::new([3; 16]), run, None));
        frames.push(terminal_attach(replacement, run, None));
        frames.push(typed(replacement, 0, b"from the replacement"));
        frames
    };
    let (mut transport, wire) = scripted(&frames);
    let result = other_server.serve(&mut transport, &cx, &observe, &root, &mut custody);
    assert!(
        matches!(
            result,
            Err(host::Refusal {
                code: host::RefusalCode::Forbidden,
                ..
            })
        ),
        "a replaced second terminal never types into the owner's writer: {result:?}"
    );
    assert!(
        written(&log).is_empty(),
        "the replacement's bytes never reached the writer"
    );
    assert!(sizes(&log).is_empty(), "the replacement resized nothing");

    // Reversible failing safety assertion: the very same frame through the
    // owning attachment is accepted, and the writer is still bound to it.
    custody
        .input(&owning, b"the owning client types")
        .expect("the original owning attachment types");
    wait_until("the owning client's frame", || {
        written(&log) == b"the owning client types"
    });
    assert_eq!(
        custody
            .terminal_mut(run)
            .expect("the run's terminal")
            .owner(),
        Some(owning.id()),
        "the writer stays bound to the run's owning attachment"
    );
    let wire = decoded(&wire.lock().expect("written"));
    assert!(
        wire.iter().any(|frame| matches!(
            frame,
            host::Frame::Refuse(host::Refusal {
                code: host::RefusalCode::Forbidden,
                ..
            })
        )),
        "the replaced terminal was told why its frame did not arrive: {wire:?}"
    );
    _owner.assert_live();
}

/// Either a fixed script or a live socket, so one merged-loop owner can admit a
/// run deterministically and then serve a connection that has to stay live while
/// the writer's own pump is observed.
enum TestTransport {
    Scripted(ScriptedTransport),
    Peer(PeerTransport),
}

impl Transport for TestTransport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Scripted(transport) => transport.read(buf),
            Self::Peer(transport) => transport.read(buf),
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        match self {
            Self::Scripted(transport) => transport.write_all(bytes),
            Self::Peer(transport) => transport.write_all(bytes),
        }
    }

    fn verified_peer(&self) -> Option<host::ServerIdentity> {
        match self {
            Self::Scripted(transport) => transport.verified_peer(),
            Self::Peer(transport) => transport.verified_peer(),
        }
    }
}

/// An acceptor over those two kinds of connection.
struct PeerAccept {
    connections: VecDeque<TestTransport>,
}

impl ChannelAccept for PeerAccept {
    type Transport = TestTransport;
    fn accept(
        &mut self,
        launch: &SessionLaunch,
        run: &Run<'_>,
    ) -> io::Result<Option<TestTransport>> {
        assert!(
            launch.serving(run),
            "a connection was accepted before the session run served the channel"
        );
        Ok(self.connections.pop_front())
    }
}

/// Write one connection's frames onto its own socket and keep it open: the host
/// consumes them and then blocks on a quiet connection, so the writer's own
/// effects happen while the connection is still live.
fn live_connection(frames: &[host::Frame]) -> (UnixStream, UnixStream) {
    use std::io::Write as _;
    let (mut client, host_side) = connection_pair();
    client
        .set_write_timeout(Some(Duration::from_secs(30)))
        .expect("bounded fixture write");
    client
        .write_all(&framed(frames))
        .expect("the connection's frames");
    client.flush().expect("the connection's frames");
    (client, host_side)
}

// Keep an actual admitting connection alive while the direct terminal tests
// exercise kernel attachments. This guard does not claim journal settlement.
struct LiveAdmission {
    client: UnixStream,
    server: Option<std::thread::JoinHandle<()>>,
}

impl LiveAdmission {
    fn assert_live(&self) {
        assert!(
            self.server
                .as_ref()
                .is_some_and(|server| !server.is_finished()),
            "admitting connection must remain live during the proof"
        );
    }
}

impl Drop for LiveAdmission {
    fn drop(&mut self) {
        let _ = self.client.shutdown(std::net::Shutdown::Both);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

fn live_admission(
    cx: &EffectContext,
    observe: &ObserveContext,
    root: &StateRoot,
    calls: SharedCalls,
) -> LiveAdmission {
    live_admission_named(
        cx,
        observe,
        root,
        calls,
        9,
        1,
        capability(),
        "guest-request",
    )
}

#[allow(clippy::too_many_arguments)]
fn live_admission_named(
    cx: &EffectContext,
    observe: &ObserveContext,
    root: &StateRoot,
    calls: SharedCalls,
    marker: u8,
    request: u8,
    credential: Secret,
    task_name: &str,
) -> LiveAdmission {
    let mut frames = terminal_hello(credential.clone());
    frames.push(terminal_invoke(RequestId::new([request; 16])));
    let (client, host_side) = live_connection(&frames);
    client
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("bounded admission response");
    host_side
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("bounded admission read");
    host_side
        .set_write_timeout(Some(Duration::from_secs(30)))
        .expect("bounded admission write");
    let cx = cx.clone();
    let observe = observe.clone();
    let root = root.clone();
    let task_name = task_name.to_owned();
    let server = std::thread::spawn(move || {
        let mut server = terminal_server_owned(&root, calls, &[marker], credential, &task_name);
        let mut transport = PeerTransport::new(host_side);
        let _ = server.serve(&mut transport, &cx, &observe, &root, &mut host::NoStreams);
    });
    let mut owner = LiveAdmission {
        client,
        server: Some(server),
    };
    let mut responses = Vec::new();
    for _ in 0..2 {
        use std::io::Read as _;
        let mut prefix = [0; 4];
        owner
            .client
            .read_exact(&mut prefix)
            .expect("admission frame length");
        let length = u32::from_be_bytes(prefix) as usize;
        assert!(length <= host::MAX_FRAME_BYTES);
        let mut body = vec![0; length];
        owner.client.read_exact(&mut body).expect("admission frame");
        responses.push(host::decode_frame(&body).expect("admission frame decodes"));
    }
    let expected = RunId::new(1, [marker; 10]).expect("run id");
    assert!(responses.iter().any(|frame| matches!(frame, host::Frame::Response(host::Response { outcome: host::ResponseOutcome::Accepted(accepted), .. }) if accepted.run == expected)));
    owner
}

/// The owning attachment, released from inside a writer effect, so a proof can
/// release exactly the row that queued the work.
fn releasing_writer(
    log: &Arc<Mutex<PtyLog>>,
    cx: &EffectContext,
    root: &StateRoot,
    batch: usize,
    capacity: usize,
) -> FakePty {
    let releasing_context = cx.clone();
    let releasing_root = root.clone();
    FakePty::new(log.clone())
        .bounded(batch, capacity)
        .after_first_write(move || {
            let mut releaser = run::resume_owner(
                &releasing_context,
                &releasing_root,
                terminal_session(),
                RunId::new(1, [9; 10]).expect("run id"),
            )
            .expect("owning attachment");
            releaser
                .release(&releasing_context, &releasing_root, Release::Eof)
                .expect("the owning client's end of stream");
        })
}

#[test]
fn stale_input_and_resize_are_refused_through_the_channel_loop() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let mut server = terminal_server(&root, calls.clone(), &[9], capability());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    // The fake writer is registered after this same connection admits the run.
    let mut custody = OpenAfterAdmission {
        custody: TerminalCustody::new(terminal_session(), observe.clone(), root.clone()),
        effect: cx.clone(),
        writer: Some(Box::new(releasing_writer(&log, &cx, &root, 4, 4))),
    };

    let attach = RequestId::new([2; 16]);
    let frames = {
        let mut frames = terminal_hello(capability());
        frames.push(terminal_invoke(RequestId::new([1; 16])));
        frames.push(terminal_attach(attach, run, None));
        frames.push(sized(attach, 0, 24, 80));
        frames
    };
    let (mut client, host_side) = live_connection(&frames);
    client
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("bounded response read");
    host_side
        .set_write_timeout(Some(Duration::from_secs(30)))
        .expect("bounded owner write");
    std::thread::scope(|scope| {
        let _close = CloseOwnerWrite(client.try_clone().expect("owner shutdown guard"));
        let serving = scope.spawn(|| {
            let mut transport = PeerTransport::new(host_side);
            server.serve(&mut transport, &cx, &observe, &root, &mut custody)
        });
        wait_until("valid resize to reach the writer", || {
            sizes(&log) == vec![size(24, 80)]
        });
        use std::io::Write as _;
        client
            .write_all(&framed(&[typed(attach, 1, b"abcdefgh")]))
            .expect("send owner input");
        wait_until("the first chunk to land", || written(&log) == b"abcd");
        wait_until("the released row to retire the writer", || {
            log.lock().expect("pty log").retired == 1
        });
        client
            .write_all(&framed(&[sized(attach, 2, 50, 120)]))
            .expect("send resize after release");
        let result = serving.join().expect("the serving thread");
        let mut wire = Vec::new();
        std::io::Read::read_to_end(&mut client, &mut wire).expect("read stale refusal");
        assert!(matches!(
            result,
            Err(host::Refusal {
                code: host::RefusalCode::InvalidRequest,
                ..
            }) | Err(host::Refusal {
                code: host::RefusalCode::Unknown,
                ..
            })
        ));
        assert!(decoded(&wire).iter().any(|frame| matches!(
            frame,
            host::Frame::Refuse(host::Refusal {
                code: host::RefusalCode::InvalidRequest,
                ..
            })
        )));
        assert_eq!(
            written(&log),
            b"abcd",
            "the work queued on the released row never landed"
        );
        assert!(
            sizes(&log) == vec![size(24, 80)],
            "only the valid resize was applied: {:?}",
            sizes(&log)
        );
        let _ = client.shutdown(std::net::Shutdown::Write);
        drop(client);
    });
}

#[test]
fn the_serving_owner_refuses_stale_input_and_resize() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let run = RunId::new(1, [9; 10]).expect("run id");
        let typing = {
            let mut frames = terminal_hello(capability());
            frames.push(terminal_invoke(RequestId::new([1; 16])));
            let attach = RequestId::new([2; 16]);
            frames.push(terminal_attach(attach, run, None));
            frames.push(sized(attach, 0, 24, 80));
            frames
        };
        // Only the typing connection has to stay live: its frames are consumed
        // and the host then blocks on the quiet socket while the writer's pump
        // finishes.
        let (mut typing_client, typing_host) = live_connection(&typing);
        typing_client
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("bounded response read");
        let acceptor = PeerAccept {
            connections: VecDeque::from(vec![TestTransport::Peer(PeerTransport::new(typing_host))]),
        };

        let (listener, key, _acceptor) = SessionTlsListener::bind(
            SocketAddr::from(([127u8, 0, 0, 1], 0)),
            Duration::from_secs(1),
        )
        .expect("bound session listener");
        let mut launch = SessionLaunch::launch(
            &mut supervisor,
            &session,
            "485-supervisor",
            key,
            Box::new(listener),
        )
        .expect("session listener custody");
        launch
            .qualify(&mut VerifiedProbe, Duration::from_secs(1))
            .expect("qualified");
        launch.publish(&supervisor).expect("published");
        let mut owner = ServingOwner::new(
            supervisor,
            session.clone(),
            launch,
            acceptor,
            serving_server_admitting(calls.clone(), root, &[9]),
            Arc::new(cx.clone()),
            Arc::new(observe.clone()),
            root,
        );
        let log = Arc::new(Mutex::new(PtyLog::default()));
        let mut terminals = OpenAfterAdmission {
            custody: owner.terminal_custody(observe),
            effect: cx.clone(),
            writer: Some(Box::new(releasing_writer(&log, cx, root, 4, 4))),
        };

        std::thread::scope(|scope| {
            let _close = CloseOwnerWrite(typing_client.try_clone().expect("owner shutdown guard"));
            let serving = scope.spawn(|| owner.serve_pending(&mut terminals));
            wait_until("valid resize to reach the writer", || {
                sizes(&log) == vec![size(24, 80)]
            });
            use std::io::Write as _;
            typing_client
                .write_all(&framed(&[typed(RequestId::new([2; 16]), 1, b"abcdefgh")]))
                .expect("send owner input");
            wait_until("the first chunk to land", || written(&log) == b"abcd");
            wait_until("the released row to retire the writer", || {
                log.lock().expect("pty log").retired == 1
            });
            typing_client
                .write_all(&framed(&[sized(RequestId::new([2; 16]), 2, 50, 120)]))
                .expect("send resize after release");
            let result = serving.join().expect("the serving thread");
            let mut wire = Vec::new();
            std::io::Read::read_to_end(&mut typing_client, &mut wire).expect("read stale refusal");
            assert!(result.is_err());
            assert!(decoded(&wire).iter().any(|frame| matches!(
                frame,
                host::Frame::Refuse(host::Refusal {
                    code: host::RefusalCode::InvalidRequest,
                    ..
                })
            )));
            assert_eq!(
                written(&log),
                b"abcd",
                "the work queued on the released row never landed"
            );
            assert!(
                sizes(&log) == vec![size(24, 80)],
                "only the valid resize was applied: {:?}",
                sizes(&log)
            );
            let _ = typing_client.shutdown(std::net::Shutdown::Write);
            drop(typing_client);
        });
    });
}

#[test]
fn replacement_input_and_resize_are_refused_through_the_channel_loop() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let _owner = live_admission(&cx, &observe, &root, calls.clone());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    let mut custody = TerminalCustody::new(terminal_session(), observe.clone(), root.clone());
    custody
        .open(
            &cx,
            run,
            vec![(1, Stream::Stdout)],
            Box::new(FakePty::new(log.clone())),
        )
        .expect("the run's terminal opens");
    let owning = run::resume_owner(&cx, &root, terminal_session(), run).expect("owning attachment");
    custody.start(&owning, &StreamStart::from([(host::TERMINAL_STREAM, 0)]));

    // Each attempt is its own client: the kernel replaces the first attachment
    // with the request id it is told to use, and the frame it then sends names
    // that replacement — an attachment the writer is not bound to.
    for (index, frame) in ["input", "resize"].into_iter().enumerate() {
        let first = RequestId::new([10 + index as u8; 16]);
        let replacement = RequestId::new([20 + index as u8; 16]);
        let refused = match frame {
            "input" => typed(replacement, 0, b"from the replacement"),
            _ => sized(replacement, 0, 50, 120),
        };
        let frames = {
            let mut frames = terminal_hello(capability());
            frames.push(terminal_attach(first, run, None));
            frames.push(terminal_attach(replacement, run, None));
            frames.push(refused);
            frames
        };
        let mut other_server = terminal_server(&root, calls.clone(), &[9], capability());
        let (mut transport, wire) = scripted(&frames);
        let result = other_server.serve(&mut transport, &cx, &observe, &root, &mut custody);
        assert!(
            matches!(
                result,
                Err(host::Refusal {
                    code: host::RefusalCode::Forbidden,
                    ..
                })
            ),
            "a replacement terminal's {frame} is refused: {result:?}"
        );
        assert!(
            written(&log).is_empty(),
            "a replacement {frame} wrote nothing"
        );
        assert!(
            sizes(&log).is_empty(),
            "a replacement {frame} resized nothing"
        );
        let wire = decoded(&wire.lock().expect("written"));
        assert!(
            wire.iter().any(|frame| matches!(
                frame,
                host::Frame::Refuse(host::Refusal {
                    code: host::RefusalCode::Forbidden,
                    ..
                })
            )),
            "the client was told why the replacement {frame} was refused: {wire:?}"
        );
    }
    assert_eq!(
        custody
            .terminal_mut(run)
            .expect("the run's terminal")
            .owner(),
        Some(owning.id()),
        "the writer stays bound to the run's owning attachment"
    );
    _owner.assert_live();
}

#[test]
fn the_serving_owner_refuses_replacement_input_and_resize() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let run = RunId::new(1, [9; 10]).expect("run id");
        // The run is admitted through a channel server of its own, so the serving
        // owner's dispatcher has not admitted it: its attach opens a second
        // terminal, which is the attachment the kernel's replacement path can
        // replace.
        let _owner = live_admission(cx, observe, root, calls.clone());

        let mut acceptor_connections = VecDeque::new();
        let mut replacements = Vec::new();
        for (index, frame) in ["input", "resize"].into_iter().enumerate() {
            let first = RequestId::new([10 + index as u8; 16]);
            let replacement = RequestId::new([20 + index as u8; 16]);
            let refused = match frame {
                "input" => typed(replacement, 0, b"from the replacement"),
                _ => sized(replacement, 0, 50, 120),
            };
            let frames = {
                let mut frames = terminal_hello(capability());
                frames.push(terminal_attach(first, run, None));
                frames.push(terminal_attach(replacement, run, None));
                frames.push(refused);
                frames
            };
            let (transport, wire) = scripted(&frames);
            acceptor_connections.push_back(TestTransport::Scripted(transport));
            replacements.push((frame, wire));
        }
        let acceptor = PeerAccept {
            connections: acceptor_connections,
        };

        let (listener, key, _acceptor) = SessionTlsListener::bind(
            SocketAddr::from(([127u8, 0, 0, 1], 0)),
            Duration::from_secs(1),
        )
        .expect("bound session listener");
        let mut launch = SessionLaunch::launch(
            &mut supervisor,
            &session,
            "485-supervisor",
            key,
            Box::new(listener),
        )
        .expect("session listener custody");
        launch
            .qualify(&mut VerifiedProbe, Duration::from_secs(1))
            .expect("qualified");
        launch.publish(&supervisor).expect("published");
        let mut owner = ServingOwner::new(
            supervisor,
            session.clone(),
            launch,
            acceptor,
            serving_server_for(calls.clone(), root),
            Arc::new(cx.clone()),
            Arc::new(observe.clone()),
            root,
        );
        let mut terminals = owner.terminal_custody(observe);
        let log = Arc::new(Mutex::new(PtyLog::default()));
        terminals
            .open(
                cx,
                run,
                vec![(1, Stream::Stdout)],
                Box::new(FakePty::new(log.clone())),
            )
            .expect("the session's terminal opens");
        let owning =
            run::resume_owner(cx, root, terminal_session(), run).expect("owning attachment");
        terminals.start(&owning, &StreamStart::from([(host::TERMINAL_STREAM, 0)]));

        for (frame, wire) in replacements {
            _owner.assert_live();
            // A refused connection ends that connection only; the session keeps
            // serving.
            let served = owner.serve_pending(&mut terminals);
            assert!(
                served.is_err(),
                "a replacement {frame} closes its own connection"
            );
            assert!(
                written(&log).is_empty(),
                "a replacement {frame} wrote nothing"
            );
            assert!(
                sizes(&log).is_empty(),
                "a replacement {frame} resized nothing"
            );
            let wire = decoded(&wire.lock().expect("written"));
            assert!(
                wire.iter().any(|frame| matches!(
                    frame,
                    host::Frame::Refuse(host::Refusal {
                        code: host::RefusalCode::Forbidden,
                        ..
                    })
                )),
                "the client was told why the replacement {frame} was refused: {wire:?}"
            );
            _owner.assert_live();
        }
        assert_eq!(
            terminals
                .terminal_mut(run)
                .expect("the run's terminal")
                .owner(),
            Some(owning.id())
        );
    });
}

#[test]
fn two_independent_terminals_and_capabilities_never_cross() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let first_run = RunId::new(1, [9; 10]).expect("run id");
    let second_run = RunId::new(1, [10; 10]).expect("run id");
    let first_capability = capability();
    let second_capability = Secret::new([0xcd; 32]);
    let _first_owner = live_admission_named(
        &cx,
        &observe,
        &root,
        calls.clone(),
        9,
        11,
        first_capability.clone(),
        "guest-request-one",
    );
    let _second_owner = live_admission_named(
        &cx,
        &observe,
        &root,
        calls.clone(),
        10,
        21,
        second_capability.clone(),
        "guest-request-two",
    );
    let mut second_server = terminal_server_owned(
        &root,
        calls.clone(),
        &[],
        second_capability.clone(),
        "guest-request-two",
    );
    let first_log = Arc::new(Mutex::new(PtyLog::default()));
    let second_log = Arc::new(Mutex::new(PtyLog::default()));
    let mut first_custody = TerminalCustody::new(terminal_session(), observe.clone(), root.clone());
    let mut second_custody =
        TerminalCustody::new(terminal_session(), observe.clone(), root.clone());

    first_custody
        .open(
            &cx,
            first_run,
            vec![(1, Stream::Stdout)],
            Box::new(FakePty::new(first_log.clone())),
        )
        .expect("the first terminal opens");
    second_custody
        .open(
            &cx,
            second_run,
            vec![(1, Stream::Stdout)],
            Box::new(FakePty::new(second_log.clone())),
        )
        .expect("the second terminal opens");

    // Exercise each terminal using the matching kernel attachment while both
    // actual admitting connections remain live; this is not a wire-owner proof.
    for (custody, run, bytes) in [
        (&mut first_custody, first_run, b"b".as_slice()),
        (&mut second_custody, second_run, b"c".as_slice()),
    ] {
        let owner = run::resume_owner(&cx, &root, terminal_session(), run).expect("original owner");
        custody.start(&owner, &StreamStart::from([(host::TERMINAL_STREAM, 0)]));
        custody
            .input(&owner, bytes)
            .expect("matching terminal input");
    }
    wait_until("each terminal's own writer", || {
        written(&first_log) == b"b" && written(&second_log) == b"c"
    });

    let first_owner =
        run::resume_owner(&cx, &root, terminal_session(), first_run).expect("first owner");
    let second_owner =
        run::resume_owner(&cx, &root, terminal_session(), second_run).expect("second owner");
    assert_eq!(
        first_custody
            .input(&second_owner, b"crossed")
            .expect_err("second owner is foreign to first custody")
            .code,
        host::RefusalCode::Unavailable
    );
    assert_eq!(
        second_custody
            .input(&first_owner, b"crossed")
            .expect_err("first owner is foreign to second custody")
            .code,
        host::RefusalCode::Unavailable
    );
    assert_eq!(written(&first_log), b"b");
    assert_eq!(written(&second_log), b"c");

    let other_run = RunId::new(1, [11; 10]).expect("run id");
    let _third_owner = live_admission_named(
        &cx,
        &observe,
        &root,
        calls.clone(),
        11,
        31,
        second_capability.clone(),
        "guest-request-three",
    );

    // A client's terminal custody never reaches another run's writer, even when
    // the caller holds that run's real attachment.
    let other_owner =
        run::resume_owner(&cx, &root, terminal_session(), other_run).expect("other run's owner");
    let refusal = first_custody
        .input(&other_owner, b"crossed")
        .expect_err("one terminal custody owns one run's writer");
    assert_eq!(refusal.code, host::RefusalCode::Unavailable);
    assert_eq!(
        written(&second_log),
        b"c",
        "no terminal wrote into another run's writer"
    );

    // One terminal's capability is not another's: the second host refuses the
    // first client's credential before any request.
    let frames = {
        let mut frames = terminal_hello(first_capability);
        frames.push(terminal_invoke(RequestId::new([41; 16])));
        frames
    };
    let (mut transport, written) = scripted(&frames);
    let result = second_server.serve(&mut transport, &cx, &observe, &root, &mut second_custody);
    assert!(
        matches!(
            result,
            Err(host::Refusal {
                code: host::RefusalCode::Unauthenticated,
                ..
            })
        ),
        "a capability is one client's: {result:?}"
    );
    assert!(
        calls.lock().expect("calls").admitted.len() == 3,
        "the refused credential admitted nothing"
    );
    let written = decoded(&written.lock().expect("written"));
    assert!(
        !written
            .iter()
            .any(|frame| matches!(frame, host::Frame::Response(_))),
        "an unauthenticated connection is answered with no response: {written:?}"
    );
    _first_owner.assert_live();
    _second_owner.assert_live();
    _third_owner.assert_live();
}

#[test]
fn a_run_with_no_terminal_owner_refuses_input_and_attach() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let mut server = terminal_server(&root, calls.clone(), &[9], capability());
    let _owner = live_admission(&cx, &observe, &root, calls.clone());
    // The session owns no terminal for anything yet.
    let mut custody = TerminalCustody::new(terminal_session(), observe.clone(), root.clone());

    let attach = RequestId::new([2; 16]);
    let frames = {
        let mut frames = terminal_hello(capability());
        frames.push(terminal_attach(attach, run, None));
        frames
    };
    let (mut transport, written) = scripted(&frames);
    server
        .serve(&mut transport, &cx, &observe, &root, &mut custody)
        .expect("the connection answers its attach with a refusal");
    let written = decoded(&written.lock().expect("written"));
    assert!(
        written.iter().any(|frame| matches!(
            frame,
            host::Frame::Response(host::Response {
                outcome: host::ResponseOutcome::Refused(host::Refusal {
                    code: host::RefusalCode::InvalidRequest,
                    ..
                }),
                ..
            })
        )),
        "an attach with no terminal owner is refused instead of opening a stream: {written:?}"
    );
    assert!(
        !written
            .iter()
            .any(|frame| matches!(frame, host::Frame::Stream(_))),
        "a refused attach published nothing"
    );

    // A validated frame for a run this session has no owner for is refused with
    // no writer to invent and nothing written.
    let owner = run::resume_owner(&cx, &root, terminal_session(), run).expect("owner attachment");
    let refusal = custody
        .input(&owner, b"nowhere")
        .expect_err("no terminal owner holds this run's writer");
    assert_eq!(refusal.code, host::RefusalCode::Unavailable);
    assert_eq!(custody.runs(), 0);

    // Opening a run this session does not own a writer for refuses, rather than
    // opening a terminal nothing could type into.
    let other = RunId::new(1, [42; 10]).expect("run id");
    let log = Arc::new(Mutex::new(PtyLog::default()));
    assert!(matches!(
        custody.open(
            &cx,
            other,
            vec![(1, Stream::Stdout)],
            Box::new(FakePty::new(log.clone())),
        ),
        Err(TerminalRefusal::NoOwner)
    ));
    assert_eq!(custody.runs(), 0);
    assert_eq!(log.lock().expect("pty log").retired, 0);
    _owner.assert_live();
}

#[test]
fn a_released_bound_attachment_stops_its_queued_work() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let _owner = live_admission(&cx, &observe, &root, calls.clone());
    // The owning row, released from inside the writer's own effect: a handle of
    // its own, so the proof can end the owner's lifetime mid-frame.
    let mut releaser =
        run::resume_owner(&cx, &root, terminal_session(), run).expect("owner attachment");
    let releasing_context = cx.clone();
    let releasing_root = root.clone();
    let log = Arc::new(Mutex::new(PtyLog::default()));
    let mut custody = TerminalCustody::new(terminal_session(), observe.clone(), root.clone());
    custody
        .open(
            &cx,
            run,
            vec![(1, Stream::Stdout)],
            Box::new(
                FakePty::new(log.clone())
                    .bounded(4, usize::MAX)
                    .after_first_write(move || {
                        releaser
                            .release(&releasing_context, &releasing_root, Release::Eof)
                            .expect("the owning client's end of stream");
                    }),
            ),
        )
        .expect("the run's terminal opens");
    let witness =
        run::resume_owner(&cx, &root, terminal_session(), run).expect("owning attachment");
    custody.start(&witness, &StreamStart::from([(host::TERMINAL_STREAM, 0)]));

    // Eight bytes, four per write effect: the first chunk lands, the attachment
    // that queued it is released, and the second chunk must never be written.
    // Nothing can be queued on a second terminal's authority, so this released
    // row is the only authority the queue ever had.
    custody
        .input(&witness, b"12345678")
        .expect("the owning attachment types");
    wait_until("the writer to retire on the released row", || {
        log.lock().expect("pty log").retired == 1
    });
    assert_eq!(
        written(&log),
        b"1234",
        "exactly one partial write landed; the next chunk was fenced out"
    );
    let terminal = custody.terminal_mut(run).expect("the run's terminal");
    assert!(terminal.retired(), "the released row retired the writer");
    assert_eq!(terminal.pending(), 0, "the retired writer drops its queue");
    _owner.assert_live();
}

#[test]
fn a_queued_resize_is_fenced_by_the_owners_lifetime() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let _owner = live_admission(&cx, &observe, &root, calls.clone());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    let mut custody = TerminalCustody::new(terminal_session(), observe.clone(), root.clone());
    custody
        .open(
            &cx,
            run,
            vec![(1, Stream::Stdout)],
            Box::new(FakePty::new(log.clone())),
        )
        .expect("the run's terminal opens");
    let mut owner =
        run::resume_owner(&cx, &root, terminal_session(), run).expect("owning attachment");
    custody.start(&owner, &StreamStart::from([(host::TERMINAL_STREAM, 0)]));

    // The resize handle is full, so the newest geometry cannot land yet.
    log.lock().expect("pty log").resize_open = false;
    custody
        .resize(&owner, size(50, 120))
        .expect("the owning attachment resizes its terminal");
    std::thread::sleep(Duration::from_millis(50));
    assert!(sizes(&log).is_empty(), "the full handle applied nothing");

    // The owning client's stream ends while the geometry is still queued...
    owner
        .release(&cx, &root, Release::Eof)
        .expect("the owning client's end of stream");
    // ...and the handle frees immediately afterwards. The next turn re-reads the
    // owning row before its effect, so the queued geometry is never applied; a
    // pump that skipped that read would apply it here.
    log.lock().expect("pty log").resize_open = true;
    wait_until("the writer to retire on the released row", || {
        log.lock().expect("pty log").retired == 1
    });
    assert!(
        sizes(&log).is_empty(),
        "a queued geometry was applied after the owning row was released: {:?}",
        sizes(&log)
    );
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        sizes(&log).is_empty(),
        "the retired writer never applies a size"
    );
    _owner.assert_live();
}

#[test]
fn a_full_writer_never_starves_the_newest_geometry() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let _owner = live_admission(&cx, &observe, &root, calls.clone());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    let mut custody = TerminalCustody::new(terminal_session(), observe.clone(), root.clone());
    // A child that has stopped reading: the writer takes four bytes and then
    // reports a full buffer forever. Queued input never drains.
    custody
        .open(
            &cx,
            run,
            vec![(1, Stream::Stdout)],
            Box::new(FakePty::new(log.clone()).bounded(4, 4)),
        )
        .expect("the run's terminal opens");
    let owner = run::resume_owner(&cx, &root, terminal_session(), run).expect("owning attachment");
    custody.start(&owner, &StreamStart::from([(host::TERMINAL_STREAM, 0)]));

    custody
        .input(&owner, b"12345678")
        .expect("the owning attachment types");
    wait_until("the writer to take what it accepts", || {
        written(&log) == b"1234"
    });

    // The newest geometry must still reach the writer, even though input stays
    // queued behind a full writer for as long as the child is not reading.
    custody
        .resize(&owner, size(50, 120))
        .expect("the owning attachment resizes its terminal");
    wait_until("the newest geometry while input is stuck", || {
        sizes(&log) == vec![size(50, 120)]
    });
    // A newer size replaces it without a backlog, and the queued input is kept.
    custody
        .resize(&owner, size(60, 140))
        .expect("the owning attachment resizes again");
    wait_until("the newest geometry to win", || {
        sizes(&log) == vec![size(50, 120), size(60, 140)]
    });
    assert_eq!(
        written(&log),
        b"1234",
        "the full writer still took nothing more"
    );
    let terminal = custody.terminal_mut(run).expect("the run's terminal");
    assert_eq!(terminal.pending(), 4, "the queued input was not dropped");
    assert_eq!(terminal.applied(), Some(size(60, 140)));
    assert!(
        !terminal.retired(),
        "a full writer is not a retired terminal"
    );
    _owner.assert_live();
}

#[test]
fn a_refused_resize_never_starves_queued_input() {
    let cx = effects();
    let fixture = effigy_test_support::Fixture::new("workspace-terminal").expect("fixture");
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let observe = observation();
    terminal_session_row(&cx, &root);
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    let run = RunId::new(1, [9; 10]).expect("run id");
    let _owner = live_admission(&cx, &observe, &root, calls.clone());
    let log = Arc::new(Mutex::new(PtyLog::default()));
    let mut custody = TerminalCustody::new(terminal_session(), observe.clone(), root.clone());
    // A resize handle that never accepts: the geometry waits forever, and the
    // input behind it must still arrive. The switch is the test's, so the result
    // does not depend on how many turns the pump takes.
    log.lock().expect("pty log").resize_open = false;
    custody
        .open(
            &cx,
            run,
            vec![(1, Stream::Stdout)],
            Box::new(FakePty::new(log.clone())),
        )
        .expect("the run's terminal opens");
    let owner = run::resume_owner(&cx, &root, terminal_session(), run).expect("owning attachment");
    custody.start(&owner, &StreamStart::from([(host::TERMINAL_STREAM, 0)]));

    custody
        .resize(&owner, size(50, 120))
        .expect("the owning attachment resizes its terminal");
    custody
        .input(&owner, b"typed input")
        .expect("the owning attachment types");
    wait_until("queued input to arrive despite the stuck resize", || {
        written(&log) == b"typed input"
    });
    assert!(sizes(&log).is_empty(), "the stuck handle applied nothing");
    let terminal = custody.terminal_mut(run).expect("the run's terminal");
    assert_eq!(terminal.delivered(), 11);
    assert_eq!(terminal.pending(), 0);
    assert_eq!(terminal.applied(), None);
    _owner.assert_live();
}

#[test]
fn owner_eof_retires_the_writer_without_further_traffic() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let run = RunId::new(1, [9; 10]).expect("run id");
        let attach = RequestId::new([2; 16]);
        let typing = {
            let mut frames = terminal_hello(capability());
            frames.push(terminal_invoke(RequestId::new([1; 16])));
            frames.push(terminal_attach(attach, run, None));
            frames.push(typed(attach, 0, b"before the owner leaves"));
            frames
        };
        let (typing_client, typing_host) = live_connection(&typing);
        typing_host
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("bounded owner read");
        typing_host
            .set_write_timeout(Some(Duration::from_secs(30)))
            .expect("bounded owner write");
        let acceptor = PeerAccept {
            connections: VecDeque::from(vec![TestTransport::Peer(PeerTransport::new(typing_host))]),
        };

        let (listener, key, _acceptor) = SessionTlsListener::bind(
            SocketAddr::from(([127u8, 0, 0, 1], 0)),
            Duration::from_secs(1),
        )
        .expect("bound session listener");
        let mut launch = SessionLaunch::launch(
            &mut supervisor,
            &session,
            "485-supervisor",
            key,
            Box::new(listener),
        )
        .expect("session listener custody");
        launch
            .qualify(&mut VerifiedProbe, Duration::from_secs(1))
            .expect("qualified");
        launch.publish(&supervisor).expect("published");
        let mut owner = ServingOwner::new(
            supervisor,
            session.clone(),
            launch,
            acceptor,
            serving_server_admitting(calls.clone(), root, &[9]),
            Arc::new(cx.clone()),
            Arc::new(observe.clone()),
            root,
        );
        let log = Arc::new(Mutex::new(PtyLog::default()));
        let mut terminals = OpenAfterAdmission {
            custody: owner.terminal_custody(observe),
            effect: cx.clone(),
            writer: Some(Box::new(FakePty::new(log.clone()))),
        };
        std::thread::scope(|scope| {
            let _close = CloseOwnerWrite(typing_client.try_clone().expect("owner shutdown guard"));
            let serving = scope.spawn(|| owner.serve_pending(&mut terminals));
            wait_until("the typed frame to reach the writer", || {
                written(&log) == b"before the owner leaves"
            });
            typing_client
                .shutdown(std::net::Shutdown::Write)
                .expect("owner EOF");
            serving
                .join()
                .expect("serving thread")
                .expect("the owner connection is served")
                .expect("a connection was waiting");
        });
        // The owning client's end of stream released its row. No frame, pump call
        // or further connection traffic exists: the writer's own pump notices the
        // released row and retires the writer exactly once.
        wait_until("the writer to retire on the owner's EOF", || {
            log.lock().expect("pty log").retired == 1
        });
        assert!(
            terminals
                .custody
                .terminal_mut(run)
                .expect("the run's terminal")
                .retired(),
            "the owner EOF retired the terminal"
        );
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(
            log.lock().expect("pty log").retired,
            1,
            "the writer is retired once, not on every idle tick"
        );
    });
}

#[test]
fn the_serving_owner_serves_the_terminal_custody_it_hands_over() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let run = RunId::new(1, [9; 10]).expect("run id");
        let attach = RequestId::new([2; 16]);
        let typing = {
            let mut frames = terminal_hello(capability());
            frames.push(terminal_invoke(RequestId::new([1; 16])));
            frames.push(terminal_attach(attach, run, None));
            frames.push(typed(attach, 0, b"held by the serving owner"));
            frames
        };
        let (typing_client, typing_host) = live_connection(&typing);
        typing_host
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("bounded owner read");
        typing_host
            .set_write_timeout(Some(Duration::from_secs(30)))
            .expect("bounded owner write");
        let acceptor = PeerAccept {
            connections: VecDeque::from(vec![TestTransport::Peer(PeerTransport::new(typing_host))]),
        };

        let (listener, key, _acceptor) = SessionTlsListener::bind(
            SocketAddr::from(([127u8, 0, 0, 1], 0)),
            Duration::from_secs(1),
        )
        .expect("bound session listener");
        let mut launch = SessionLaunch::launch(
            &mut supervisor,
            &session,
            "485-supervisor",
            key,
            Box::new(listener),
        )
        .expect("session listener custody");
        launch
            .qualify(&mut VerifiedProbe, Duration::from_secs(1))
            .expect("qualified");
        launch.publish(&supervisor).expect("published");
        let mut owner = ServingOwner::new(
            supervisor,
            session.clone(),
            launch,
            acceptor,
            serving_server_admitting(calls.clone(), root, &[9]),
            Arc::new(cx.clone()),
            Arc::new(observe.clone()),
            root,
        );
        let log = Arc::new(Mutex::new(PtyLog::default()));
        let custody = owner.terminal_custody(observe);
        assert_eq!(
            custody.session(),
            session.session(),
            "custody remains session-bound"
        );
        let mut terminals = OpenAfterAdmission {
            custody,
            effect: cx.clone(),
            writer: Some(Box::new(FakePty::new(log.clone()))),
        };
        let served = std::thread::scope(|scope| {
            let _close = CloseOwnerWrite(typing_client.try_clone().expect("owner shutdown guard"));
            let serving = scope.spawn(|| owner.serve_pending(&mut terminals));
            wait_until("the served frame to reach the writer", || {
                written(&log) == b"held by the serving owner"
            });
            typing_client
                .shutdown(std::net::Shutdown::Write)
                .expect("owner EOF");
            serving
                .join()
                .expect("serving thread")
                .expect("the owner connection is served")
                .expect("a connection was waiting")
        });
        assert_eq!(served.responses, 2, "invoke and attach were answered");
        assert_eq!(owner.served(), 1);
        assert_eq!(
            runs_admitted(&calls),
            vec![run],
            "the serving owner's dispatcher admitted the terminal's run"
        );
    });
}

#[test]
fn the_retained_service_hands_over_a_session_bound_terminal_custody() {
    let calls: SharedCalls = Arc::new(Mutex::new(Calls::default()));
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let (listener, key, acceptor) = SessionTlsListener::bind(
            SocketAddr::from(([127u8, 0, 0, 1], 0)),
            Duration::from_secs(1),
        )
        .expect("bound session listener");
        let mut launch = SessionLaunch::launch(
            &mut supervisor,
            &session,
            "485-supervisor",
            key,
            Box::new(listener),
        )
        .expect("session listener custody");
        launch
            .qualify(&mut VerifiedProbe, Duration::from_secs(1))
            .expect("qualified");
        launch.publish(&supervisor).expect("published");
        let lease = launch.lease().expect("the published lease").clone();
        let owner = ServingOwner::new(
            supervisor,
            session.clone(),
            launch,
            acceptor,
            serving_server_for(calls.clone(), root),
            Arc::new(cx.clone()),
            Arc::new(observe.clone()),
            root,
        );
        let mut service = super::supervisor::ServiceOwner::retained(owner, session.clone(), lease)
            .expect("the session's service");
        let mut terminals = service.terminal_custody(observe);
        assert_eq!(terminals.session(), session.session());
        // A run this session never admitted has no owning attachment: the
        // handoff refuses rather than opening a terminal nothing could type into.
        let run = RunId::new(1, [9; 10]).expect("run id");
        let log = Arc::new(Mutex::new(PtyLog::default()));
        assert!(matches!(
            terminals.open(
                cx,
                run,
                vec![(1, Stream::Stdout)],
                Box::new(FakePty::new(log.clone())),
            ),
            Err(TerminalRefusal::NoOwner)
        ));
        assert_eq!(terminals.runs(), 0);
        assert!(
            !service
                .serve_pending(&mut terminals)
                .expect("a turn with nothing waiting"),
            "nothing was waiting on the retained service"
        );
        let _ = service.abandon();
    });
}

fn runs_admitted(calls: &SharedCalls) -> Vec<RunId> {
    calls.lock().expect("calls").admitted.clone()
}

#[test]
fn level_one_query_requires_selected_peer_capability_before_sending_inputs() {
    let reply = host::encode_frame(&host::Frame::HelloOk(host::HelloOk {
        capabilities: vec![],
        protocol: host::PROTOCOL_V1,
        effigy_version: running_binary().version,
    }))
    .unwrap();
    let mut transport = MuteTransport::answering_negotiation();
    transport.input = (reply.len() as u32)
        .to_be_bytes()
        .into_iter()
        .chain(reply)
        .collect();
    let written = transport.written.clone();
    let mut client = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
    client
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    let before = written.lock().unwrap().len();
    let intent = Intent::new(RequestBody::Observe(ObserveBody {
        query: Some(host::ObserveQuery {
            cwd: "private-cwd".into(),
            args: vec!["doctor".into(), "private-argument".into()],
            output: host::OutputMode::Json,
        }),
        op: "doctor".into(),
        target: Some(Target::Host),
        run_id: None,
    }));
    let error = client.send(&intent).unwrap_err();
    assert!(
        matches!(&error, ChannelError::Refused { code: "forbidden", message } if message.contains("observe_query_v1"))
    );
    assert!(!format!("{error:?}").contains("private"));
    assert_eq!(
        written.lock().unwrap().len(),
        before,
        "unnegotiated query wrote private inputs"
    );
}

#[test]
fn level_one_dispatch_never_reopens_after_unknown_or_uses_guest_fallback() {
    struct CountingOpener {
        opened: usize,
    }
    impl ChannelOpener for CountingOpener {
        fn open(
            &mut self,
            verified: &VerifiedWorkspaceContext,
            live: &LiveGuestRun,
        ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
            self.opened += 1;
            Ok(Box::new(WorkspaceClient::connect(
                MuteTransport::answering_negotiation(),
                verified.clone(),
                live.clone(),
            )?))
        }
    }
    let context = workspace_document(Some(session_run()), None);
    let mut guest = FakeGuest::new();
    publish(&mut guest, &context, Some(&capability()));
    let mut checkout = FakeCheckout::default();
    let mut opener = CountingOpener { opened: 0 };
    let error = dispatch_in(
        &parsed(&["build"]),
        &[],
        &published(&context),
        &mut guest,
        &mut checkout,
        &mut opener,
        Path::new(GUEST_CHECKOUT),
    )
    .unwrap_err();
    assert!(error.to_string().contains("unknown"));
    assert_eq!(opener.opened, 1, "Unknown reopened and resent the request");
    assert_eq!(
        checkout.reads, 0,
        "Unknown fell back to guest-local checkout reads"
    );
}

#[test]
fn level_one_hidden_dispatch_refuses_while_limited_probe_entry_keeps_existing_bypass() {
    struct ReadForbidden;
    impl GuestReader for ReadForbidden {
        fn read_file(&mut self, _: &str) -> Result<Option<Vec<u8>>, RunnerError> {
            panic!("hidden request read guest authority");
        }
        fn file_sha256(&mut self, _: &str) -> Result<Option<String>, RunnerError> {
            panic!("hidden request hashed guest authority");
        }
    }
    let probe = Command::InternalGuestProbe(effigy_cli::GuestProbeArgs::Identity);
    assert!(
        guest_command(&probe, None, Path::new("/private-producer"))
            .unwrap()
            .is_none(),
        "the limited probe entry must retain its existing bypass"
    );
    let context = workspace_document(Some(session_run()), None);
    let mut checkout = FakeCheckout::default();
    for command in [
        probe,
        Command::InternalScriptRun(effigy_cli::InternalScriptRunArgs {
            file: "/private-script".into(),
            repo_root: None,
            task_name: None,
            args: vec![],
        }),
    ] {
        let error = dispatch_in(
            &command,
            &[],
            &published(&context),
            &mut ReadForbidden,
            &mut checkout,
            &mut NoChannel,
            Path::new(GUEST_CHECKOUT),
        )
        .unwrap_err();
        assert!(error.to_string().contains("hidden internal"));
        assert!(!error.to_string().contains("private"));
    }
    assert_eq!(checkout.reads, 0);
}

#[test]
fn level_one_qa_status_and_stop_route_to_read_and_existing_control() {
    assert!(
        matches!(classify(&parsed(&["tasks", "qa-group", "status", "run"])), GuestOperation::Observe { op, .. } if op == "tasks:qa-group:status")
    );
    assert!(
        matches!(classify(&parsed(&["tasks", "qa-group", "stop", "run"])), GuestOperation::Invoke { selector: Selector::Operation(op), .. } if op == "tasks:qa-group:stop")
    );
}

#[test]
fn level_one_old_strict_peer_refusal_names_unsupported_feature_before_authentication() {
    let response = host::encode_frame(&host::Frame::Refuse(host::Refusal::invalid(
        "unknown hello field",
    )))
    .unwrap();
    let mut transport = MuteTransport::answering_negotiation();
    transport.input = (response.len() as u32)
        .to_be_bytes()
        .into_iter()
        .chain(response)
        .collect();
    let written = transport.written.clone();
    let error = WorkspaceClient::connect(transport, verified(), live_run()).unwrap_err();
    assert!(
        matches!(error, ChannelError::Refused { code: "forbidden", message } if message.contains("observe_query_v1"))
    );
    let bytes = written.lock().unwrap();
    let length = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
    assert_eq!(
        bytes.len(),
        length + 4,
        "unsupported negotiation wrote more than hello"
    );
    assert!(matches!(
        host::decode_frame(&bytes[4..]).unwrap(),
        host::Frame::Hello(_)
    ));
}

#[test]
fn level_one_unknown_intent_clone_cannot_be_resent_after_transport_loss() {
    let intent = Intent::new(RequestBody::Invoke(InvocationBody {
        selector: Selector::Task("build".into()),
        cwd: ".".into(),
        target: Target::Host,
        args: vec![],
        output: ChannelOutputMode::Text,
    }));
    let mut first = WorkspaceClient::connect(
        MuteTransport::answering_negotiation(),
        verified(),
        live_run(),
    )
    .unwrap();
    first
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    assert!(matches!(
        first.send(&intent),
        Err(ChannelError::Unknown { .. })
    ));
    let cloned = intent.clone();
    let transport = MuteTransport::answering_negotiation();
    let written = transport.written.clone();
    let mut second = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
    second
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    let before = written.lock().unwrap().len();
    let _refused = second.send(&cloned).unwrap_err();
    assert_eq!(
        written.lock().unwrap().len(),
        before,
        "an indeterminate clone resent its request"
    );
}

fn level_one_append_response(
    transport: &mut MuteTransport,
    intent: &Intent,
    outcome: host::ResponseOutcome,
) {
    let bytes = host::encode_frame(&host::Frame::Response(host::Response {
        id: intent.id(),
        outcome,
    }))
    .unwrap();
    transport.input.extend((bytes.len() as u32).to_be_bytes());
    transport.input.extend(bytes);
}

#[test]
fn level_one_host_unknown_fences_all_intent_clones_but_known_answers_allow_safe_replay() {
    for unknown in [true, false] {
        let intent = Intent::new(RequestBody::Observe(ObserveBody {
            query: None,
            op: "doctor".into(),
            target: Some(Target::Host),
            run_id: None,
        }));
        let mut transport = MuteTransport::answering_negotiation();
        let response = || {
            if unknown {
                host::ResponseOutcome::Refused(host::Refusal::unknown(
                    "outcome cannot be established",
                ))
            } else {
                host::ResponseOutcome::Observed(serde_json::json!({"observed":true}))
            }
        };
        level_one_append_response(&mut transport, &intent, response());
        level_one_append_response(&mut transport, &intent, response());
        let written = transport.written.clone();
        let mut client = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
        client
            .authenticate(GuestAuth::Capability(capability()))
            .unwrap();
        let first = client.send(&intent);
        let before = written.lock().unwrap().len();
        let cloned = intent.clone();
        let second = client.send(&cloned);
        if unknown {
            assert!(matches!(first, Err(ChannelError::Unknown { .. })));
            assert!(matches!(second, Err(ChannelError::Unknown { .. })));
            assert_eq!(
                written.lock().unwrap().len(),
                before,
                "host Unknown resent a clone"
            );
        } else {
            assert!(
                first.is_ok() && second.is_ok(),
                "a known answer must retain safe replay semantics"
            );
            assert!(written.lock().unwrap().len() > before);
        }
    }
}

#[test]
fn level_one_prewrite_refusal_does_not_mark_an_unsent_intent_indeterminate() {
    let intent = Intent::new(RequestBody::Observe(ObserveBody {
        query: Some(host::ObserveQuery {
            cwd: ".".into(),
            args: vec!["doctor".into()],
            output: host::OutputMode::Text,
        }),
        op: "doctor".into(),
        target: Some(Target::Host),
        run_id: None,
    }));
    let hello = host::encode_frame(&host::Frame::HelloOk(host::HelloOk {
        capabilities: vec![],
        protocol: host::PROTOCOL_V1,
        effigy_version: running_binary().version,
    }))
    .unwrap();
    let mut transport = MuteTransport::answering_negotiation();
    transport.input = (hello.len() as u32)
        .to_be_bytes()
        .into_iter()
        .chain(hello)
        .collect();
    let written = transport.written.clone();
    let mut client = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
    client
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    let before = written.lock().unwrap().len();
    assert!(matches!(
        client.send(&intent),
        Err(ChannelError::Refused {
            code: "forbidden",
            ..
        })
    ));
    assert_eq!(written.lock().unwrap().len(), before);
    let transport = MuteTransport::answering_negotiation();
    let written = transport.written.clone();
    let mut capable = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
    capable
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    let before = written.lock().unwrap().len();
    assert!(matches!(
        capable.send(&intent.clone()),
        Err(ChannelError::Unknown { .. })
    ));
    assert!(
        written.lock().unwrap().len() > before,
        "an unsent intent was incorrectly fenced"
    );
}

#[test]
fn level_one_partial_write_is_unknown_and_never_resends() {
    struct PartialWrite(MuteTransport, bool);
    impl Transport for PartialWrite {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.0.read(bytes)
        }
        fn verified_peer(&self) -> Option<host::ServerIdentity> {
            self.0.verified_peer()
        }
        fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
            if self.1 {
                self.0
                    .written
                    .lock()
                    .unwrap()
                    .extend_from_slice(&bytes[..2]);
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "partial request"))
            } else {
                self.0.write_all(bytes)
            }
        }
    }
    let transport = PartialWrite(MuteTransport::answering_negotiation(), false);
    let written = transport.0.written.clone();
    let mut client = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
    client
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    client.transport.1 = true;
    let intent = Intent::new(RequestBody::Observe(ObserveBody {
        query: None,
        op: "doctor".into(),
        target: Some(Target::Host),
        run_id: None,
    }));
    assert!(matches!(
        client.send(&intent),
        Err(ChannelError::Unknown { .. })
    ));
    let before = written.lock().unwrap().len();
    assert!(matches!(
        client.send(&intent.clone()),
        Err(ChannelError::Unknown { .. })
    ));
    assert_eq!(
        written.lock().unwrap().len(),
        before,
        "partial-write Unknown resent bytes"
    );
}

#[test]
fn level_one_concurrent_intent_clone_refuses_before_a_second_transport_write() {
    use std::sync::mpsc;
    struct BlockingWrite {
        inner: MuteTransport,
        started: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl Transport for BlockingWrite {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.inner.read(bytes)
        }
        fn verified_peer(&self) -> Option<host::ServerIdentity> {
            self.inner.verified_peer()
        }
        fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
            if matches!(host::decode_frame(&bytes[4..]), Ok(host::Frame::Request(_))) {
                self.started
                    .send(())
                    .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
                self.release
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?;
            }
            self.inner.write_all(bytes)
        }
    }
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let transport = BlockingWrite {
        inner: MuteTransport::answering_negotiation(),
        started: started_tx,
        release: release_rx,
    };
    let mut first = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
    first
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    let intent = Intent::new(RequestBody::Observe(ObserveBody {
        query: None,
        op: "doctor".into(),
        target: Some(Target::Host),
        run_id: None,
    }));
    let transport = MuteTransport::answering_negotiation();
    let written = transport.written.clone();
    let mut second = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
    second
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    let before = written.lock().unwrap().len();
    let cloned = intent.clone();
    let first_thread = std::thread::spawn(move || first.send(&cloned));
    let ready = started_rx.recv_timeout(Duration::from_secs(5));
    let refused = second.send(&intent);
    let released = release_tx.send(());
    let first_result = first_thread.join();
    assert!(ready.is_ok() && released.is_ok());
    assert!(matches!(
        first_result.unwrap(),
        Err(ChannelError::Unknown { .. })
    ));
    assert!(matches!(refused, Err(ChannelError::Local(message)) if message.contains("in flight")));
    assert_eq!(
        written.lock().unwrap().len(),
        before,
        "a concurrent clone wrote a second request"
    );
    assert!(matches!(
        second.send(&intent),
        Err(ChannelError::Unknown { .. })
    ));
    assert_eq!(written.lock().unwrap().len(), before);
}

#[derive(Clone, Debug)]
enum LevelOnePostwriteReply {
    Malformed,
    Oversized,
    Mismatched,
    Unexpected,
    HostUnknown,
    UnreadableAttach,
    Typed {
        method: host::Method,
        outcome: host::ResponseOutcome,
    },
    RequestAttachment {
        run: RunId,
        cursor: u64,
        client: effigy_kernel::run::Client,
        wrong_id: bool,
    },
}

fn level_one_postwrite_peer_exchange(
    reply: LevelOnePostwriteReply,
) -> (Intent, Result<ChannelOutcome, ChannelError>, Vec<RunId>) {
    level_one_postwrite_peer_exchange_in(reply, false)
}

fn level_one_postwrite_peer_exchange_in(
    reply: LevelOnePostwriteReply,
    admit_first: bool,
) -> (Intent, Result<ChannelOutcome, ChannelError>, Vec<RunId>) {
    let method = match &reply {
        LevelOnePostwriteReply::UnreadableAttach
        | LevelOnePostwriteReply::RequestAttachment { .. } => host::Method::Attach,
        LevelOnePostwriteReply::Typed { method, .. } => *method,
        _ => host::Method::Invoke,
    };
    let body = match method {
        host::Method::Observe => RequestBody::Observe(ObserveBody {
            query: None,
            op: "doctor".into(),
            target: Some(Target::Host),
            run_id: None,
        }),
        host::Method::Cancel => RequestBody::Cancel(host::RunBody { run: session_run() }),
        host::Method::Detach => RequestBody::Detach(host::RunBody { run: session_run() }),
        host::Method::Attach => RequestBody::Attach(AttachBody {
            run: session_run(),
            streams: vec![StreamKind::Stdout],
            cursor: None,
            cursors: None,
        }),
        host::Method::Invoke => RequestBody::Invoke(InvocationBody {
            selector: Selector::Task("build".into()),
            cwd: ".".into(),
            target: Target::Host,
            args: vec![],
            output: ChannelOutputMode::Text,
        }),
    };
    let intent = Intent::new(body);
    let (client_side, host_side) = connection_pair();
    // Unlike a preloaded reply, this peer cannot answer until it has decoded
    // the actual request. Every refusal below therefore follows delivery.
    let peer = std::thread::spawn(move || -> Result<RequestId, String> {
        let mut transport = PeerTransport::new(host_side);
        let hello = read_frame(&mut transport).map_err(|error| format!("{error:?}"))?;
        if !matches!(hello, Frame::Hello(_)) {
            return Err("expected hello".into());
        }
        host::write_frame(
            &mut transport,
            &Frame::HelloOk(host::HelloOk {
                capabilities: vec![host::ChannelCapability::ObserveQueryV1],
                protocol: host::PROTOCOL_V1,
                effigy_version: running_binary().version,
            }),
        )
        .map_err(|error| error.to_string())?;
        if !matches!(
            read_frame(&mut transport).map_err(|error| format!("{error:?}"))?,
            Frame::Authenticate(_)
        ) {
            return Err("expected authentication".into());
        }
        if admit_first {
            let Frame::Request(admission) =
                read_frame(&mut transport).map_err(|error| format!("{error:?}"))?
            else {
                return Err("expected admission request".into());
            };
            if !matches!(admission.body, RequestBody::Invoke(_)) {
                return Err("expected invoke before attach".into());
            }
            host::write_frame(
                &mut transport,
                &Frame::Response(host::Response {
                    id: admission.id,
                    outcome: level_one_accepted_reply(),
                }),
            )
            .map_err(|error| error.to_string())?;
        }
        let Frame::Request(request) =
            read_frame(&mut transport).map_err(|error| format!("{error:?}"))?
        else {
            return Err("expected request".into());
        };
        let response = match reply {
            LevelOnePostwriteReply::Malformed => {
                let bytes = b"not-json-private-reply";
                transport
                    .write_all(&(bytes.len() as u32).to_be_bytes())
                    .map_err(|error| error.to_string())?;
                transport
                    .write_all(bytes)
                    .map_err(|error| error.to_string())?;
                return Ok(request.id);
            }
            LevelOnePostwriteReply::Oversized => {
                transport
                    .write_all(&((host::MAX_FRAME_BYTES + 1) as u32).to_be_bytes())
                    .map_err(|error| error.to_string())?;
                return Ok(request.id);
            }
            LevelOnePostwriteReply::Mismatched => {
                let mut foreign = *request.id.bytes();
                foreign[0] ^= 1;
                Frame::Response(host::Response {
                    id: RequestId::new(foreign),
                    outcome: host::ResponseOutcome::Observed(
                        serde_json::json!({"private-reply":true}),
                    ),
                })
            }
            LevelOnePostwriteReply::Unexpected => Frame::HelloOk(host::HelloOk {
                capabilities: vec![],
                protocol: host::PROTOCOL_V1,
                effigy_version: running_binary().version,
            }),
            LevelOnePostwriteReply::HostUnknown => Frame::Response(host::Response {
                id: request.id,
                outcome: host::ResponseOutcome::Refused(host::Refusal::unknown(
                    "the host cannot establish this outcome",
                )),
            }),
            LevelOnePostwriteReply::Typed { outcome, .. } => Frame::Response(host::Response {
                id: request.id,
                outcome,
            }),
            LevelOnePostwriteReply::RequestAttachment {
                run,
                cursor,
                client,
                wrong_id,
            } => Frame::Response(host::Response {
                id: request.id,
                outcome: level_one_request_attachment_reply(
                    request.id, run, cursor, client, wrong_id,
                ),
            }),
            LevelOnePostwriteReply::UnreadableAttach => Frame::Response(host::Response {
                id: request.id,
                outcome: host::ResponseOutcome::Observed(serde_json::json!({"private-reply":true})),
            }),
        };
        host::write_frame(&mut transport, &response).map_err(|error| error.to_string())?;
        Ok(request.id)
    });
    let (outcome, admitted) =
        match WorkspaceClient::connect(PeerTransport::new(client_side), verified(), live_run()) {
            Ok(mut client) => {
                let outcome = client
                    .authenticate(GuestAuth::Capability(capability()))
                    .and_then(|()| {
                        if admit_first {
                            client.send(&level_one_build_intent()).map(|_| ())
                        } else {
                            Ok(())
                        }
                    })
                    .and_then(|()| client.send(&intent));
                (
                    outcome,
                    client
                        .admitted
                        .values()
                        .map(|admission| admission.run)
                        .collect(),
                )
            }
            Err(error) => (Err(error), Vec::new()),
        };
    let received = peer
        .join()
        .expect("peer reaped")
        .expect("peer received and answered request");
    assert_eq!(received, intent.id());
    (intent, outcome, admitted)
}

fn level_one_postwrite_unknown_control(reply: LevelOnePostwriteReply) {
    let (intent, outcome, admitted) = level_one_postwrite_peer_exchange(reply.clone());
    assert!(
        admitted.is_empty(),
        "an invalid reply changed the admitted run"
    );
    level_one_unknown_has_no_clone_write(&intent, outcome);
}

fn level_one_unknown_has_no_clone_write(
    intent: &Intent,
    outcome: Result<ChannelOutcome, ChannelError>,
) {
    let error = outcome.expect_err("invalid post-delivery answer");
    assert!(
        matches!(&error, ChannelError::Unknown { .. }),
        "reported as {error:?} after host delivery"
    );
    let message = error.into_runner().to_string();
    assert!(
        message.contains("unknown")
            && message.contains("independent host CLI")
            && message.contains("do not submit it as a new request"),
        "{message}"
    );
    assert!(!message.contains("private-reply"));
    let transport = MuteTransport::answering_negotiation();
    let written = transport.written.clone();
    let mut reconnect = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
    reconnect
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    let before = written.lock().unwrap().len();
    assert!(matches!(
        reconnect.send(&intent.clone()),
        Err(ChannelError::Unknown { .. })
    ));
    assert_eq!(
        written.lock().unwrap().len(),
        before,
        "post-delivery Unknown clone resent"
    );
}

fn level_one_build_intent() -> Intent {
    Intent::new(RequestBody::Invoke(InvocationBody {
        selector: Selector::Task("build".into()),
        cwd: ".".into(),
        target: Target::Host,
        args: vec![],
        output: ChannelOutputMode::Text,
    }))
}

#[test]
fn level_one_postwrite_malformed_reply_is_unknown() {
    level_one_postwrite_unknown_control(LevelOnePostwriteReply::Malformed);
}
#[test]
fn level_one_postwrite_oversized_reply_is_unknown() {
    level_one_postwrite_unknown_control(LevelOnePostwriteReply::Oversized);
}
#[test]
fn level_one_postwrite_mismatched_reply_is_unknown() {
    level_one_postwrite_unknown_control(LevelOnePostwriteReply::Mismatched);
}
#[test]
fn level_one_postwrite_unexpected_reply_is_unknown() {
    level_one_postwrite_unknown_control(LevelOnePostwriteReply::Unexpected);
}
#[test]
fn level_one_postwrite_host_unknown_reports_no_resend_guidance() {
    level_one_postwrite_unknown_control(LevelOnePostwriteReply::HostUnknown);
}
#[test]
fn level_one_postwrite_unreadable_attach_reply_is_unknown() {
    level_one_postwrite_unknown_control(LevelOnePostwriteReply::UnreadableAttach);
}

fn level_one_accepted_reply() -> host::ResponseOutcome {
    host::ResponseOutcome::Accepted(host::Accepted {
        run: session_run(),
        placement: effigy_kernel::ctx::Placement::Host,
    })
}

fn level_one_completed_reply() -> host::ResponseOutcome {
    host::ResponseOutcome::Completed(host::Completed {
        outcome: serde_json::json!({"completed":true}),
        exit: host::ExitStatus {
            code: Some(0),
            signal: None,
        },
    })
}

fn level_one_attachment_reply(run: RunId, cursor: u64) -> host::ResponseOutcome {
    host::ResponseOutcome::Observed(
        serde_json::to_value(host::AttachView {
            attachment: effigy_kernel::run::AttachmentId::new([17; 16]),
            run,
            client: effigy_kernel::run::Client::Secondary,
            cursor,
        })
        .unwrap(),
    )
}

#[test]
fn level_one_wrong_same_id_outcome_type_is_unknown_for_every_request_method() {
    let mut cases = vec![(
        host::Method::Invoke,
        host::ResponseOutcome::Observed(serde_json::json!({"private-reply":true})),
    )];
    for method in [
        host::Method::Observe,
        host::Method::Attach,
        host::Method::Cancel,
        host::Method::Detach,
    ] {
        cases.push((method, level_one_accepted_reply()));
        cases.push((method, level_one_completed_reply()));
    }
    for (method, outcome) in cases {
        level_one_postwrite_unknown_control(LevelOnePostwriteReply::Typed { method, outcome });
    }
}

#[test]
fn level_one_wrong_same_id_outcome_payload_is_unknown() {
    for (method, outcome) in [
        (
            host::Method::Attach,
            level_one_attachment_reply(RunId::new(1, [44; 10]).unwrap(), 0),
        ),
        (
            host::Method::Attach,
            level_one_attachment_reply(session_run(), 1),
        ),
        (
            host::Method::Cancel,
            host::ResponseOutcome::Observed(serde_json::json!({"private-reply":true})),
        ),
        (
            host::Method::Detach,
            host::ResponseOutcome::Observed(serde_json::json!({"private-reply":true})),
        ),
        (
            host::Method::Invoke,
            host::ResponseOutcome::Accepted(host::Accepted {
                run: session_run(),
                placement: effigy_kernel::ctx::Placement::Guest,
            }),
        ),
    ] {
        level_one_postwrite_unknown_control(LevelOnePostwriteReply::Typed { method, outcome });
    }
}

fn level_one_request_attachment_reply(
    id: RequestId,
    run: RunId,
    cursor: u64,
    client: effigy_kernel::run::Client,
    wrong_id: bool,
) -> host::ResponseOutcome {
    let mut attachment = if client == effigy_kernel::run::Client::Owner {
        *effigy_kernel::run::owner_attachment_id(run).bytes()
    } else {
        *id.bytes()
    };
    if wrong_id {
        attachment[0] ^= 1;
    }
    host::ResponseOutcome::Observed(
        serde_json::to_value(host::AttachView {
            attachment: effigy_kernel::run::AttachmentId::new(attachment),
            run,
            client,
            cursor,
        })
        .unwrap(),
    )
}

#[test]
fn level_one_secondary_attachment_wrong_identity_is_unknown() {
    level_one_postwrite_unknown_control(LevelOnePostwriteReply::RequestAttachment {
        run: session_run(),
        cursor: 0,
        client: effigy_kernel::run::Client::Secondary,
        wrong_id: true,
    });
}

#[test]
fn level_one_nonadmitting_connection_cannot_accept_owner_role() {
    level_one_postwrite_unknown_control(LevelOnePostwriteReply::RequestAttachment {
        run: session_run(),
        cursor: 0,
        client: effigy_kernel::run::Client::Owner,
        wrong_id: false,
    });
}

#[test]
fn level_one_admitting_connection_validates_owner_identity_and_role() {
    for (client, wrong_id) in [
        (effigy_kernel::run::Client::Owner, false),
        (effigy_kernel::run::Client::Owner, true),
        (effigy_kernel::run::Client::Secondary, false),
    ] {
        let (intent, outcome, admitted) = level_one_postwrite_peer_exchange_in(
            LevelOnePostwriteReply::RequestAttachment {
                run: session_run(),
                cursor: 0,
                client,
                wrong_id,
            },
            true,
        );
        assert_eq!(admitted, vec![session_run()]);
        if client == effigy_kernel::run::Client::Owner && !wrong_id {
            let view = attach_view(&outcome.expect("the admitting connection's owner"));
            assert_eq!(view.client, effigy_kernel::run::Client::Owner);
            assert_eq!(
                view.attachment,
                effigy_kernel::run::owner_attachment_id(session_run())
            );
        } else {
            level_one_unknown_has_no_clone_write(&intent, outcome);
        }
    }
}

#[test]
fn level_one_known_admission_replay_does_not_grant_fresh_connection_ownership() {
    for client_role in [
        effigy_kernel::run::Client::Secondary,
        effigy_kernel::run::Client::Owner,
    ] {
        let invoke = level_one_build_intent();
        let attach = Intent::attach(session_run(), vec![StreamKind::Stdout], Default::default());
        // The first connection receives a definitive acceptance. Replaying
        // that known request is safe, but conveys no new owner authority.
        let mut initial_transport = MuteTransport::answering_negotiation();
        level_one_append_response(&mut initial_transport, &invoke, level_one_accepted_reply());
        let mut initial =
            WorkspaceClient::connect(initial_transport, verified(), live_run()).unwrap();
        initial
            .authenticate(GuestAuth::Capability(capability()))
            .unwrap();
        initial.send(&invoke).unwrap();
        assert_eq!(
            initial
                .admitted
                .values()
                .map(|admission| admission.run)
                .collect::<Vec<_>>(),
            vec![session_run()]
        );
        drop(initial);

        let mut transport = MuteTransport::answering_negotiation();
        level_one_append_response(&mut transport, &invoke, level_one_accepted_reply());
        level_one_append_response(
            &mut transport,
            &attach,
            level_one_request_attachment_reply(attach.id(), session_run(), 0, client_role, false),
        );
        let mut reconnect = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
        reconnect
            .authenticate(GuestAuth::Capability(capability()))
            .unwrap();
        reconnect.send(&invoke.clone()).unwrap();
        assert!(reconnect.admitted.is_empty());
        let outcome = reconnect.send(&attach);
        if client_role == effigy_kernel::run::Client::Secondary {
            assert_eq!(attach_view(&outcome.unwrap()).client, client_role);
        } else {
            level_one_unknown_has_no_clone_write(&attach, outcome);
        }
    }
}

#[test]
fn level_one_detach_retires_owner_eligibility_without_replay_revival() {
    for response in [
        host::ResponseOutcome::Observed(serde_json::Value::Null),
        host::ResponseOutcome::Refused(host::Refusal::unknown("detach outcome unknown")),
        host::ResponseOutcome::Refused(host::Refusal::forbidden("detach not allowed")),
    ] {
        let invoke = level_one_build_intent();
        let detach = Intent::new(RequestBody::Detach(host::RunBody { run: session_run() }));
        let mut transport = MuteTransport::answering_negotiation();
        level_one_append_response(&mut transport, &invoke, level_one_accepted_reply());
        level_one_append_response(&mut transport, &detach, response.clone());
        level_one_append_response(&mut transport, &invoke, level_one_accepted_reply());
        let mut client = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
        client
            .authenticate(GuestAuth::Capability(capability()))
            .unwrap();
        client.send(&invoke).unwrap();
        let outcome = client.send(&detach);
        let definite_refusal = matches!(
            response,
            host::ResponseOutcome::Refused(host::Refusal {
                code: host::RefusalCode::Forbidden,
                ..
            })
        );
        assert_eq!(client.admitted.is_empty(), !definite_refusal);
        assert_eq!(
            outcome.is_ok(),
            matches!(response, host::ResponseOutcome::Observed(_))
        );
        client.send(&invoke.clone()).unwrap();
        assert_eq!(
            client.admitted.is_empty(),
            !definite_refusal,
            "replay revived a retired owner"
        );
    }
}

#[test]
fn level_one_completion_retires_only_its_own_admission() {
    let first = level_one_build_intent();
    let second = level_one_build_intent();
    let other_run = RunId::new(1, [66; 10]).unwrap();
    let mut transport = MuteTransport::answering_negotiation();
    level_one_append_response(&mut transport, &first, level_one_accepted_reply());
    level_one_append_response(
        &mut transport,
        &second,
        host::ResponseOutcome::Accepted(host::Accepted {
            run: other_run,
            placement: effigy_kernel::ctx::Placement::Host,
        }),
    );
    level_one_append_response(&mut transport, &first, level_one_completed_reply());
    level_one_append_response(&mut transport, &first, level_one_accepted_reply());
    let mut client = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
    client
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    client.send(&first).unwrap();
    client.send(&second).unwrap();
    assert_eq!(client.admitted.len(), 2);
    client.send(&first).unwrap();
    assert_eq!(
        client
            .admitted
            .values()
            .map(|admission| admission.run)
            .collect::<Vec<_>>(),
        vec![other_run]
    );
    let error = client
        .send(&first)
        .expect_err("a completed request cannot admit again");
    assert!(matches!(error, ChannelError::Unknown { .. }));
    assert_eq!(
        client
            .admitted
            .values()
            .map(|admission| admission.run)
            .collect::<Vec<_>>(),
        vec![other_run]
    );
}

#[test]
fn level_one_completion_on_a_clone_retires_original_connection_ownership() {
    let invoke = level_one_build_intent();
    let still_owner = Intent::attach(session_run(), vec![StreamKind::Stdout], Default::default());
    let attach = Intent::attach(session_run(), vec![StreamKind::Stdout], Default::default());
    let mut transport = MuteTransport::answering_negotiation();
    level_one_append_response(&mut transport, &invoke, level_one_accepted_reply());
    level_one_append_response(
        &mut transport,
        &still_owner,
        level_one_request_attachment_reply(
            still_owner.id(),
            session_run(),
            0,
            effigy_kernel::run::Client::Owner,
            false,
        ),
    );
    level_one_append_response(
        &mut transport,
        &attach,
        level_one_request_attachment_reply(
            attach.id(),
            session_run(),
            0,
            effigy_kernel::run::Client::Owner,
            false,
        ),
    );
    let written = transport.written.clone();
    let mut original = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
    original
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    original.send(&invoke).unwrap();

    let mut replay_transport = MuteTransport::answering_negotiation();
    level_one_append_response(&mut replay_transport, &invoke, level_one_accepted_reply());
    level_one_append_response(&mut replay_transport, &invoke, level_one_completed_reply());
    let mut replay = WorkspaceClient::connect(replay_transport, verified(), live_run()).unwrap();
    replay
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    replay.send(&invoke.clone()).unwrap();
    assert_eq!(
        attach_view(
            &original
                .send(&still_owner)
                .expect("known acceptance preserves original owner")
        )
        .client,
        effigy_kernel::run::Client::Owner,
    );
    replay.send(&invoke.clone()).unwrap();

    let before = written.lock().unwrap().len();
    assert!(original.detach(session_run()).is_err());
    assert_eq!(
        written.lock().unwrap().len(),
        before,
        "completed owner sent detach"
    );
    level_one_unknown_has_no_clone_write(&attach, original.send(&attach));
}

#[test]
fn level_one_connection_cannot_change_its_authenticated_client() {
    let invoke = level_one_build_intent();
    let observe = Intent::new(RequestBody::Observe(ObserveBody {
        query: None,
        op: "doctor".into(),
        target: Some(Target::Host),
        run_id: None,
    }));
    let mut transport = MuteTransport::answering_negotiation();
    level_one_append_response(&mut transport, &invoke, level_one_accepted_reply());
    level_one_append_response(
        &mut transport,
        &observe,
        host::ResponseOutcome::Refused(host::Refusal::unauthenticated("credential revoked")),
    );
    let written = transport.written.clone();
    let mut client = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
    let before = written.lock().unwrap().len();
    assert!(matches!(
        client.send(&invoke),
        Err(ChannelError::Refused {
            code: "unauthenticated",
            ..
        })
    ));
    assert_eq!(
        written.lock().unwrap().len(),
        before,
        "request sent without authentication"
    );
    client
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    client.send(&invoke).unwrap();
    let before = written.lock().unwrap().len();
    assert!(client
        .authenticate(GuestAuth::Capability(Secret::new([88; 32])))
        .is_err());
    assert_eq!(
        written.lock().unwrap().len(),
        before,
        "credential changed on a live connection"
    );
    assert_eq!(client.admitted.len(), 1);
    assert!(client.send(&observe).is_err());
    assert!(
        client.admitted.is_empty(),
        "revoked authority retained owner eligibility"
    );
}

#[test]
fn level_one_partial_authentication_receipt_fences_all_later_writes() {
    use std::io::Read as _;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    struct PartialAuthentication {
        inner: PeerTransport,
        attempts: Arc<AtomicUsize>,
        received: mpsc::Receiver<()>,
    }
    impl Transport for PartialAuthentication {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.inner.read(bytes)
        }
        fn verified_peer(&self) -> Option<host::ServerIdentity> {
            self.inner.verified_peer()
        }
        fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
            if matches!(
                host::decode_frame(&bytes[4..]),
                Ok(Frame::Authenticate(_) | Frame::Enrol(_))
            ) {
                self.attempts.fetch_add(1, Ordering::SeqCst);
                self.inner.write_all(&bytes[..bytes.len() / 2])?;
                self.received
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?;
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "partial credential frame",
                ))
            } else {
                self.inner.write_all(bytes)
            }
        }
    }

    for (enrol, queued_refusal) in [(false, false), (true, false), (false, true), (true, true)] {
        let (client_side, host_side) = connection_pair();
        let (received_tx, received_rx) = mpsc::channel();
        let peer = std::thread::spawn(move || -> Result<(usize, usize), String> {
            let mut transport = PeerTransport::new(host_side);
            if !matches!(read_frame(&mut transport), Ok(Frame::Hello(_))) {
                return Err("expected hello".into());
            }
            host::write_frame(
                &mut transport,
                &Frame::HelloOk(host::HelloOk {
                    capabilities: vec![host::ChannelCapability::ObserveQueryV1],
                    protocol: host::PROTOCOL_V1,
                    effigy_version: running_binary().version,
                }),
            )
            .map_err(|error| error.to_string())?;
            let mut prefix = [0; 4];
            transport
                .stream
                .read_exact(&mut prefix)
                .map_err(|error| error.to_string())?;
            let body_len = u32::from_be_bytes(prefix) as usize;
            if body_len > host::MAX_FRAME_BYTES {
                return Err("authentication frame exceeded the bound".into());
            }
            let received = (body_len + 4) / 2;
            let mut partial = vec![0; received - 4];
            transport
                .stream
                .read_exact(&mut partial)
                .map_err(|error| error.to_string())?;
            // An unkeyed refusal cannot prove whether this partial credential
            // was accepted. Signal actual receipt before the write returns.
            if queued_refusal {
                host::write_frame(
                    &mut transport,
                    &Frame::Refuse(host::Refusal::unauthenticated(
                        "unkeyed queued authentication refusal",
                    )),
                )
                .map_err(|error| error.to_string())?;
            }
            transport
                .stream
                .shutdown(std::net::Shutdown::Write)
                .map_err(|error| error.to_string())?;
            received_tx.send(()).map_err(|error| error.to_string())?;
            let mut trailing = Vec::new();
            match transport.stream.read_to_end(&mut trailing) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {}
                Err(error) => return Err(error.to_string()),
            }
            Ok((received, trailing.len()))
        });
        let attempts = Arc::new(AtomicUsize::new(0));
        let mut client = WorkspaceClient::connect(
            PartialAuthentication {
                inner: PeerTransport::new(client_side),
                attempts: attempts.clone(),
                received: received_rx,
            },
            verified(),
            live_run(),
        )
        .unwrap();
        let auth = if enrol {
            GuestAuth::Enrolment {
                credential: capability(),
                session: SessionIdentity {
                    run: session_run(),
                    generation: Generation::new(1),
                },
            }
        } else {
            GuestAuth::Capability(capability())
        };
        let first = client.authenticate(auth);
        let second = client.authenticate(GuestAuth::Capability(Secret::new([0xcd; 32])));
        let intent = level_one_build_intent();
        let request = client.send(&intent);
        let retained_enrolment = client.enrolled.is_some();
        drop(client);
        let (received, trailing) = peer
            .join()
            .expect("peer reaped")
            .expect("peer read partial auth");
        assert!(received > 4);
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            1,
            "second credential write after partial receipt"
        );
        assert_eq!(
            trailing, 0,
            "bytes followed an indeterminate authentication frame"
        );
        assert!(
            !retained_enrolment,
            "unacknowledged enrolment retained staged authority"
        );
        for outcome in [first, second] {
            let error = outcome.expect_err("authentication ambiguity cannot grant access");
            assert!(matches!(error, ChannelError::Unknown { .. }), "{error:?}");
            let message = error.into_runner().to_string();
            assert!(!message.contains(&"ab".repeat(32)));
            assert!(message.contains("unknown"));
        }
        let error = request.expect_err("an uncompleted authentication sends no command");
        assert!(matches!(
            error,
            ChannelError::Refused {
                code: "unauthenticated",
                ..
            }
        ));
        assert!(error
            .into_runner()
            .to_string()
            .contains("no command was sent"));
        // This command was never encoded or written. Its intent therefore
        // remains usable on a fresh, successfully authenticated connection.
        let mut fresh_transport = MuteTransport::answering_negotiation();
        level_one_append_response(&mut fresh_transport, &intent, level_one_accepted_reply());
        let mut fresh = WorkspaceClient::connect(fresh_transport, verified(), live_run()).unwrap();
        fresh
            .authenticate(GuestAuth::Capability(capability()))
            .unwrap();
        assert!(matches!(
            fresh.send(&intent),
            Ok(ChannelOutcome::Accepted { .. })
        ));
    }
}

#[test]
fn level_one_authentication_prewrite_failure_keeps_the_connection_unsent() {
    for encoding in [false, true] {
        let observe = Intent::new(RequestBody::Observe(ObserveBody {
            query: None,
            op: "doctor".into(),
            target: Some(Target::Host),
            run_id: None,
        }));
        let mut transport = MuteTransport::answering_negotiation();
        level_one_append_response(
            &mut transport,
            &observe,
            host::ResponseOutcome::Observed(serde_json::Value::Null),
        );
        let written = transport.written.clone();
        let mut client = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
        if encoding {
            client.live.container.container_id = "x".repeat(host::MAX_FRAME_BYTES + 1);
        } else {
            client.live.environment_generation = "not-a-generation".into();
        }
        let before = written.lock().unwrap().len();
        let error = client
            .authenticate(GuestAuth::Enrolment {
                credential: capability(),
                session: SessionIdentity {
                    run: session_run(),
                    generation: Generation::new(1),
                },
            })
            .expect_err("invalid local authentication input");
        assert!(!matches!(error, ChannelError::Unknown { .. }));
        assert_eq!(written.lock().unwrap().len(), before);
        assert!(client.enrolled.is_none());
        client.live = live_run();
        client
            .authenticate(GuestAuth::Capability(capability()))
            .expect("no credential was attempted");
        assert!(matches!(
            client.send(&observe),
            Ok(ChannelOutcome::Observed(_))
        ));
    }
}

#[test]
fn level_one_partial_request_write_cannot_use_a_queued_unkeyed_refusal() {
    use std::io::Read as _;
    use std::sync::mpsc;
    struct PartialRequest(PeerTransport, mpsc::Receiver<()>);
    impl Transport for PartialRequest {
        fn verified_peer(&self) -> Option<host::ServerIdentity> {
            self.0.verified_peer()
        }
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.0.read(bytes)
        }
        fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
            if matches!(host::decode_frame(&bytes[4..]), Ok(Frame::Request(_))) {
                self.0.write_all(&bytes[..bytes.len() / 2])?;
                self.1
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?;
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "partial request"))
            } else {
                self.0.write_all(bytes)
            }
        }
    }
    let (queued_tx, queued_rx) = mpsc::channel();
    let (client_side, host_side) = connection_pair();
    let peer = std::thread::spawn(move || -> Result<usize, String> {
        let mut transport = PeerTransport::new(host_side);
        if !matches!(read_frame(&mut transport), Ok(Frame::Hello(_))) {
            return Err("hello".into());
        }
        host::write_frame(
            &mut transport,
            &Frame::HelloOk(host::HelloOk {
                capabilities: vec![host::ChannelCapability::ObserveQueryV1],
                protocol: host::PROTOCOL_V1,
                effigy_version: running_binary().version,
            }),
        )
        .map_err(|error| error.to_string())?;
        if !matches!(read_frame(&mut transport), Ok(Frame::Authenticate(_))) {
            return Err("credential".into());
        }
        let mut prefix = [0; 4];
        transport
            .stream
            .read_exact(&mut prefix)
            .map_err(|error| error.to_string())?;
        let body_len = u32::from_be_bytes(prefix) as usize;
        if body_len > host::MAX_FRAME_BYTES {
            return Err("request frame bound".into());
        }
        let mut partial = vec![0; (body_len + 4) / 2 - 4];
        transport
            .stream
            .read_exact(&mut partial)
            .map_err(|error| error.to_string())?;
        host::write_frame(
            &mut transport,
            &Frame::Refuse(host::Refusal::unauthenticated("unkeyed queued refusal")),
        )
        .map_err(|error| error.to_string())?;
        queued_tx.send(()).map_err(|error| error.to_string())?;
        let mut trailing = Vec::new();
        match transport.stream.read_to_end(&mut trailing) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {}
            Err(error) => return Err(error.to_string()),
        }
        Ok(trailing.len())
    });
    let mut client = WorkspaceClient::connect(
        PartialRequest(PeerTransport::new(client_side), queued_rx),
        verified(),
        live_run(),
    )
    .unwrap();
    client
        .authenticate(GuestAuth::Capability(capability()))
        .unwrap();
    let intent = level_one_build_intent();
    let first = client.send(&intent);
    let second = client.send(&intent.clone());
    drop(client);
    assert_eq!(
        peer.join()
            .expect("peer reaped")
            .expect("peer received partial request"),
        0
    );
    for outcome in [first, second] {
        let error = outcome.expect_err("an unkeyed refusal proves no request outcome");
        assert!(matches!(error, ChannelError::Unknown { .. }), "{error:?}");
        assert!(error
            .into_runner()
            .to_string()
            .contains("do not submit it as a new request"));
    }
}

#[test]
fn level_one_valid_method_outcome_matrix_and_definitive_refusals_stay_sendable() {
    let typed = |method, outcome| LevelOnePostwriteReply::Typed { method, outcome };
    let mut cases = vec![
        typed(host::Method::Invoke, level_one_accepted_reply()),
        typed(host::Method::Invoke, level_one_completed_reply()),
        typed(
            host::Method::Observe,
            host::ResponseOutcome::Observed(serde_json::json!({"observed":true})),
        ),
        LevelOnePostwriteReply::RequestAttachment {
            run: session_run(),
            cursor: 0,
            client: effigy_kernel::run::Client::Secondary,
            wrong_id: false,
        },
        typed(
            host::Method::Cancel,
            host::ResponseOutcome::Observed(serde_json::Value::Null),
        ),
        typed(
            host::Method::Detach,
            host::ResponseOutcome::Observed(serde_json::Value::Null),
        ),
    ];
    for method in [
        host::Method::Invoke,
        host::Method::Observe,
        host::Method::Attach,
        host::Method::Cancel,
        host::Method::Detach,
    ] {
        cases.push(typed(
            method,
            host::ResponseOutcome::Refused(host::Refusal::forbidden("known refusal")),
        ));
    }
    for reply in cases {
        let (intent, outcome, _) = level_one_postwrite_peer_exchange(reply.clone());
        let response = match reply {
            LevelOnePostwriteReply::Typed { outcome, .. } => outcome,
            LevelOnePostwriteReply::RequestAttachment {
                run,
                cursor,
                client,
                wrong_id,
            } => level_one_request_attachment_reply(intent.id(), run, cursor, client, wrong_id),
            _ => panic!("positive matrix has only typed responses"),
        };
        match &response {
            host::ResponseOutcome::Refused(_) => assert!(matches!(
                outcome,
                Err(ChannelError::Refused {
                    code: "forbidden",
                    ..
                })
            )),
            _ => assert!(outcome.is_ok(), "{response:?}: {outcome:?}"),
        }
        let mut transport = MuteTransport::answering_negotiation();
        level_one_append_response(&mut transport, &intent, response);
        let written = transport.written.clone();
        let mut replay = WorkspaceClient::connect(transport, verified(), live_run()).unwrap();
        replay
            .authenticate(GuestAuth::Capability(capability()))
            .unwrap();
        let before = written.lock().unwrap().len();
        let replayed = replay.send(&intent.clone());
        assert!(
            !matches!(replayed, Err(ChannelError::Unknown { .. })),
            "valid answer fenced replay"
        );
        assert!(written.lock().unwrap().len() > before);
    }
}

fn argv_fixture_stream_response(intent: &Intent) -> Option<Result<ChannelOutcome, ChannelError>> {
    let run = RunId::new(1, [41; 10]).unwrap();
    match &intent.body {
        RequestBody::Attach(_) => Some(Err(ChannelError::Refused {
            code: "unavailable",
            message: "already closed".into(),
        })),
        RequestBody::Observe(body) if body.op.starts_with("runs:outcome:") => {
            Some(Ok(ChannelOutcome::Observed(
                serde_json::json!({"state":"completed","outcome":{"workload":"Succeeded","cleanup_complete":true}}),
            )))
        }
        RequestBody::Observe(body) if body.op.starts_with("runs:output:") => {
            let empty =
                serde_json::json!({"from":0,"next":0,"bytes":"","gap":null,"truncated":false});
            Some(Ok(ChannelOutcome::Observed(
                serde_json::json!({"schema":"effigy.workspace-output.v1","run":run,"stdout":empty,"stderr":empty}),
            )))
        }
        _ => None,
    }
}

#[test]
fn level_one_dispatch_uses_explicit_frozen_arguments_and_checkout_relative_cwd() {
    struct Capture(Arc<Mutex<Vec<RequestBody>>>);
    impl ChannelRequest for Capture {
        fn authenticate(&mut self, _: GuestAuth) -> Result<(), ChannelError> {
            Ok(())
        }
        fn send(&mut self, intent: &Intent) -> Result<ChannelOutcome, ChannelError> {
            if let Some(response) = argv_fixture_stream_response(intent) {
                return response;
            }
            self.0.lock().unwrap().push(intent.body.clone());
            Ok(match intent.body {
                RequestBody::Invoke(_) => ChannelOutcome::Accepted {
                    run: RunId::new(1, [41; 10]).unwrap(),
                    placement: RunPlacement::Host,
                },
                _ => ChannelOutcome::Observed(serde_json::json!({"observed":true})),
            })
        }
        fn detach(&mut self, _: RunId) -> Result<(), ChannelError> {
            panic!("argument construction does not detach")
        }
    }
    impl ChannelOpener for Capture {
        fn open(
            &mut self,
            _: &VerifiedWorkspaceContext,
            _: &LiveGuestRun,
        ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
            Ok(Box::new(Self(self.0.clone())))
        }
    }
    // Request construction only; guest_run_status_reaches_real_host_dispatch
    // separately verifies these explicit args are parsed by the real observer.
    let operation = argv(&[
        "--json",
        "doctor",
        "nested/build",
        "--",
        "two words",
        "literal;$()",
    ]);
    let task_tail = argv(&["--", "two words", "literal;$()"]);
    let cases = [
        (
            parse_command(operation.clone()).unwrap(),
            operation.clone(),
            true,
        ),
        (
            Command::Task(effigy_cli::TaskInvocation {
                name: "build".into(),
                args: task_tail.clone(),
            }),
            task_tail.clone(),
            false,
        ),
    ];
    for (command, supplied, observed) in cases {
        let context = workspace_document(Some(session_run()), None);
        let mut guest = FakeGuest::new();
        publish(&mut guest, &context, Some(&capability()));
        let mut checkout = FakeCheckout::default();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut channel = Capture(captured.clone());
        dispatch_in(
            &command,
            &supplied,
            &published(&context),
            &mut guest,
            &mut checkout,
            &mut channel,
            Path::new("/workspace/nested"),
        )
        .unwrap();
        let captured = captured.lock().unwrap();
        assert_eq!(captured.len(), 1);
        if observed {
            let RequestBody::Observe(body) = &captured[0] else {
                panic!("Observe query");
            };
            let query = body.query.as_ref().unwrap();
            assert_eq!(query.args, operation);
            assert_eq!(query.cwd, "nested");
            assert_eq!(query.output, ChannelOutputMode::Json);
        } else {
            let RequestBody::Invoke(body) = &captured[0] else {
                panic!("task invocation");
            };
            assert_eq!(body.args, task_tail);
            assert_eq!(body.cwd, "nested");
            assert_eq!(body.selector, Selector::Task("build".into()));
        }
        assert_eq!(checkout.reads, 0);
    }
}

#[test]
fn level_one_pinned_or_refused_commands_do_not_capture_process_arguments() {
    for command in [
        Command::Version,
        parsed(&["tasks"]),
        Command::InternalGuestProbe(effigy_cli::GuestProbeArgs::Identity),
    ] {
        assert!(invocation_args(&command, None).unwrap().is_empty());
    }
}

#[test]
fn level_one_public_entry_keeps_original_argv_with_guest_io_faked() {
    const CHILD: &str = "EFFIGY_TEST_PUBLIC_ARGV_CHILD";
    const ROOT: &str = "EFFIGY_TEST_PUBLIC_ARGV_ROOT";
    let operation = argv(&[
        "--json",
        "doctor",
        "nested/build",
        "--",
        "two words",
        "literal;$()",
    ]);
    let status = argv(&["--json", "run", "status", &session_run().to_string()]);
    let task = argv(&["--json", "build", "--", "two words", "literal;$()"]);
    let local = argv(&["--json", "tasks"]);
    if std::env::var_os(CHILD).is_some() {
        struct Capture(Arc<Mutex<Vec<serde_json::Value>>>);
        impl ChannelRequest for Capture {
            fn authenticate(&mut self, _: GuestAuth) -> Result<(), ChannelError> {
                Ok(())
            }
            fn send(&mut self, intent: &Intent) -> Result<ChannelOutcome, ChannelError> {
                if let Some(response) = argv_fixture_stream_response(intent) {
                    return response;
                }
                self.0.lock().unwrap().push(serde_json::json!({"method":intent.body.method().as_str(), "body":&intent.body}));
                Ok(match intent.body {
                    RequestBody::Invoke(_) => ChannelOutcome::Accepted {
                        run: RunId::new(1, [41; 10]).unwrap(),
                        placement: RunPlacement::Host,
                    },
                    _ => ChannelOutcome::Observed(serde_json::json!({"ok":true})),
                })
            }
            fn detach(&mut self, _: RunId) -> Result<(), ChannelError> {
                panic!("public argv fixture does not detach")
            }
        }
        impl ChannelOpener for Capture {
            fn open(
                &mut self,
                _: &VerifiedWorkspaceContext,
                _: &LiveGuestRun,
            ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
                Ok(Box::new(Self(self.0.clone())))
            }
        }
        let project = PathBuf::from(std::env::var_os(ROOT).unwrap())
            .canonicalize()
            .unwrap();
        let mut context = workspace_document(Some(session_run()), None);
        context.0.paths.guest_checkout = project.display().to_string();
        context.0.paths.workdir = project.join("nested").display().to_string();
        let mut guest = FakeGuest::new();
        publish(&mut guest, &context, Some(&capability()));
        let published = published(&context);
        let mut checkout = FakeCheckout::default();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_by_hook = captured.clone();
        let mut channel = Capture(captured_by_hook);
        with_guest_command_io(
            move |cmd, args, cwd| {
                dispatch_in(
                    cmd,
                    args,
                    &published,
                    &mut guest,
                    &mut checkout,
                    &mut channel,
                    cwd,
                )
                .map(Some)
            },
            || {
                for explicit in [&operation, &status, &task, &local] {
                    assert_ne!(
                        *explicit,
                        std::env::args().skip(1).collect::<Vec<_>>(),
                        "fixture requires explicit argv unlike ambient process argv"
                    );
                    crate::run_main(explicit.clone()).unwrap();
                }
            },
        );
        std::fs::write(
            project.join("observed.json"),
            serde_json::to_vec(&*captured.lock().unwrap()).unwrap(),
        )
        .unwrap();
        return;
    }
    let fixture = effigy_test_support::Fixture::new("workspace-public-entry-argv").unwrap();
    let project = fixture.path().join("checkout");
    let task_home = fixture.path().join("home");
    std::fs::create_dir_all(project.join("nested")).unwrap();
    std::fs::create_dir_all(&task_home).unwrap();
    std::fs::write(
        project.join("effigy.toml"),
        "[tasks]\nbuild = 'echo unused'\n",
    )
    .unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child.args(["--exact", "runner::workspace_client::tests::level_one_public_entry_keeps_original_argv_with_guest_io_faked", "--nocapture"])
        .current_dir(project.join("nested")).env(CHILD, "1").env(ROOT, &project)
        .env("HOME", &task_home).env("XDG_CONFIG_HOME", task_home.join(".config"))
        .env("XDG_DATA_HOME", task_home.join(".local/share")).env("XDG_CACHE_HOME", task_home.join(".cache"))
        .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    // The selector supplies the real pool; retain it under this private HOME.
    if let Some(pool) = std::env::var_os("OXIDE_POOL") {
        child.env("OXIDE_POOL", pool);
    }
    let running = child.spawn().unwrap();
    let pid = running.id();
    let output = running.wait_with_output().unwrap();
    eprintln!("public-entry child pid={pid} exit={}", output.status);
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let observed: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(project.join("observed.json")).unwrap()).unwrap();
    assert_eq!(
        observed.len(),
        3,
        "pinned local tasks must not open the channel"
    );
    assert_eq!(
        observed[0]["body"]["query"]["args"],
        serde_json::json!(operation),
        "public run_main lost its original operation argv"
    );
    assert_eq!(
        observed[1]["body"]["query"]["args"],
        serde_json::json!(status)
    );
    let Command::Task(expected) = parse_command(task).unwrap() else {
        panic!("task");
    };
    assert_eq!(
        observed[2]["body"]["args"],
        serde_json::json!(expected.args)
    );
    for row in &observed[..2] {
        assert_eq!(row["body"]["query"]["cwd"], "nested");
        assert_eq!(row["body"]["query"]["output"], "json");
    }
    assert_eq!(observed[2]["body"]["cwd"], "nested");
    let json_start = output
        .stdout
        .iter()
        .position(|byte| *byte == b'{')
        .expect("JSON envelope");
    let mut values = serde_json::Deserializer::from_slice(&output.stdout[json_start..])
        .into_iter::<serde_json::Value>();
    let mut envelopes = Vec::new();
    while let Some(Ok(value)) = values.next() {
        envelopes.push(value);
    }
    assert_eq!(envelopes.len(), 4);
    assert!(envelopes
        .iter()
        .all(|row| row["schema"] == "effigy.command.v1"));
    assert_eq!(
        envelopes[3]["result"]["tasks"],
        serde_json::json!(["build", "seed"])
    );
}

#[test]
fn level_one_embedded_builtins_require_matching_original_arguments() {
    let fixture = effigy_test_support::Fixture::new("workspace-embedded-argv").unwrap();
    std::fs::write(
        fixture.path().join("effigy.toml"),
        "[tasks]\nbuild = 'echo unused'\n",
    )
    .unwrap();
    let context = effigy_context::EffigyRuntimeContext::builder()
        .cwd_override(Some(fixture.path().to_owned()))
        .captured_env(effigy_context::CapturedEnv {
            home: Some(fixture.path().join("home").into_os_string()),
            ..Default::default()
        })
        .capture()
        .unwrap();
    let command = parsed(&["doctor", "first-private-selector"]);
    with_guest_command_io(
        |_, _, _| panic!("invalid original arguments reached guest I/O"),
        || {
            let missing = crate::runner::run_command_with_context(command.clone(), &context)
                .unwrap_err()
                .to_string();
            assert!(
                missing.contains("original arguments") && missing.contains("independent host CLI")
            );
            let mismatch = crate::runner::run_command_with_context_and_args(
                command,
                &context,
                &argv(&["doctor", "other-private-selector"]),
            )
            .unwrap_err()
            .to_string();
            assert!(mismatch.contains("do not match"));
            assert!(
                !mismatch.contains("first-private-selector")
                    && !mismatch.contains("other-private-selector")
            );
        },
    );
    let tail = argv(&["--", "embedded value"]);
    let expected = tail.clone();
    with_guest_command_io(
        move |cmd, args, _| {
            assert!(matches!(cmd, Command::Task(task) if task.name == "build"));
            assert_eq!(args, expected);
            Ok(Some("canonical task".into()))
        },
        || {
            assert_eq!(
                crate::runner::run_command_with_context(
                    Command::Task(effigy_cli::TaskInvocation {
                        name: "build".into(),
                        args: tail
                    }),
                    &context
                )
                .unwrap(),
                "canonical task"
            );
        },
    );
}
