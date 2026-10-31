//! The reserved publication input builder over fake backend observations.
//!
//! Every test here drives the real builder. Only the backend/provisioning
//! observation seam is scripted: the plan, the alias derivation, the rendered
//! policy, the predecessor guard, the decision comparison, the artifact
//! authentication and the deadline are the production ones. No test starts a
//! container, applies Compose or switches a selector.

use std::cell::Cell;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use effigy_containers::environment::{BackendKind, BackendReason, BackendSelection};
use effigy_containers::BackendId;
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::id::{Digest, RunId};
use effigy_kernel::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use effigy_kernel::workspace_session::{
    ArtifactEvidence, ContainerIdentity, PublishInputs, TargetEvidence,
};
use effigy_test_support::Fixture;

use super::*;
use crate::runner::error::RunnerError;
use crate::runner::workspace_client::context::{
    GuestReader, GuestWorkspace, COMPOSED_MANIFEST_KIND,
};
use crate::runner::workspace_command::provisioning::reserved_guest::WorkspaceGuestProbe;
use crate::runner::workspace_command::provisioning::{
    workspace_artifact_receipt_path, workspace_effigy_active_version_file,
};
use crate::runner::workspace_transport::qualification::{BindRoute, GuestChannelProbe};

const CONTAINER_ID: &str = "9d1c0b6a7f5e4d3c2b1a09f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4";
const BOOT_ID: &str = "3f6a1c92-5b7e-4c1d-8a2f-0e9d7c6b5a41";
const OTHER_CONTAINER_ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OTHER_BOOT_ID: &str = "8b7e4c1d-3f6a-5b7e-9a2f-6b5a41c92d0e";

fn evidence() -> Evidence {
    Evidence::new("test observation".into(), None).expect("fixed evidence")
}

/// The machine type this process runs, so "native" is the case under test.
fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => panic!("no pinned family for this host: {other}"),
    }
}

fn host_machine() -> &'static str {
    match host_arch() {
        "amd64" => "x86_64",
        _ => "aarch64",
    }
}

fn other_arch() -> &'static str {
    match host_arch() {
        "amd64" => "arm64",
        _ => "amd64",
    }
}

fn host_family() -> LinuxWorkspaceTarget {
    LinuxWorkspaceTarget::from_image(host_machine(), "gnu").expect("host family")
}

/// What a native backend reports for this host's own images.
fn image_facts() -> ObservedImagePlatform {
    ObservedImagePlatform {
        os: "linux".into(),
        architecture: host_arch().into(),
        variant: String::new(),
    }
}

fn container_mode_manifest(root: &Path) {
    std::fs::write(
        root.join("effigy.toml"),
        r#"[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"
working_dir = "/workspace"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace"
"#,
    )
    .expect("container-mode manifest");
    std::fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n",
    )
    .expect("compose file");
}

/// A checkout whose `web` container declares an operator workspace, with the
/// snapshotted render the alias plan renders into.
fn checkout(label: &str) -> (Fixture, PathBuf, PathBuf) {
    let fixture = Fixture::new(&format!("effigy-reserved-input-{label}-")).expect("fixture");
    let root = fixture.path().join("checkout");
    let home = fixture.path().join("home/.effigy");
    std::fs::create_dir_all(&root).expect("checkout");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::create_dir_all(root.join(".git")).expect("git directory");
    container_mode_manifest(&root);
    (fixture, root, home)
}

fn rendered_policy(root: &Path) -> EffectiveContainerPolicy {
    let mut policy = effigy_containers::load_container_policy(root, Some("web")).expect("policy");
    policy.workspace_user = Some("dev".to_owned());
    policy.workspace_home = Some("/home/dev".to_owned());
    assert!(policy.render.is_some(), "the alias render needs a snapshot");
    policy
}

fn colima_selection(profile: &str) -> BackendSelection {
    BackendSelection {
        kind: BackendKind::Colima,
        engine: BackendId::colima_nerdctl(),
        context: profile.to_owned(),
        reason: BackendReason::ManifestDriver("colima".into()),
    }
}

fn desktop_selection() -> BackendSelection {
    BackendSelection {
        kind: BackendKind::DockerDesktop,
        engine: BackendId::docker_compose(),
        context: "desktop-linux".to_owned(),
        reason: BackendReason::ManifestDriver("docker".into()),
    }
}

fn engine_selection() -> BackendSelection {
    BackendSelection {
        kind: BackendKind::DockerEngine,
        engine: BackendId::docker_compose(),
        context: "default".to_owned(),
        reason: BackendReason::ManifestDriver("docker".into()),
    }
}

fn host_identity(platform: &str, libc: &str) -> WorkspaceContainerIdentity {
    WorkspaceContainerIdentity {
        container_id: CONTAINER_ID.to_owned(),
        vm_boot_id: BOOT_ID.to_owned(),
        platform: platform.to_owned(),
        libc: libc.to_owned(),
    }
}

fn idle_predecessor() -> PredecessorState {
    PredecessorState {
        running: false,
        desired: None,
    }
}

/// The scripted backend/provisioning seam. Production never constructs it.
struct FakeBackend {
    selection: Observed<BackendSelection>,
    addressing: Observed<ReservedAddressing>,
    predecessor: Observed<PredecessorState>,
    binding: Observed<VerifiedBinding>,
    identity: Observed<WorkspaceContainerIdentity>,
    image: Observed<ObservedImagePlatform>,
    artifact: Observed<PathBuf>,
    /// How long each observation pretends to take, so expiry can be exercised
    /// while one is in flight.
    takes: Option<Duration>,
    reads: Rc<Cell<usize>>,
}

impl FakeBackend {
    fn container_mode(selection: BackendSelection, addressing: ReservedAddressing) -> Self {
        Self {
            selection: Observed::Verified(selection.clone(), evidence()),
            addressing: Observed::Verified(addressing, evidence()),
            predecessor: Observed::Verified(idle_predecessor(), evidence()),
            binding: Observed::Verified(
                VerifiedBinding {
                    container_id: CONTAINER_ID.to_owned(),
                    selected: selection.clone(),
                },
                evidence(),
            ),
            identity: Observed::Verified(host_identity(host_machine(), "gnu"), evidence()),
            image: Observed::Verified(image_facts(), evidence()),
            artifact: Observed::Verified(PathBuf::from("/nonexistent/artifact"), evidence()),
            takes: None,
            reads: Rc::new(Cell::new(0)),
        }
    }

    fn colima() -> Self {
        Self::container_mode(
            colima_selection("effigy"),
            ReservedAddressing {
                proxy_bind: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                colima_gateway: Some("192.168.5.2".parse().expect("gateway")),
            },
        )
    }

    fn count(&self) -> usize {
        self.reads.get()
    }

    fn call(&self) {
        self.reads.set(self.reads.get() + 1);
        if let Some(takes) = self.takes {
            std::thread::sleep(takes);
        }
    }

    fn with_artifact(mut self, artifact: PathBuf) -> Self {
        self.artifact = Observed::Verified(artifact, evidence());
        self
    }

    fn with_identity(mut self, identity: WorkspaceContainerIdentity) -> Self {
        self.identity = Observed::Verified(identity, evidence());
        self
    }

    fn with_image(mut self, image: ObservedImagePlatform) -> Self {
        self.image = Observed::Verified(image, evidence());
        self
    }

    fn with_predecessor(mut self, predecessor: PredecessorState) -> Self {
        self.predecessor = Observed::Verified(predecessor, evidence());
        self
    }
}

impl ReservedBackendObservation for FakeBackend {
    fn selection(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: PublicationDeadline,
    ) -> Observed<BackendSelection> {
        self.call();
        self.selection.clone()
    }
    fn addressing(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: &BackendSelection,
        _: PublicationDeadline,
    ) -> Observed<ReservedAddressing> {
        self.call();
        self.addressing.clone()
    }
    fn predecessor(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: PublicationDeadline,
    ) -> Observed<PredecessorState> {
        self.call();
        self.predecessor.clone()
    }
    fn binding(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: PublicationDeadline,
    ) -> Observed<VerifiedBinding> {
        self.call();
        self.binding.clone()
    }
    fn identity(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: &str,
        _: PublicationDeadline,
    ) -> Observed<WorkspaceContainerIdentity> {
        self.call();
        self.identity.clone()
    }
    fn image_platform(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: &str,
        _: PublicationDeadline,
    ) -> Observed<ObservedImagePlatform> {
        self.call();
        self.image.clone()
    }
    fn artifact(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: LinuxWorkspaceTarget,
        _: PublicationDeadline,
    ) -> Observed<PathBuf> {
        self.call();
        self.artifact.clone()
    }
    fn guest<'a>(
        &'a mut self,
        _: &'a EffectiveContainerPolicy,
        _: &ContainerIdentity,
        _: PublicationDeadline,
    ) -> Result<Box<dyn GuestWorkspace + 'a>, EnvError> {
        Ok(Box::new(NoGuest))
    }
}

/// A guest that refuses every operation: the builder under test is asserted to
/// perform no guest I/O of its own.
struct NoGuest;

fn refused_guest<T>() -> Result<T, RunnerError> {
    Err(RunnerError::task_invocation(
        "the input builder performed guest I/O",
    ))
}

impl GuestReader for NoGuest {
    fn read_file(&mut self, _: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        refused_guest()
    }
    fn file_sha256(&mut self, _: &str) -> Result<Option<String>, RunnerError> {
        refused_guest()
    }
}

impl GuestWorkspace for NoGuest {
    fn read_link(&mut self, _: &str) -> Result<Option<String>, RunnerError> {
        refused_guest()
    }
    fn ensure_dir(&mut self, _: &str, _: u32) -> Result<(), RunnerError> {
        refused_guest()
    }
    fn write_file(&mut self, _: &str, _: &[u8], _: u32) -> Result<(), RunnerError> {
        refused_guest()
    }
    fn set_owner(&mut self, _: &str, _: &str) -> Result<(), RunnerError> {
        refused_guest()
    }
    fn stage_host_file(&mut self, _: &Path, _: &str) -> Result<(), RunnerError> {
        refused_guest()
    }
    fn install_executable(&mut self, _: &str, _: &str) -> Result<(), RunnerError> {
        refused_guest()
    }
    fn replace_pointer(&mut self, _: &str, _: &str) -> Result<(), RunnerError> {
        refused_guest()
    }
    fn replace_link(&mut self, _: &str, _: &str) -> Result<(), RunnerError> {
        refused_guest()
    }
    fn probe_container_identity(&mut self) -> Result<WorkspaceContainerIdentity, RunnerError> {
        refused_guest()
    }
}

/// The injected probe. The builder only carries it; this test never publishes.
struct NoProbe;

impl GuestChannelProbe for NoProbe {
    fn resolve(
        &mut self,
        _: &WorkspaceContainerIdentity,
        _: &str,
        _: Duration,
    ) -> Observed<crate::runner::workspace_transport::qualification::ResolvedAlias> {
        Observed::Absent(AbsenceProof::NoRow {
            table: "test".to_string(),
        })
    }
    fn dial(
        &mut self,
        _: &WorkspaceContainerIdentity,
        _: &str,
        _: &Digest,
        _: Duration,
    ) -> Observed<crate::runner::workspace_transport::qualification::GuestDialEvidence> {
        Observed::Absent(AbsenceProof::NoRow {
            table: "test".to_string(),
        })
    }
}

impl WorkspaceGuestProbe for NoProbe {
    fn context_dial(
        &mut self,
        _: &WorkspaceContainerIdentity,
        _: &GuestObject,
        _: &GuestObject,
        _: Duration,
    ) -> Observed<crate::runner::workspace_transport::qualification::GuestDialEvidence> {
        Observed::Absent(AbsenceProof::NoRow {
            table: "test".to_string(),
        })
    }
    fn identity(
        &mut self,
        _: &WorkspaceContainerIdentity,
        _: &GuestObject,
        _: Duration,
    ) -> Observed<crate::runner::workspace_client::identity::GuestSelfObservation> {
        Observed::Absent(AbsenceProof::NoRow {
            table: "test".to_string(),
        })
    }
}

/// A produced artifact and the producer's own receipt and version beside it.
fn produced_artifact(root: &Path, family: LinuxWorkspaceTarget) -> PathBuf {
    let artifact = root
        .join("artifacts")
        .join(format!("effigy-{}", family.release_triple()));
    std::fs::create_dir_all(artifact.parent().expect("parent")).expect("artifact directory");
    let bytes = b"pinned guest artifact".to_vec();
    std::fs::write(&artifact, &bytes).expect("artifact bytes");
    std::fs::write(
        workspace_effigy_active_version_file(&artifact),
        format!("{}\n", effigy_core::build_info::active_version()),
    )
    .expect("active version");
    std::fs::write(
        workspace_artifact_receipt_path(&artifact),
        format!(
            "release_triple={}\nsha256={}\n",
            family.release_triple(),
            Digest::of(&bytes).hex()
        ),
    )
    .expect("receipt");
    artifact
}

fn decision(inputs: PublishInputs) -> ReservedDecisionFacts {
    ReservedDecisionFacts {
        inputs,
        reservation_generation: 7,
        created_by: RunId::new(1_760_000_000_000, [1; 10]).expect("run id"),
    }
}

/// The decision the caller would freeze for a fresh project on this plan.
fn fresh_decision(plan: &ReservedPublicationPlan) -> ReservedDecisionFacts {
    let inputs = plan
        .publish_inputs(
            ArtifactEvidence::Unselected {
                image_reference: "example:one".into(),
                backend: plan.backend_label().to_owned(),
            },
            TargetEvidence::Absent {
                project: plan.policy().project_name.clone(),
                backend: plan.backend_label().to_owned(),
            },
        )
        .expect("fresh decision inputs");
    decision(inputs)
}

/// The decision the caller would freeze when it re-observed a live target.
fn reobserved_decision(
    plan: &ReservedPublicationPlan,
    target: ContainerIdentity,
) -> ReservedDecisionFacts {
    let inputs = plan
        .publish_inputs(
            ArtifactEvidence::Selected {
                digest: Digest::of(b"previous artifact"),
                object: format!(
                    "/run/effigy/objects/bin/{}",
                    Digest::of(b"previous artifact")
                ),
            },
            TargetEvidence::Observed(target),
        )
        .expect("reobserved decision inputs");
    decision(inputs)
}

fn deadline() -> PublicationDeadline {
    PublicationDeadline::new(Instant::now() + Duration::from_secs(30))
}

/// The production probe factory shape: it only needs the verified target.
fn probe_factory<'a>() -> ReservedProbeFactory<'a> {
    Box::new(|_target| Ok(Box::new(NoProbe) as Box<dyn WorkspaceGuestProbe>))
}

fn plan_for(
    policy: &EffectiveContainerPolicy,
    backend: &mut FakeBackend,
) -> Result<Option<ReservedPublicationPlan>, EnvError> {
    plan(
        ReservedPlanRequest {
            policy,
            deadline: deadline(),
        },
        backend,
    )
}

// ------------------------------------------------------- the mode boundary

#[test]
fn a_hybrid_environment_reads_no_backend_fact() {
    let (_fixture, root, home) = checkout("hybrid");
    std::fs::write(root.join("effigy.toml"), "[tasks]\n").expect("hybrid manifest");
    let mut policy = crate::runner::test_support::effective_container_policy(
        "web",
        "demo-web",
        "app",
        root.join("compose.yml"),
    );
    policy.repo_root = root.clone();
    policy.workspace_user = Some("dev".to_owned());
    policy.workspace_home = Some("/home/dev".to_owned());
    let mut backend = FakeBackend::colima();

    let planned =
        effigy_containers::with_test_effigy_home(&home, || plan_for(&policy, &mut backend))
            .expect("hybrid planning");

    assert!(planned.is_none(), "hybrid publishes nothing");
    assert_eq!(backend.count(), 0, "hybrid reads no backend fact");
}

// ------------------------------------------------------------ alias planning

#[test]
fn colima_preserves_the_admitted_profile_and_renders_the_observed_gateway() {
    let (_fixture, root, home) = checkout("colima");
    let policy = rendered_policy(&root);
    let mut backend = FakeBackend::colima();

    let planned =
        effigy_containers::with_test_effigy_home(&home, || plan_for(&policy, &mut backend))
            .expect("plan")
            .expect("container mode");

    assert_eq!(planned.alias().backend, Backend::Colima);
    assert_eq!(planned.alias().profile, "effigy");
    assert_eq!(planned.alias().render_value, "192.168.5.2");
    assert_eq!(
        planned.alias().route,
        BindRoute::Proxy {
            host_bind: IpAddr::V4(Ipv4Addr::LOCALHOST)
        }
    );
    assert_eq!(planned.backend_label(), "colima-nerdctl");
    assert_eq!(planned.guest_inputs().profile, "effigy");
    // The same rendered policy the apply is handed carries the alias Compose
    // must give the guest, and its closure digest is the one a live predecessor
    // must already have published.
    let render = planned.policy().render.as_ref().expect("render");
    assert_eq!(planned.render_digest(), render.closure.digest());
    let override_path = planned
        .policy()
        .compose_files
        .last()
        .expect("alias override");
    assert!(override_path.starts_with(&render.path));
    let rendered = planned
        .policy()
        .read_compose(override_path)
        .expect("in-memory closure");
    assert!(
        rendered.contains("effigy-host: 192.168.5.2") || rendered.contains("\"192.168.5.2\""),
        "{rendered}"
    );
    // A decision input that names another backend is refused, not published.
    assert!(planned
        .publish_inputs(
            ArtifactEvidence::Unselected {
                image_reference: "example:one".into(),
                backend: planned.backend_label().to_owned(),
            },
            TargetEvidence::Absent {
                project: planned.policy().project_name.clone(),
                backend: planned.backend_label().to_owned(),
            },
        )
        .is_ok());
    assert!(planned
        .publish_inputs(
            ArtifactEvidence::Unselected {
                image_reference: "example:one".into(),
                backend: "docker-compose".into(),
            },
            TargetEvidence::Absent {
                project: planned.policy().project_name.clone(),
                backend: "colima-nerdctl".into(),
            },
        )
        .is_err());
}

#[test]
fn docker_desktop_renders_the_host_gateway_token() {
    let (_fixture, root, home) = checkout("desktop");
    let policy = rendered_policy(&root);
    let mut backend = FakeBackend::container_mode(
        desktop_selection(),
        ReservedAddressing {
            proxy_bind: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            colima_gateway: None,
        },
    );

    let planned =
        effigy_containers::with_test_effigy_home(&home, || plan_for(&policy, &mut backend))
            .expect("plan")
            .expect("container mode");

    assert_eq!(planned.alias().backend, Backend::DockerDesktop);
    assert_eq!(planned.alias().render_value, "host-gateway");
    assert_eq!(planned.alias().profile, "desktop-linux");
    assert_eq!(planned.backend_label(), "docker-compose");
}

#[test]
fn linux_docker_binds_directly_and_needs_no_proxy_address() {
    let (_fixture, root, home) = checkout("linux");
    let policy = rendered_policy(&root);
    let mut backend = FakeBackend::container_mode(
        engine_selection(),
        ReservedAddressing {
            proxy_bind: None,
            colima_gateway: None,
        },
    );

    let planned =
        effigy_containers::with_test_effigy_home(&home, || plan_for(&policy, &mut backend))
            .expect("plan")
            .expect("container mode");

    assert_eq!(planned.alias().backend, Backend::LinuxDocker);
    assert_eq!(planned.alias().route, BindRoute::Direct);
    assert_eq!(planned.alias().render_value, "host-gateway");
}

#[test]
fn an_alias_address_no_observation_supplied_refuses() {
    let (_fixture, root, home) = checkout("alias-negatives");
    let policy = rendered_policy(&root);
    for (selection, addressing, expected) in [
        (
            colima_selection("effigy"),
            ReservedAddressing {
                proxy_bind: None,
                colima_gateway: Some("192.168.5.2".parse().expect("gateway")),
            },
            "proxy bind",
        ),
        (
            colima_selection("effigy"),
            ReservedAddressing {
                proxy_bind: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                colima_gateway: None,
            },
            "gateway",
        ),
        (
            desktop_selection(),
            ReservedAddressing {
                proxy_bind: None,
                colima_gateway: None,
            },
            "proxy bind",
        ),
    ] {
        let mut backend = FakeBackend::container_mode(selection, addressing);
        let error = effigy_containers::with_test_effigy_home(&home, || {
            plan_for(&policy, &mut backend).expect_err("no observed address")
        })
        .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn an_unknown_backend_selection_refuses_before_any_render() {
    let (_fixture, root, home) = checkout("selection-unknown");
    let policy = rendered_policy(&root);
    let mut backend = FakeBackend::colima();
    backend.selection = Observed::Unknown(UnknownReason::StoreUnreadable("no record".into()));

    let error = effigy_containers::with_test_effigy_home(&home, || {
        plan_for(&policy, &mut backend).expect_err("unknown selection")
    })
    .to_string();

    assert!(error.contains("backend selection is unknown"), "{error}");
    assert_eq!(backend.count(), 1, "the plan stops at the selection");
}

#[test]
fn an_expired_deadline_refuses_before_planning() {
    let (_fixture, root, _home) = checkout("expired");
    let policy = rendered_policy(&root);
    let mut backend = FakeBackend::colima();
    let expired = PublicationDeadline::new(Instant::now() - Duration::from_secs(1));

    let error = plan(
        ReservedPlanRequest {
            policy: &policy,
            deadline: expired,
        },
        &mut backend,
    )
    .expect_err("expired budget")
    .to_string();

    assert!(error.contains("deadline expired"), "{error}");
    assert_eq!(backend.count(), 0, "an expired budget spawns nothing");
}

// ------------------------------------------------- the live predecessor guard

#[test]
fn a_live_predecessor_on_this_very_render_is_compatible() {
    let (_fixture, root, home) = checkout("predecessor-same");
    let policy = rendered_policy(&root);
    // The render digest the record published is this plan's own render, which is
    // what a re-up of an unchanged generation looks like.
    let probe_render = effigy_containers::with_test_effigy_home(&home, || {
        plan_for(&policy, &mut FakeBackend::colima())
            .expect("plan")
            .expect("container mode")
            .render_digest()
            .to_owned()
    });
    let mut backend = FakeBackend::colima().with_predecessor(PredecessorState {
        running: true,
        desired: Some(probe_render),
    });

    let planned =
        effigy_containers::with_test_effigy_home(&home, || plan_for(&policy, &mut backend));

    assert!(planned.expect("plan").is_some());
}

#[test]
fn a_live_predecessor_on_another_render_is_refused_before_apply() {
    let (_fixture, root, home) = checkout("predecessor-differs");
    let policy = rendered_policy(&root);
    let mut backend = FakeBackend::colima().with_predecessor(PredecessorState {
        running: true,
        desired: Some(Digest::of(b"another render").hex()),
    });

    let error = effigy_containers::with_test_effigy_home(&home, || {
        plan_for(&policy, &mut backend).expect_err("a live predecessor serves another render")
    })
    .to_string();

    assert!(error.contains("live workspace predecessor"), "{error}");
    assert!(error.contains("container web down"), "{error}");
}

#[test]
fn a_live_predecessor_with_no_recorded_render_is_refused() {
    let (_fixture, root, home) = checkout("predecessor-unrecorded");
    let policy = rendered_policy(&root);
    let mut backend = FakeBackend::colima().with_predecessor(PredecessorState {
        running: true,
        desired: None,
    });

    let error = effigy_containers::with_test_effigy_home(&home, || {
        plan_for(&policy, &mut backend).expect_err("nothing proves the predecessor's render")
    })
    .to_string();

    assert!(error.contains("recorded `none`"), "{error}");
}

#[test]
fn an_unknown_predecessor_refuses() {
    let (_fixture, root, home) = checkout("predecessor-unknown");
    let policy = rendered_policy(&root);
    let mut backend = FakeBackend::colima();
    backend.predecessor = Observed::Unknown(UnknownReason::Injected);

    let error = effigy_containers::with_test_effigy_home(&home, || {
        plan_for(&policy, &mut backend).expect_err("unknown predecessor")
    })
    .to_string();

    assert!(error.contains("predecessor is unknown"), "{error}");
}

// ------------------------------------------------------- post-apply observation

struct Observation {
    _fixture: Fixture,
    root: PathBuf,
    home: PathBuf,
}

fn observation_checkout(label: &str) -> Observation {
    let (fixture, root, home) = checkout(label);
    Observation {
        _fixture: fixture,
        root,
        home,
    }
}

/// Plan for one observation checkout. The caller owns the policy, so the plan's
/// borrow of it can outlive this call.
fn planned_for(
    policy: &EffectiveContainerPolicy,
    backend: &mut FakeBackend,
) -> ReservedPublicationPlan {
    plan_for(policy, backend)
        .expect("plan")
        .expect("container mode")
}

/// Plan under the fixture's own Effigy home, returning the plan itself.
fn planned_in_home(
    observation: &Observation,
    policy: &EffectiveContainerPolicy,
    backend: &mut FakeBackend,
) -> ReservedPublicationPlan {
    effigy_containers::with_test_effigy_home(&observation.home, || planned_for(policy, backend))
}

#[test]
fn a_guest_that_names_another_container_is_refused_as_a_raced_binding() {
    let observation = observation_checkout("raced");
    let mut backend = FakeBackend::colima();
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let rendered = plan.rendered_policy();
    let probe = probe_factory();
    backend.identity = Observed::Verified(
        WorkspaceContainerIdentity {
            container_id: OTHER_CONTAINER_ID.to_owned(),
            vm_boot_id: OTHER_BOOT_ID.to_owned(),
            ..host_identity(host_machine(), "gnu")
        },
        evidence(),
    );
    let facts = fresh_decision(&plan);

    let error = observe(
        ReservedObservationRequest {
            repo_root: &observation.root,
        },
        &plan,
        &rendered,
        &probe,
        &facts,
        &mut backend,
    )
    .expect_err("raced binding")
    .to_string();

    assert!(error.contains("another container"), "{error}");
}

#[test]
fn a_target_the_decision_did_not_freeze_is_refused() {
    let observation = observation_checkout("stale");
    let mut backend = FakeBackend::colima();
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let rendered = plan.rendered_policy();
    let probe = probe_factory();
    let mut inputs = plan
        .publish_inputs(
            ArtifactEvidence::Unselected {
                image_reference: "example:one".into(),
                backend: plan.backend_label().to_owned(),
            },
            TargetEvidence::Absent {
                project: plan.policy().project_name.clone(),
                backend: plan.backend_label().to_owned(),
            },
        )
        .expect("decision inputs");
    inputs.target = TargetEvidence::Observed(ContainerIdentity {
        container_id: CONTAINER_ID.to_owned(),
        vm_boot_id: OTHER_BOOT_ID.to_owned(),
        backend: "colima-nerdctl".into(),
    });
    let facts = decision(inputs);

    let error = observe(
        ReservedObservationRequest {
            repo_root: &observation.root,
        },
        &plan,
        &rendered,
        &probe,
        &facts,
        &mut backend,
    )
    .expect_err("stale decision evidence")
    .to_string();

    assert!(error.contains("the identity the decision froze"), "{error}");
}

#[test]
fn an_unknown_image_platform_refuses() {
    let observation = observation_checkout("image-unknown");
    let mut backend = FakeBackend::colima();
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let rendered = plan.rendered_policy();
    let probe = probe_factory();
    backend.image = Observed::Unknown(UnknownReason::Inconsistent(
        "image inspect is unavailable".into(),
    ));
    let facts = fresh_decision(&plan);

    let error = observe(
        ReservedObservationRequest {
            repo_root: &observation.root,
        },
        &plan,
        &rendered,
        &probe,
        &facts,
        &mut backend,
    )
    .expect_err("unknown image platform")
    .to_string();

    assert!(error.contains("image platform is unknown"), "{error}");
}

#[test]
fn a_guest_that_disagrees_with_its_image_refuses() {
    let observation = observation_checkout("image-conflict");
    let mut backend = FakeBackend::colima();
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let rendered = plan.rendered_policy();
    let probe = probe_factory();
    backend.identity = Observed::Verified(host_identity(other_arch_machine(), "gnu"), evidence());
    let facts = fresh_decision(&plan);

    let error = observe(
        ReservedObservationRequest {
            repo_root: &observation.root,
        },
        &plan,
        &rendered,
        &probe,
        &facts,
        &mut backend,
    )
    .expect_err("conflicting observations")
    .to_string();

    assert!(error.contains("while its image reports"), "{error}");
}

#[test]
fn emulated_execution_is_refused_rather_than_claimed() {
    let observation = observation_checkout("emulated");
    let mut backend = FakeBackend::colima();
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let rendered = plan.rendered_policy();
    let probe = probe_factory();
    // A consistent emulated world: the backend says the image is for another
    // machine type and the guest agrees with it.
    backend.image = Observed::Verified(
        ObservedImagePlatform {
            architecture: other_arch().into(),
            ..image_facts()
        },
        evidence(),
    );
    backend.identity = Observed::Verified(host_identity(other_arch_machine(), "gnu"), evidence());
    let facts = fresh_decision(&plan);

    let error = observe(
        ReservedObservationRequest {
            repo_root: &observation.root,
        },
        &plan,
        &rendered,
        &probe,
        &facts,
        &mut backend,
    )
    .expect_err("unverified emulation")
    .to_string();

    assert!(error.contains("emulated execution"), "{error}");
}

fn other_arch_machine() -> &'static str {
    match other_arch() {
        "amd64" => "x86_64",
        _ => "aarch64",
    }
}

#[test]
fn an_image_without_a_provable_c_library_refuses() {
    let observation = observation_checkout("libc");
    let mut backend = FakeBackend::colima();
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let rendered = plan.rendered_policy();
    let probe = probe_factory();
    backend.identity = Observed::Verified(host_identity(host_machine(), ""), evidence());
    let facts = fresh_decision(&plan);

    let error = observe(
        ReservedObservationRequest {
            repo_root: &observation.root,
        },
        &plan,
        &rendered,
        &probe,
        &facts,
        &mut backend,
    )
    .expect_err("no provable libc")
    .to_string();

    assert!(error.contains("no provable C library"), "{error}");
}

#[test]
fn a_tampered_artifact_that_disagrees_with_its_receipt_refuses() {
    let observation = observation_checkout("artifact-receipt");
    let artifact = produced_artifact(&observation.root, host_family());
    std::fs::write(&artifact, b"swapped bytes").expect("swapped artifact");
    let mut backend = FakeBackend::colima();
    backend.artifact = Observed::Verified(artifact, evidence());
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let rendered = plan.rendered_policy();
    let probe = probe_factory();
    let facts = fresh_decision(&plan);

    let error = effigy_containers::with_test_effigy_home(&observation.home, || {
        observe(
            ReservedObservationRequest {
                repo_root: &observation.root,
            },
            &plan,
            &rendered,
            &probe,
            &facts,
            &mut backend,
        )
        .expect_err("unauthenticated artifact")
    })
    .to_string();

    assert!(error.contains("receipt records sha256"), "{error}");
}

#[test]
fn a_fresh_target_is_attributed_to_this_activation_and_carries_the_verified_platform() {
    let observation = observation_checkout("fresh");
    let artifact = produced_artifact(&observation.root, host_family());
    let mut backend = FakeBackend::colima();
    backend.artifact = Observed::Verified(artifact.clone(), evidence());
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let rendered = plan.rendered_policy();
    let probe = probe_factory();
    let facts = fresh_decision(&plan);
    let project = plan.policy().project_name.clone();

    let prepared = effigy_containers::with_test_effigy_home(&observation.home, || {
        observe(
            ReservedObservationRequest {
                repo_root: &observation.root,
            },
            &plan,
            &rendered,
            &probe,
            &facts,
            &mut backend,
        )
        .expect("fresh publication inputs")
    });

    assert_eq!(prepared.verified_container_id(), CONTAINER_ID);
    let input = prepared.into_input();
    assert_eq!(input.artifact, artifact);
    assert_eq!(input.alias.backend, Backend::Colima);
    assert!(
        input.deadline > Instant::now(),
        "the producer gets the one absolute deadline"
    );
    let Observed::Verified(platform, _) = &input.platform else {
        panic!("a built input carries a verified platform");
    };
    assert_eq!(platform.target.container_id, CONTAINER_ID);
    assert_eq!(platform.target.vm_boot_id, BOOT_ID);
    assert_eq!(platform.target.backend, "colima-nerdctl");
    // Both sides come from independent owners: the backend's image inspect and
    // the artifact's own receipt.
    assert_eq!(
        platform.platform,
        ImagePlatform {
            os: "linux".into(),
            architecture: host_arch().into(),
            variant: String::new(),
            loader: LoaderFamily::Gnu,
        },
        "the image platform is the backend's own report plus the observed loader"
    );
    assert_eq!(platform.artifact_platform, host_family().image_platform());
    assert_eq!(
        platform.artifact_version,
        effigy_core::build_info::active_version()
    );
    assert!(
        platform.execution_available,
        "this host runs the image type"
    );
    assert_eq!(
        platform.artifact.path,
        format!("/run/effigy/objects/bin/{}", platform.artifact.digest)
    );
    let Observed::Verified(targets, _) = &input.targets else {
        panic!("a built input carries verified target candidates");
    };
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].identity.container_id, CONTAINER_ID);
    assert_eq!(targets[0].project, project);
    // A fresh project's container is created by this activation.
    assert_eq!(targets[0].created_by, Some(facts.created_by));
    assert_eq!(
        input.context.activation.container.container_id,
        CONTAINER_ID
    );
    assert_eq!(
        input.context.activation.environment.generation,
        facts.reservation_generation.to_string()
    );
    assert_eq!(input.context.backend.profile, "effigy");
    assert!(
        input.snapshots.is_empty()
            || input.snapshots.iter().all(|(object, bytes)| {
                object.digest == Digest::of(bytes)
                    && object.path.starts_with("/run/effigy/snapshots/")
            })
    );
}

#[test]
fn a_reobserved_target_is_not_attributed_to_this_activation() {
    let observation = observation_checkout("reobserved");
    let artifact = produced_artifact(&observation.root, host_family());
    let mut backend = FakeBackend::colima();
    backend.artifact = Observed::Verified(artifact, evidence());
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let rendered = plan.rendered_policy();
    let probe = probe_factory();
    let facts = reobserved_decision(
        &plan,
        ContainerIdentity {
            container_id: CONTAINER_ID.to_owned(),
            vm_boot_id: BOOT_ID.to_owned(),
            backend: "colima-nerdctl".into(),
        },
    );

    let prepared = effigy_containers::with_test_effigy_home(&observation.home, || {
        observe(
            ReservedObservationRequest {
                repo_root: &observation.root,
            },
            &plan,
            &rendered,
            &probe,
            &facts,
            &mut backend,
        )
        .expect("reobserved publication inputs")
    });
    let input = prepared.into_input();
    let Observed::Verified(targets, _) = &input.targets else {
        panic!("a built input carries verified target candidates");
    };
    assert_eq!(
        targets[0].created_by, None,
        "a container this activation did not create is never attributed to it"
    );
}

// ------------------------------------------------------ production adapters

#[test]
fn the_production_selection_observation_reads_an_absent_record_as_a_policy_choice() {
    let (_fixture, root, _home) = checkout("observed-backend");
    let policy = rendered_policy(&root);
    // No environment record exists yet: the policy's own source is the verified
    // selection, and it is an observation rather than a `None` collapse.
    let observed = crate::runner::environment::observed_backend(
        &root,
        &policy,
        Instant::now() + Duration::from_secs(10),
    );
    let Observed::Verified(selected, _) = observed else {
        panic!("a policy selection is a verified observation, not a default");
    };
    assert!(!selected.context.trim().is_empty());
}

// ------------------------------------------------------------ the one secret

#[test]
fn the_enrolment_secret_is_64_canonical_hex_and_fresh_every_time() {
    let first = ReservedEnrolmentSecret::mint().expect("entropy");
    let second = ReservedEnrolmentSecret::mint().expect("entropy");
    for secret in [&first, &second] {
        let text = secret.expose();
        assert_eq!(text.len(), 64, "{text}");
        assert!(Digest::parse_hex(&text).is_some(), "{text}");
    }
    assert_ne!(first.expose(), second.expose());
    assert!(ReservedEnrolmentSecret::canonical(&"A".repeat(64)).is_none());
}

#[test]
fn an_observation_that_outlives_the_budget_is_a_timeout_not_a_panic() {
    let (_fixture, root, home) = checkout("expiry-in-flight");
    let policy = rendered_policy(&root);
    let mut backend = FakeBackend::colima();
    // The addressing observation outlives the budget. In a debug build a panic
    // here would abort the publication instead of leaving it pending/unknown.
    // The budget survives the first observation's start and not its end.
    backend.takes = Some(Duration::from_millis(150));
    let expired = PublicationDeadline::new(Instant::now() + Duration::from_millis(100));

    let error = effigy_containers::with_test_effigy_home(&home, || {
        plan(
            ReservedPlanRequest {
                policy: &policy,
                deadline: expired,
            },
            &mut backend,
        )
        .expect_err("an expired observation is not a fact")
    })
    .to_string();

    // Either the adapter noticed the elapsed budget and reported a timeout, or
    // the planner refused the expired clock right after the read. Both refuse;
    // neither panics nor publishes.
    assert!(
        error.contains("deadline expired") || error.contains("timed out"),
        "{error}"
    );
    assert_eq!(
        backend.count(),
        1,
        "planning stopped as soon as an observation outlived the budget"
    );
    assert!(
        matches!(
            backend.artifact,
            Observed::Verified(ref path, _) if path == Path::new("/nonexistent/artifact")
        ),
        "no artifact was observed"
    );
}

#[test]
fn a_same_engine_but_different_profile_binding_is_refused() {
    let observation = observation_checkout("profile-race");
    let mut backend = FakeBackend::colima();
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let rendered = plan.rendered_policy();
    let probe = probe_factory();
    // The same engine, another Colima profile: a target bound to it would have
    // published this plan's profile and alias.
    backend.binding = Observed::Verified(
        VerifiedBinding {
            container_id: CONTAINER_ID.to_owned(),
            selected: colima_selection("another-profile"),
        },
        evidence(),
    );
    let facts = fresh_decision(&plan);

    let error = observe(
        ReservedObservationRequest {
            repo_root: &observation.root,
        },
        &plan,
        &rendered,
        &probe,
        &facts,
        &mut backend,
    )
    .expect_err("another profile is another backend")
    .to_string();

    assert!(error.contains("another-profile"), "{error}");
    assert!(error.contains("effigy"), "{error}");
}

#[test]
fn a_policy_that_is_not_the_plans_render_is_refused() {
    let observation = observation_checkout("foreign-render");
    let mut backend = FakeBackend::colima();
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let probe = probe_factory();
    let facts = fresh_decision(&plan);

    let error = observe(
        ReservedObservationRequest {
            repo_root: &observation.root,
        },
        &plan,
        // The unrendered policy: no `effigy-host` alias, so not the plan's render.
        &policy,
        &probe,
        &facts,
        &mut backend,
    )
    .expect_err("another render was handed over")
    .to_string();

    assert!(
        error.contains("another render than the planned one"),
        "{error}"
    );
}

/// The integration shape #452 writes, type-checked against `up_prepared`'s own
/// callback parameter: one begin/apply, then the callback returns an input that
/// owns its probe, so nothing borrows a local that dies with the callback.
#[test]
fn the_published_integration_shape_compiles_and_owns_its_probe() {
    /// Exactly `up_prepared`'s third parameter.
    fn accepts_published<'a>(
        published: impl FnOnce(
            &mut effigy_containers::environment::WorkspaceActivation<'static>,
        ) -> Result<ReservedGuestInput<'a>, EnvError>,
    ) {
        // The compiler, not a run, proves the shape: a real activation needs the
        // kernel journal, and the producer's own owner tests drive that.
        let _ = published;
    }

    let observation = observation_checkout("integration-shape");
    let artifact = produced_artifact(&observation.root, host_family());
    let mut backend = FakeBackend::colima();
    backend.artifact = Observed::Verified(artifact.clone(), evidence());
    // Declared before the plan so the closure below can borrow it for the
    // publication's lifetime.
    let mut second = FakeBackend::colima();
    second.artifact = Observed::Verified(artifact, evidence());
    let policy = rendered_policy(&observation.root);
    let plan = planned_in_home(&observation, &policy, &mut backend);
    let rendered = plan.rendered_policy();
    let probe = probe_factory();
    let facts = fresh_decision(&plan);

    // The same shape built for real, so the ownership claim is exercised: the
    // input's probe is the one the injected factory made for this target, and
    // `into_input()` takes no borrow.
    let input = effigy_containers::with_test_effigy_home(&observation.home, || {
        observe(
            ReservedObservationRequest {
                repo_root: &observation.root,
            },
            &plan,
            &rendered,
            &probe,
            &facts,
            &mut backend,
        )
        .expect("the observation builds the owner input")
        .into_input()
    });
    assert_eq!(input.alias.backend, Backend::Colima);
    assert!(input.deadline > Instant::now());

    // And the same closure, type-checked against `up_prepared`'s parameter.
    let second_facts = facts;
    accepts_published(|_activation| {
        Ok(observe(
            ReservedObservationRequest {
                repo_root: &observation.root,
            },
            &plan,
            &rendered,
            &probe,
            &second_facts,
            &mut second,
        )?
        .into_input())
    });
}

// --------------------------------------------- the bounded source closure

/// A checkout whose manifest includes many fragments: composing its closure has
/// to read and pin every one of them.
fn fragment_checkout(label: &str, fragments: usize) -> (Fixture, PathBuf, PathBuf) {
    let fixture = Fixture::new(&format!("effigy-reserved-input-{label}-")).expect("fixture");
    let root = fixture.path().join("checkout");
    let home = fixture.path().join("home/.effigy");
    std::fs::create_dir_all(root.join("fragments")).expect("checkout");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::create_dir_all(root.join(".git")).expect("git directory");
    let mut includes = Vec::new();
    for index in 0..fragments {
        let relative = format!("fragments/{index:04}.toml");
        std::fs::write(
            root.join(&relative),
            format!(
                "[tasks.fragment-{index}]\nrun = \"true\"\ndescription = \"fragment {index} that the closure must read and pin\"\n"
            ),
        )
        .expect("fragment");
        includes.push(format!("\"{relative}\""));
    }
    std::fs::write(
        root.join("effigy.toml"),
        format!(
            "[manifest]\ninclude = [{}]\n\n[containers]\ndefault = \"web\"\n\n[containers.web]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace\"\n\n[systems]\ndefault = \"dev\"\n\n[systems.dev]\ndefault_workspace = \"app\"\n\n[systems.dev.workspaces.app]\ncontainer = \"web\"\nworking_dir = \"/workspace\"\n",
            includes.join(", ")
        ),
    )
    .expect("manifest");
    std::fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n",
    )
    .expect("compose file");
    (fixture, root, home)
}

/// The closure-shaped bounded read: an expired budget refuses without reading,
/// a real budget verifies, and the checkout is reusable either way.
fn bounded_closure(
    root: &Path,
    paths: &crate::runner::workspace_client::context::WorkspacePathMap,
    until: Instant,
) -> Result<
    crate::runner::workspace_command::provisioning::WorkspaceSourceClosure,
    crate::runner::workspace_command::provisioning::BoundedReadRefusal,
> {
    let owned_root = root.to_path_buf();
    let owned_paths = paths.clone();
    crate::runner::workspace_command::provisioning::bounded_publication_read(
        "test-source-closure",
        until,
        move || {
            crate::runner::workspace_command::provisioning::workspace_source_closure(
                &owned_root,
                &owned_paths,
            )
        },
    )
}

#[test]
fn the_source_closure_is_bounded_by_the_publication_deadline() {
    let (_fixture, root, _home) = fragment_checkout("closure-bound", 8);
    let policy = rendered_policy(&root);
    let paths = crate::runner::workspace_command::provisioning::workspace_path_map(
        &root,
        &crate::runner::workspace_command::provisioning::operator_workspace_for_policy(
            &root, &policy,
        )
        .expect("operator workspace")
        .expect("container mode"),
    )
    .expect("path map");

    // A deadline that has already passed must not wait for the worker at all.
    assert!(
        matches!(
            bounded_closure(&root, &paths, Instant::now() - Duration::from_secs(1)),
            Err(crate::runner::workspace_command::provisioning::BoundedReadRefusal::TimedOut)
        ),
        "an expired publication refuses instead of reading the checkout"
    );

    // The abandoned worker is read-only: the checkout is unchanged and the next
    // attempt with a real budget closes its pins normally.
    let closed = bounded_closure(&root, &paths, Instant::now() + Duration::from_secs(30))
        .expect("a bounded closure with budget left");
    assert!(
        closed
            .pins
            .iter()
            .any(|pin| pin.kind == COMPOSED_MANIFEST_KIND),
        "the composed manifest is pinned"
    );
    assert!(
        closed
            .pins
            .iter()
            .any(|pin| pin.name.contains("fragments/0000.toml")),
        "every composed fragment is pinned: {:?}",
        closed
            .pins
            .iter()
            .map(|pin| pin.name.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_stalled_fragment_stops_the_closure_at_the_deadline() {
    let (_fixture, root, _home) = fragment_checkout("closure-stalled", 2);
    let policy = rendered_policy(&root);
    let paths = crate::runner::workspace_command::provisioning::workspace_path_map(
        &root,
        &crate::runner::workspace_command::provisioning::operator_workspace_for_policy(
            &root, &policy,
        )
        .expect("operator workspace")
        .expect("container mode"),
    )
    .expect("path map");
    // One included fragment is a FIFO: reading it blocks until a writer appears,
    // which is the stalled read a clock check cannot interrupt.
    let stalled_fragment = root.join("fragments/0001.toml");
    std::fs::remove_file(&stalled_fragment).expect("remove the fragment");
    stalled(&stalled_fragment);

    let started = Instant::now();
    let refusal = bounded_closure(&root, &paths, started + Duration::from_millis(50));

    assert!(
        matches!(
            refusal,
            Err(crate::runner::workspace_command::provisioning::BoundedReadRefusal::TimedOut)
        ),
        "a stalled fragment is a timeout, not a blocked publication"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the closure stopped at its deadline"
    );
}

#[test]
fn a_publication_never_plans_past_its_deadline_composing_pins() {
    let (_fixture, root, home) = fragment_checkout("closure-expiry", 400);
    let policy = rendered_policy(&root);
    let mut backend = FakeBackend::colima();
    // One millisecond cannot cover composing 400 fragments and the rest of
    // planning, so the publication must refuse rather than publish anyway.
    let expired = PublicationDeadline::new(Instant::now() + Duration::from_millis(1));

    let refusal = effigy_containers::with_test_effigy_home(&home, || {
        plan(
            ReservedPlanRequest {
                policy: &policy,
                deadline: expired,
            },
            &mut backend,
        )
    });

    let error = match refusal {
        Ok(Some(_)) => panic!("a publication past its deadline must not plan"),
        Ok(None) => panic!("this checkout declares an operator workspace"),
        Err(error) => error.to_string(),
    };
    assert!(
        error.contains("deadline expired") || error.contains("source closure is unknown"),
        "{error}"
    );
}

// ------------------------------------ blocked reads cannot outlive the budget

/// A FIFO at `path`: any reader of it blocks until a writer appears, which is the
/// stalled-filesystem case a clock check cannot interrupt.
fn stalled(path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).expect("path");
    let created = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
    assert_eq!(created, 0, "mkfifo {} failed", path.display());
}

#[test]
fn a_stalled_artifact_sidecar_refuses_at_the_deadline() {
    let observation = observation_checkout("stalled-sidecar");
    let artifact = produced_artifact(&observation.root, host_family());
    // The receipt sidecar of a produced artifact can be a stalled read.
    let receipt = workspace_artifact_receipt_path(&artifact);
    std::fs::remove_file(&receipt).expect("remove the receipt");
    stalled(&receipt);

    let started = Instant::now();
    let refusal = crate::runner::workspace_command::provisioning::verify_guest_artifact_bounded(
        artifact.clone(),
        host_family(),
        started + Duration::from_millis(50),
    );
    let refused = match refusal {
        Err(crate::runner::workspace_command::provisioning::BoundedReadRefusal::TimedOut) => true,
        Err(crate::runner::workspace_command::provisioning::BoundedReadRefusal::Unavailable(
            reason,
        )) => panic!("a stalled receipt is a timeout, not an error: {reason}"),
        Ok(_) => false,
    };

    assert!(refused, "a stalled receipt cannot produce an identity");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the publication stopped at its deadline instead of blocking"
    );

    // The read was abandoned, not committed: a real artifact still authenticates.
    std::fs::remove_file(&receipt).expect("remove the fifo");
    let healthy = produced_artifact(&observation.root, host_family());
    crate::runner::workspace_command::provisioning::verify_guest_artifact_bounded(
        healthy,
        host_family(),
        Instant::now() + Duration::from_secs(30),
    )
    .expect("a healthy artifact still authenticates after the timeout");
}

#[test]
fn a_stalled_manifest_refuses_at_the_deadline_before_planning() {
    let (_fixture, root, home) = checkout("stalled-manifest");
    let policy = rendered_policy(&root);
    // The operator-workspace read loads the manifest; a stalled manifest must not
    // hold planning past the publication's one deadline.
    std::fs::remove_file(root.join("effigy.toml")).expect("remove the manifest");
    stalled(&root.join("effigy.toml"));

    let mut backend = FakeBackend::colima();
    let started = Instant::now();
    let refusal = effigy_containers::with_test_effigy_home(&home, || {
        plan(
            ReservedPlanRequest {
                policy: &policy,
                deadline: PublicationDeadline::new(started + Duration::from_millis(50)),
            },
            &mut backend,
        )
    });
    let error = match refusal {
        Ok(_) => panic!("a stalled manifest must not produce a plan"),
        Err(error) => error.to_string(),
    };

    assert!(
        error.contains("deadline expired") || error.contains("operator workspace is unknown"),
        "{error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "planning stopped at its deadline instead of blocking on the manifest"
    );
    assert_eq!(backend.count(), 0, "no backend fact was read");
}

#[test]
fn a_stalled_colima_profile_refuses_at_the_deadline() {
    let (_fixture, root, home) = checkout("stalled-profile");
    let policy = rendered_policy(&root);
    // A profile file that blocks on read is the case a clock check cannot
    // interrupt; the production addressing observation must give up at the
    // publication's deadline instead of holding planning.
    let colima_home = home.join("colima");
    std::fs::create_dir_all(colima_home.join("effigy")).expect("colima profile directory");
    stalled(&colima_home.join("effigy/colima.yaml"));
    let _env = crate::contract_test_support::EnvGuard::set_many(&[(
        "COLIMA_HOME",
        Some(colima_home.display().to_string()),
    )]);
    let mut observation = ContainerBackendObservation;

    let started = Instant::now();
    let observed = observation.addressing(
        &policy,
        &colima_selection("effigy"),
        PublicationDeadline::new(started + Duration::from_millis(50)),
    );

    match observed {
        Observed::Unknown(_) => {}
        other => panic!("a stalled profile cannot be a verified address: {other:?}"),
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the profile observation stopped at its deadline"
    );
}

#[test]
fn an_expired_budget_reads_no_environment_record() {
    let (_fixture, root, _home) = checkout("expired-record");
    let policy = rendered_policy(&root);

    let observed =
        crate::runner::environment::record_until(&policy, Instant::now() - Duration::from_secs(1));

    assert!(
        matches!(observed, Observed::Unknown(UnknownReason::TimedOut { .. })),
        "an expired budget is a timeout, not a filesystem read"
    );
    // The identity token a genuine read would establish is absent: the expired
    // budget refused before any filesystem work.
    assert!(
        !root
            .join(".git")
            .join(effigy_core::worktree_scope::CHECKOUT_TOKEN_FILE)
            .exists(),
        "an expired read establishes no checkout identity"
    );
}
