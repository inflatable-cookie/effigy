//! Verified inputs for the reserved workspace publication (052 §8.11).
//!
//! [`prepare_reserved_guest`](crate::runner::workspace_command::provisioning::reserved_guest::prepare_reserved_guest)
//! publishes one guest session from a `ReservedGuestInput`. Every field of that
//! input has an owner and an observation, and none of it may be filled in by a
//! held selector, a default or a test value. This module is the only place that
//! turns those observations into the producer's input: it derives, checks and
//! refuses, and it never applies Compose, allocates a generation or switches a
//! selector.
//!
//! The facts are not all knowable at one moment, so the work is split at the
//! ordinary gated apply:
//!
//! * [`plan`] runs before `WorkspaceActivation::begin` freezes the decision and
//!   before the apply. It observes the admitted backend and the addresses that
//!   backend's guest can actually reach, refuses to re-render over a live
//!   predecessor it cannot prove compatible, renders the `effigy-host` alias into
//!   the *same* policy the apply is handed, and freezes the environment
//!   materialisation (`GuestContextInputs`) the decision and the published
//!   context share. Its [`ReservedPublicationPlan`] is the one owner of those
//!   facts for the rest of the activation.
//! * [`observe`] runs after the apply, on the admitted activation. It re-verifies
//!   the one exact container, reads the image platform from the backend's own
//!   image inspect and the loader family from the image root, authenticates the
//!   produced artifact for that platform against its producer receipt, and
//!   returns the producer's input plus the single enrolment secret it may stage.
//!
//! Both halves read the same absolute [`PublicationDeadline`], which the producer
//! also receives: planning, every backend read, every guest command and every
//! probe measure the same clock, so a delay between observation and publication
//! cannot extend any of them.
//!
//! **Process boundary.** The plan, the observed input and the guest all stay in
//! one process. The durable owner that #507 launches before the activation builds
//! the plan, freezes the decision and applies Compose once with
//! `plan.policy()`, then observes and hands `into_input` its producer in that same
//! process; nothing here is serializable state and no live handle crosses a
//! process boundary. The deadline is created by that owner, never inherited.
//!
//! Refused here, before any selection switch: an unknown or absent backend
//! selection, an alias address no observation supplied, a live predecessor this
//! render would recreate, a container that is not the one the decision froze, an
//! unknown or unsupported image platform, a guest whose own machine type
//! disagrees with the image, an artifact whose producer receipt names another
//! family, a hybrid environment, and any image this host cannot execute natively
//! (emulation has no qualified observation owner: see [`native_execution`]).
#![allow(dead_code)] // The workspace-client owner (#452) installs the consuming caller.

use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_containers::environment::{BackendKind, BackendSelection};
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::id::{Digest, RunId};
use effigy_kernel::observe::{Evidence, Observed, UnknownReason};
use effigy_kernel::run::RunError;
use effigy_kernel::workspace_session::{
    ArtifactEvidence, BoundPlatform, ContainerIdentity, GuestContextInputs, GuestObject,
    ImagePlatform, LoaderFamily, PublishInputs, TargetCandidate, TargetEvidence,
};

use crate::runner::container_runtime_prep::{observed_image_platform, ObservedImagePlatform};
use crate::runner::error::RunnerError;
use crate::runner::workspace_client::context::{
    snapshot_path, GuestWorkspace, WorkspaceActivationIdentity, WorkspaceBackendDescriptor,
    WorkspaceChannelLocator, WorkspaceContainerIdentity, WorkspaceContextInput,
    WorkspaceEnrolmentCredential, WorkspaceEnvironmentIdentity, WorkspaceHostIdentity,
    WorkspacePathMap, CHANNEL_PROTOCOL_VERSION,
};
use crate::runner::workspace_command::provisioning::reserved_guest::{
    ReservedGuestInput, WorkspaceGuestProbe,
};
use crate::runner::workspace_command::provisioning::staging::DeadlineGuest;
use crate::runner::workspace_command::provisioning::{
    held_linux_workspace_effigy_artifact, host_user, observed_workspace_container_identity,
    operator_workspace_for_policy, verify_guest_artifact_until, workspace_path_map,
    workspace_routes, workspace_source_closure, LinuxWorkspaceTarget, WorkspaceSourceClosure,
};
use crate::runner::workspace_transport::qualification::{Backend, ChannelAliasPlan};

type EnvError = effigy_containers::environment::EnvError;

/// Planning will not begin the checkout's source closure with less budget than
/// this left: the closure reads the checkout and composes the manifest, and a
/// publication that cannot finish it must refuse before it starts.
const SOURCE_CLOSURE_FLOOR: Duration = Duration::from_millis(250);

/// The fixed evidence label every derived value in this module carries. It names
/// this builder, so a receipt reader can tell a builder fact from an
/// owner-issued one.
fn evidence() -> Evidence {
    Evidence::new("reserved publication input builder".into(), None).expect("fixed evidence")
}

fn unavailable(message: impl Into<String>) -> EnvError {
    EnvError::Kernel(RunError::unavailable(message))
}
fn refused(message: impl Into<String>) -> EnvError {
    EnvError::Kernel(RunError::refused(message))
}
fn io(error: RunnerError) -> EnvError {
    unavailable(error.to_string())
}

/// The one absolute deadline of a publication, and the remaining budget each
/// read must be given. An expired budget refuses before the read, never after.
#[derive(Clone, Copy, Debug)]
pub(in crate::runner) struct PublicationDeadline {
    until: Instant,
}

impl PublicationDeadline {
    pub(in crate::runner) fn new(until: Instant) -> Self {
        Self { until }
    }

    pub(in crate::runner) fn remaining(&self) -> Result<Duration, EnvError> {
        self.until
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| unavailable("workspace publication deadline expired"))
    }

    pub(in crate::runner) fn at(&self) -> Instant {
        self.until
    }
}

/// Addresses the selected backend observed for the workspace alias. `proxy_bind`
/// is the host address its guest proxy reaches (absent for a directly bound
/// backend); `colima_gateway` is the profile's own configured guest-side gateway.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runner) struct ReservedAddressing {
    pub(in crate::runner) proxy_bind: Option<IpAddr>,
    pub(in crate::runner) colima_gateway: Option<IpAddr>,
}

/// The host-verified identity of the environment's primary container: its full
/// backend id and the *complete* backend selection (kind, profile/context and
/// engine) that was admitted when it was bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::runner) struct VerifiedBinding {
    pub(in crate::runner) container_id: String,
    pub(in crate::runner) selected: BackendSelection,
}

impl VerifiedBinding {
    /// The engine label a published target and its evidence carry.
    pub(in crate::runner) fn backend_label(&self) -> &str {
        self.selected.engine.as_str()
    }
}

/// Whether a live predecessor exists, and what proves it compatible.
///
/// `desired` is the render digest this checkout's environment record published
/// for the running stack. A render the record never published is not a
/// compatible predecessor: applying it can recreate the container a live session
/// is serving.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::runner) struct PredecessorState {
    pub(in crate::runner) running: bool,
    pub(in crate::runner) desired: Option<String>,
}

/// The verified backend and provisioning observations a reserved publication
/// needs.
///
/// The production implementation is [`ContainerBackendObservation`]. Tests
/// implement this trait to script `Observed` answers; no product path can be
/// built from those.
pub(in crate::runner) trait ReservedBackendObservation {
    /// The engine family, profile and why it was selected. An unreadable record
    /// stays `Unknown`; it never falls back to a policy default.
    fn selection(
        &mut self,
        policy: &EffectiveContainerPolicy,
        deadline: PublicationDeadline,
    ) -> Observed<BackendSelection>;
    /// The alias addresses that selection observed. A backend whose addresses
    /// are not observed yields `Unknown`, never a default.
    fn addressing(
        &mut self,
        policy: &EffectiveContainerPolicy,
        selected: &BackendSelection,
        deadline: PublicationDeadline,
    ) -> Observed<ReservedAddressing>;
    /// Whether a container of this project is already running, and the render
    /// digest this checkout's record published for it.
    fn predecessor(
        &mut self,
        policy: &EffectiveContainerPolicy,
        deadline: PublicationDeadline,
    ) -> Observed<PredecessorState>;
    /// The host-verified full container id of the primary service.
    fn binding(
        &mut self,
        policy: &EffectiveContainerPolicy,
        deadline: PublicationDeadline,
    ) -> Observed<VerifiedBinding>;
    /// The guest's own boot, machine type and C library in that exact container.
    fn identity(
        &mut self,
        policy: &EffectiveContainerPolicy,
        target_id: &str,
        deadline: PublicationDeadline,
    ) -> Observed<WorkspaceContainerIdentity>;
    /// The OS, architecture and variant the backend reports for the exact image
    /// that container runs.
    fn image_platform(
        &mut self,
        policy: &EffectiveContainerPolicy,
        container_id: &str,
        deadline: PublicationDeadline,
    ) -> Observed<ObservedImagePlatform>;
    /// The produced guest artifact this host already holds for one family,
    /// authenticated later by its producer receipt.
    fn artifact(
        &mut self,
        policy: &EffectiveContainerPolicy,
        target: LinuxWorkspaceTarget,
        deadline: PublicationDeadline,
    ) -> Observed<PathBuf>;
    /// The guest filesystem for that exact container, bounded by the publication
    /// deadline and one cumulative capture budget.
    fn guest<'a>(
        &'a mut self,
        policy: &'a EffectiveContainerPolicy,
        target: &ContainerIdentity,
        deadline: PublicationDeadline,
    ) -> Result<Box<dyn GuestWorkspace + 'a>, EnvError>;
}

/// The production observation: the environment record's backend selection, the
/// Compose engine's own binding, the guest's `/proc` and loader facts, the
/// backend's image inspect, and the host's produced artifact.
#[derive(Debug, Default)]
pub(in crate::runner) struct ContainerBackendObservation;

impl ReservedBackendObservation for ContainerBackendObservation {
    fn selection(
        &mut self,
        policy: &EffectiveContainerPolicy,
        deadline: PublicationDeadline,
    ) -> Observed<BackendSelection> {
        // The record's own answer, with an unreadable record or recorded backend
        // kept `Unknown` instead of collapsing into the policy default.
        crate::runner::environment::observed_backend(
            policy.repo_root.as_path(),
            policy,
            deadline.at(),
        )
    }

    fn addressing(
        &mut self,
        _policy: &EffectiveContainerPolicy,
        selected: &BackendSelection,
        deadline: PublicationDeadline,
    ) -> Observed<ReservedAddressing> {
        let mut addressing = ReservedAddressing {
            proxy_bind: None,
            colima_gateway: None,
        };
        if selected.kind != BackendKind::DockerEngine {
            // Colima's Lima forwarder and the Docker Desktop proxy both reach
            // the host on its loopback. The builder proves the address is a
            // local, bindable one instead of assuming a remote host IP, and the
            // guest-side reachability is still proved later by the qualified
            // resolve/dial in `prepare_channel`.
            match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) {
                Ok(listener) => {
                    let address = listener
                        .local_addr()
                        .ok()
                        .map(|address| address.ip())
                        .filter(|ip| !ip.is_unspecified() && !ip.is_multicast());
                    drop(listener);
                    let Some(address) = address else {
                        return Observed::Unknown(UnknownReason::Inconsistent(
                            "the host did not report the proxy bind it just opened".into(),
                        ));
                    };
                    addressing.proxy_bind = Some(address);
                }
                Err(error) => {
                    return Observed::Unknown(UnknownReason::Inconsistent(format!(
                        "the host loopback proxy bind is unavailable: {error}"
                    )))
                }
            }
        }
        if selected.kind == BackendKind::Colima {
            // The profile's own configuration, never the renderer's default: a
            // profile that names no gateway does not prove the address its guest
            // would use.
            match effigy_containers::runtime_gateway_observation(&selected.context) {
                Observed::Verified(Some(configured), _) => {
                    match configured.trim().parse::<IpAddr>() {
                        Ok(gateway) if !gateway.is_unspecified() && !gateway.is_multicast() => {
                            addressing.colima_gateway = Some(gateway)
                        }
                        _ => {
                            return Observed::Unknown(UnknownReason::Inconsistent(format!(
                                "the Colima profile gateway `{configured}` is not a usable address"
                            )))
                        }
                    }
                }
                Observed::Verified(None, _) => {
                    return Observed::Unknown(UnknownReason::Inconsistent(
                        "the Colima profile configures no gateway address".into(),
                    ))
                }
                Observed::Absent(_) => {
                    return Observed::Unknown(UnknownReason::Inconsistent(
                        "the Colima profile has written no configuration, so its gateway is \
                         unobserved"
                            .into(),
                    ))
                }
                Observed::Unknown(reason) => return Observed::Unknown(reason),
            }
        }
        // An observation that finished past the budget is a timeout, not a fact:
        // the caller turns this into the pending/unknown path, never a panic.
        if deadline.remaining().is_err() {
            return Observed::Unknown(UnknownReason::TimedOut {
                phase: "workspace alias addressing".into(),
            });
        }
        Observed::Verified(addressing, evidence())
    }

    fn predecessor(
        &mut self,
        policy: &EffectiveContainerPolicy,
        deadline: PublicationDeadline,
    ) -> Observed<PredecessorState> {
        // One bounded project listing decides whether anything is running, then
        // the verified binding and the record decide whether it is ours and what
        // render created it.
        let rows = match effigy_containers::exec::list_compose_containers_for_policy_including_stopped_with_deadline(
            policy.repo_root.as_path(),
            policy,
            Some(deadline.at()),
        ) {
            Ok(rows) => rows,
            Err(error) => {
                return Observed::Unknown(UnknownReason::Inconsistent(format!(
                    "the project inventory is unavailable: {error}"
                )))
            }
        };
        let running = rows
            .iter()
            .filter(|row| {
                row.project_name.as_deref() == Some(policy.project_name.as_str())
                    && !row.oneoff
                    && effigy_containers::exec::compose_status_is_running(&row.status)
            })
            .count();
        if running == 0 {
            return Observed::Verified(
                PredecessorState {
                    running: false,
                    desired: None,
                },
                evidence(),
            );
        }
        // A running stack must be this checkout's and scope's, with a full id.
        if let Err(error) = crate::runner::container_runtime_prep::ensure_primary_service_binding(
            policy.repo_root.as_path(),
            policy,
            Some(deadline.at()),
        ) {
            return Observed::Unknown(UnknownReason::Inconsistent(format!(
                "a container of this project is running but its ownership is unproven: {error}"
            )));
        }
        let desired = match crate::runner::environment::record_until(policy, deadline.at()) {
            Observed::Verified(record, _) => record.desired,
            Observed::Absent(_) => None,
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        Observed::Verified(
            PredecessorState {
                running: true,
                desired,
            },
            evidence(),
        )
    }

    fn binding(
        &mut self,
        policy: &EffectiveContainerPolicy,
        deadline: PublicationDeadline,
    ) -> Observed<VerifiedBinding> {
        let bound = crate::runner::container_runtime_prep::ensure_primary_service_binding(
            policy.repo_root.as_path(),
            policy,
            Some(deadline.at()),
        );
        match bound {
            Ok(container) => {
                let selected = crate::runner::environment::observed_backend(
                    policy.repo_root.as_path(),
                    policy,
                    deadline.at(),
                );
                // The whole selection, not just its engine: two Colima profiles
                // or two Docker contexts share an engine while naming different
                // backends, so only the full value fences the planned profile.
                let selected = match selected {
                    Observed::Verified(selected, _) => selected,
                    Observed::Absent(_) => {
                        return Observed::Unknown(UnknownReason::Inconsistent(
                            "the admitted backend is absent".into(),
                        ))
                    }
                    Observed::Unknown(reason) => return Observed::Unknown(reason),
                };
                Observed::Verified(
                    VerifiedBinding {
                        container_id: container.id().to_owned(),
                        selected,
                    },
                    evidence(),
                )
            }
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        }
    }

    fn identity(
        &mut self,
        policy: &EffectiveContainerPolicy,
        target_id: &str,
        deadline: PublicationDeadline,
    ) -> Observed<WorkspaceContainerIdentity> {
        match observed_workspace_container_identity(
            policy.repo_root.as_path(),
            policy,
            target_id,
            deadline.at(),
        ) {
            Ok(identity) => Observed::Verified(identity, evidence()),
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        }
    }

    fn image_platform(
        &mut self,
        policy: &EffectiveContainerPolicy,
        container_id: &str,
        deadline: PublicationDeadline,
    ) -> Observed<ObservedImagePlatform> {
        observed_image_platform(
            policy.repo_root.as_path(),
            policy,
            container_id,
            deadline.at(),
        )
    }

    fn artifact(
        &mut self,
        policy: &EffectiveContainerPolicy,
        target: LinuxWorkspaceTarget,
        deadline: PublicationDeadline,
    ) -> Observed<PathBuf> {
        // A held, current artifact only: the publication's deadline cannot bound
        // a download, so a host that would need one refuses here.
        if deadline.remaining().is_err() {
            return Observed::Unknown(UnknownReason::TimedOut {
                phase: "workspace artifact".into(),
            });
        }
        match held_linux_workspace_effigy_artifact(policy.repo_root.as_path(), target) {
            Ok(artifact) => Observed::Verified(artifact, evidence()),
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        }
    }

    fn guest<'a>(
        &'a mut self,
        policy: &'a EffectiveContainerPolicy,
        target: &ContainerIdentity,
        deadline: PublicationDeadline,
    ) -> Result<Box<dyn GuestWorkspace + 'a>, EnvError> {
        // One absolute deadline and one cumulative capture budget for every
        // staging, snapshot and selector command the producer performs.
        Ok(Box::new(
            DeadlineGuest::new(policy.repo_root.as_path(), policy, target, deadline.at())
                .map_err(io)?,
        ))
    }
}

/// Builds the production probe for the exact target a publication verified.
///
/// The probe's own constructor needs the target (container id and boot), which
/// only exists after the apply. The caller injects this factory instead of a
/// probe value, so the builder can construct the probe from the identity it just
/// verified and hand ownership to the producer: no borrow of a local that dies
/// at the publication closure's boundary.
pub(in crate::runner) type ReservedProbeFactory<'a> =
    Box<dyn Fn(&ContainerIdentity) -> Result<Box<dyn WorkspaceGuestProbe + 'a>, RunnerError> + 'a>;

/// What a caller must hand the pre-decision plan.
pub(in crate::runner) struct ReservedPlanRequest<'a> {
    /// The policy this activation was loaded from. [`plan`] derives the rendered
    /// policy the apply and the decision must share from it.
    pub(in crate::runner) policy: &'a EffectiveContainerPolicy,
    /// One absolute deadline for planning, apply-adjacent observation and every
    /// bounded guest read of this publication.
    pub(in crate::runner) deadline: PublicationDeadline,
}

/// The pre-decision facts one activation freezes before the ordinary apply.
///
/// It owns the rendered policy (the same one the apply and the decision take),
/// the observed alias plan and the environment materialisation. Nothing here is
/// re-derived later: [`observe`] reads this plan.
pub(in crate::runner) struct ReservedPublicationPlan {
    selected: BackendSelection,
    alias: ChannelAliasPlan,
    policy: EffectiveContainerPolicy,
    guest: GuestContextInputs,
    snapshots: Vec<(String, Vec<u8>)>,
    selection: Digest,
    inputs: Digest,
    deadline: PublicationDeadline,
    backend_label: String,
    render_digest: String,
}

impl std::fmt::Debug for ReservedPublicationPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReservedPublicationPlan")
            .field("backend", &self.backend_label)
            .field("profile", &self.selected.context)
            .field("render", &self.render_digest)
            .finish_non_exhaustive()
    }
}

impl ReservedPublicationPlan {
    /// The rendered policy the apply and the decision must share. It carries the
    /// observed `effigy-host` alias, so a guest resolves the address the host
    /// actually bound.
    pub(in crate::runner) fn policy(&self) -> &EffectiveContainerPolicy {
        &self.policy
    }

    /// The same rendered policy, owned by the caller for the rest of the
    /// publication. [`observe`] takes it so the guest the producer stages
    /// through borrows the caller's policy, not this plan.
    pub(in crate::runner) fn rendered_policy(&self) -> EffectiveContainerPolicy {
        self.policy.clone()
    }

    pub(in crate::runner) fn backend(&self) -> &BackendSelection {
        &self.selected
    }

    /// The engine label the published target and artifact evidence carry.
    pub(in crate::runner) fn backend_label(&self) -> &str {
        &self.backend_label
    }

    pub(in crate::runner) fn alias(&self) -> &ChannelAliasPlan {
        &self.alias
    }

    /// The Compose closure digest this plan applies. A live predecessor must
    /// already have published exactly this one.
    pub(in crate::runner) fn render_digest(&self) -> &str {
        &self.render_digest
    }

    pub(in crate::runner) fn deadline(&self) -> PublicationDeadline {
        self.deadline
    }

    /// The environment materialisation the decision freezes and the published
    /// context carries. Both must be this one value.
    pub(in crate::runner) fn guest_inputs(&self) -> &GuestContextInputs {
        &self.guest
    }

    /// The one decision input for this activation. The caller supplies only the
    /// evidence it observed under the decision's gate (the artifact it can name
    /// before apply and the target it re-observed); the guest materialisation and
    /// both digests are this plan's own frozen values.
    pub(in crate::runner) fn publish_inputs(
        &self,
        artifact: ArtifactEvidence,
        target: TargetEvidence,
    ) -> Result<PublishInputs, EnvError> {
        match &artifact {
            ArtifactEvidence::Unselected { backend, .. } if backend != &self.backend_label => {
                return Err(refused(
                    "workspace artifact evidence names another backend than the admitted one",
                ))
            }
            ArtifactEvidence::Selected { object, .. } => {
                if object.trim().is_empty() {
                    return Err(refused("workspace artifact evidence is incomplete"));
                }
            }
            ArtifactEvidence::Unselected { .. } => {}
        }
        match &target {
            TargetEvidence::Absent { project, backend } => {
                if project != &self.policy.project_name || backend != &self.backend_label {
                    return Err(refused(
                        "workspace target evidence names another project or backend",
                    ));
                }
            }
            TargetEvidence::Observed(identity) => {
                if identity.backend != self.backend_label {
                    return Err(refused(
                        "workspace target evidence names another backend than the admitted one",
                    ));
                }
            }
        }
        Ok(PublishInputs {
            artifact,
            selection: self.selection.clone(),
            inputs_digest: self.inputs.clone(),
            target,
            platform: None,
            guest: Some(self.guest.clone()),
        })
    }
}

/// Plan the publication from the backend and the host's provisioning records.
///
/// `Ok(None)` is a hybrid environment: it declares no operator workspace, so it
/// publishes no guest artifact, context, listener or credential and this function
/// reads no backend fact (§8.1).
pub(in crate::runner) fn plan(
    request: ReservedPlanRequest<'_>,
    observation: &mut dyn ReservedBackendObservation,
) -> Result<Option<ReservedPublicationPlan>, EnvError> {
    let policy = request.policy;
    let deadline = request.deadline;
    let Some(workspace) =
        operator_workspace_for_policy(policy.repo_root.as_path(), policy).map_err(io)?
    else {
        return Ok(None);
    };
    deadline.remaining()?;
    let selected = verified(
        observation.selection(policy, deadline),
        "workspace backend selection",
    )?;
    deadline.remaining()?;
    let addressing = verified(
        observation.addressing(policy, &selected, deadline),
        "workspace alias addressing",
    )?;
    let backend = match selected.kind {
        BackendKind::Colima => Backend::Colima,
        BackendKind::DockerDesktop => Backend::DockerDesktop,
        BackendKind::DockerEngine => Backend::LinuxDocker,
    };
    // An alias no observation supplied, or a profile the selection cannot name,
    // refuses here rather than becoming a guessed address or render value.
    let alias = ChannelAliasPlan::observed(
        backend,
        &selected.context,
        addressing.proxy_bind,
        addressing.colima_gateway,
    )
    .map_err(|error| refused(format!("workspace alias is unqualified: {error}")))?;
    deadline.remaining()?;
    // The apply must be handed the policy that carries this alias; the alias
    // render is in-memory and creates no container and no network.
    let rendered = policy
        .with_workspace_host_alias(&alias.render_value)
        .map_err(|error| refused(format!("workspace alias cannot be rendered: {error}")))?;
    let render_digest = rendered
        .render
        .as_ref()
        .map(|render| render.closure.digest().to_owned())
        .ok_or_else(|| refused("the rendered workspace policy has no snapshotted render"))?;
    deadline.remaining()?;
    // Before anything applies: a container already running for this project is a
    // live predecessor, and this apply may recreate it.
    let predecessor = verified(
        observation.predecessor(policy, deadline),
        "workspace predecessor",
    )?;
    if predecessor.running && predecessor.desired.as_deref() != Some(render_digest.as_str()) {
        return Err(refused(format!(
            "a live workspace predecessor is running and this publication applies another render \
             (`{}`, recorded `{}`); stop it with `effigy container {} down` before republishing",
            render_digest,
            predecessor.desired.as_deref().unwrap_or("none"),
            policy.name,
        )));
    }
    // The source closure is a local file read plus the composed manifest, which
    // may resolve pinned sources with their own bounded timeouts. It is the last
    // unbounded-in-principle step before planning returns, so planning refuses to
    // start it without a floor of budget left and refuses the result when the
    // clock has passed: a publication is never built on work that overran it.
    deadline.remaining()?;
    if deadline
        .remaining()
        .is_ok_and(|remaining| remaining < SOURCE_CLOSURE_FLOOR)
    {
        return Err(unavailable(format!(
            "workspace publication has less than {}ms left, which cannot close the checkout's \
             source pins",
            SOURCE_CLOSURE_FLOOR.as_millis()
        )));
    }
    // One read of the checkout's identity, path map and pinned source closure:
    // the decision's frozen materialisation and the snapshots the producer
    // stages come from this single observation.
    let paths = workspace_path_map(rendered.repo_root.as_path(), &workspace).map_err(io)?;
    let closure = workspace_source_closure(rendered.repo_root.as_path(), &paths).map_err(io)?;
    deadline.remaining()?;
    let guest = environment_materialisation(&rendered, &selected, paths, &closure)?;
    let backend_label = selected.engine.as_str().to_owned();
    let selection = Digest::of(&canonical(&serde_json::json!({
        "environment_resource": guest.environment_resource.clone(),
        "guest_paths": {
            "host_checkout": guest.paths.host_checkout.clone(),
            "guest_checkout": guest.paths.guest_checkout.clone(),
            "workdir": guest.paths.workdir.clone(),
            "user": guest.paths.user.clone(),
            "home": guest.paths.home.clone(),
        },
        "alias": {
            "alias": alias.alias.clone(),
            "render_value": alias.render_value.clone(),
        },
        "backend": backend_label.clone(),
        "profile": selected.context.clone(),
    }))?);
    let inputs = Digest::of(&canonical(&serde_json::json!({
        "backend": backend_label,
        "profile": selected.context,
        "project": rendered.project_name.clone(),
        "service": rendered.primary_service.clone(),
        "sources": guest.sources.clone(),
    }))?);
    Ok(Some(ReservedPublicationPlan {
        selected,
        alias,
        policy: rendered,
        guest,
        snapshots: closure.snapshots,
        selection,
        inputs,
        deadline,
        backend_label,
        render_digest,
    }))
}

fn environment_materialisation(
    policy: &EffectiveContainerPolicy,
    selected: &BackendSelection,
    paths: WorkspacePathMap,
    closure: &WorkspaceSourceClosure,
) -> Result<GuestContextInputs, EnvError> {
    let guest = GuestContextInputs {
        version: effigy_core::build_info::active_version(),
        protocol: CHANNEL_PROTOCOL_VERSION,
        host_user: host_user(),
        checkout_id: crate::runner::task_engine::checkout::mint(policy.repo_root.as_path())
            .map_err(io)?
            .text()
            .to_owned(),
        // The environment resource an activation publishes, spelled the one way
        // the record and the context both spell it.
        environment_resource: format!("env:{}", policy.name),
        profile: selected.context.clone(),
        runtime: None,
        vm_type: None,
        routes: workspace_routes(policy),
        paths,
        sources: closure.pins.clone(),
    };
    guest
        .validate()
        .map_err(|error| refused(error.to_string()))?;
    Ok(guest)
}

/// What [`observe`] may use besides the plan.
pub(in crate::runner) struct ReservedObservationRequest<'a> {
    /// Where the apply read its checkout; the same root the plan rendered from.
    pub(in crate::runner) repo_root: &'a Path,
}

/// The frozen decision a post-apply observation is compared against.
///
/// It is read from the admitted activation, never invented: the reservation's
/// frozen publish inputs carry the target evidence and the guest
/// materialisation, and the activation carries the run identity a freshly
/// created target is attributed to.
#[derive(Debug)]
pub(in crate::runner) struct ReservedDecisionFacts {
    inputs: PublishInputs,
    reservation_generation: u64,
    created_by: RunId,
}

impl ReservedDecisionFacts {
    /// The only production constructor: the admitted activation's own frozen
    /// decision.
    pub(in crate::runner) fn of(
        activation: &mut effigy_containers::environment::WorkspaceActivation<'static>,
    ) -> Result<Self, EnvError> {
        let inputs = activation
            .reservation()
            .inputs()
            .cloned()
            .ok_or_else(|| refused("workspace activation has no frozen publication inputs"))?;
        let reservation_generation = activation.reservation().generation().get();
        let created_by = activation.environment_mut().id();
        Ok(Self {
            inputs,
            reservation_generation,
            created_by,
        })
    }
}

/// The producer's input, built from post-apply observations, plus the one secret
/// this publication may enrol with.
pub(in crate::runner) struct PreparedReservedPublication<'a> {
    guest: Box<dyn GuestWorkspace + 'a>,
    artifact: PathBuf,
    targets: Vec<TargetCandidate>,
    platform: BoundPlatform,
    alias: ChannelAliasPlan,
    context: WorkspaceContextInput,
    snapshots: Vec<(GuestObject, Vec<u8>)>,
    secret: ReservedEnrolmentSecret,
    target: ContainerIdentity,
    probe: Box<dyn WorkspaceGuestProbe + 'a>,
    reservation_generation: u64,
    deadline: Instant,
}

impl std::fmt::Debug for PreparedReservedPublication<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedReservedPublication")
            .field("target", &self.target.container_id)
            .field("artifact", &self.artifact)
            .finish_non_exhaustive()
    }
}

impl PreparedReservedPublication<'_> {
    /// The exact container this publication binds, once the host has verified it.
    /// A caller that needs to construct a probe for this target takes the id from
    /// here and nowhere else.
    pub(in crate::runner) fn target(&self) -> &ContainerIdentity {
        &self.target
    }

    pub(in crate::runner) fn verified_container_id(&self) -> &str {
        &self.target.container_id
    }
}

impl<'a> PreparedReservedPublication<'a> {
    /// The one `ReservedGuestInput` this publication is allowed to stage. It
    /// consumes the prepared facts, so a second publication cannot reuse the
    /// secret or the staged-target assumptions, and it takes no borrow: the
    /// probe it carries was built for the verified target and is owned here.
    pub(in crate::runner) fn into_input(self) -> ReservedGuestInput<'a> {
        let Self {
            guest,
            artifact,
            targets,
            platform,
            alias,
            context,
            snapshots,
            secret,
            target,
            probe,
            reservation_generation,
            deadline,
        } = self;
        let container_id = target.container_id.clone();
        ReservedGuestInput {
            guest,
            artifact,
            targets: Observed::Verified(targets, evidence()),
            platform: Observed::Verified(platform, evidence()),
            alias,
            probe,
            context,
            snapshots,
            credential: Box::new(move |session, generation| {
                if session.environment_generation().get() != reservation_generation {
                    return Err(refused(
                        "workspace session names another environment generation",
                    ));
                }
                Ok(WorkspaceEnrolmentCredential::new(
                    generation.to_string(),
                    container_id.clone(),
                    secret.expose(),
                ))
            }),
            deadline,
        }
    }
}

/// Observe the exact target and build the producer's input.
///
/// `policy` is the caller's own [`ReservedPublicationPlan::rendered_policy`] copy:
/// it must be the plan's exact render (checked here), and it is what the guest the
/// producer stages through borrows.
///
/// Every fact is derived from the plan, the host's verified binding, the
/// backend's image inspect, the guest's own report in that binding and the
/// artifact's producer receipt. The frozen decision evidence is the guard: a
/// target the decision did not name refuses before anything is staged, so a
/// raced or replaced container is never published into.
pub(in crate::runner) fn observe<'a>(
    request: ReservedObservationRequest<'_>,
    plan: &ReservedPublicationPlan,
    policy: &'a EffectiveContainerPolicy,
    probe: &ReservedProbeFactory<'a>,
    facts: &ReservedDecisionFacts,
    observation: &'a mut dyn ReservedBackendObservation,
) -> Result<PreparedReservedPublication<'a>, EnvError> {
    let deadline = plan.deadline;
    if policy.render.as_ref().map(|render| render.closure.digest())
        != Some(plan.render_digest.as_str())
    {
        return Err(refused(
            "workspace publication was handed another render than the planned one",
        ));
    }
    if request.repo_root != plan.policy.repo_root.as_path() {
        return Err(refused(
            "workspace publication observed another checkout than the planned one",
        ));
    }
    let ReservedDecisionFacts {
        inputs,
        reservation_generation,
        created_by,
    } = facts;
    let reservation_generation = *reservation_generation;
    let created_by = *created_by;
    // The decision must be this plan's decision: another plan's frozen
    // materialisation, selection or inputs would publish bytes the decision
    // never compared.
    if inputs.guest.as_ref() != Some(plan.guest_inputs())
        || inputs.selection != plan.selection
        || inputs.inputs_digest != plan.inputs
    {
        return Err(refused(
            "workspace decision froze another publication input than this plan",
        ));
    }
    deadline.remaining()?;
    let binding = verified(
        observation.binding(policy, deadline),
        "workspace primary binding",
    )?;
    // A full lowercase 64-hex id is the only id form a target may carry; a short
    // id, a name or a hash-prefixed spelling refuses here.
    if Digest::parse_hex(&binding.container_id).is_none() {
        return Err(refused(
            "workspace binding is not a full container id, so placement is unproven",
        ));
    }
    // The complete frozen selection, not just its engine: a different profile or
    // Docker context with the same engine would otherwise publish the planned
    // profile and alias for another backend's target.
    if binding.selected != *plan.backend() {
        return Err(refused(format!(
            "workspace binding reports backend `{}`/`{}` but the admitted selection is `{}`/`{}`",
            binding.selected.engine.as_str(),
            binding.selected.context,
            plan.backend().engine.as_str(),
            plan.backend().context,
        )));
    }
    deadline.remaining()?;
    let identity = verified(
        observation.identity(policy, &binding.container_id, deadline),
        "workspace guest identity",
    )?;
    // The guest read the container the host bound, not a replacement the service
    // alias now resolves to.
    if identity.container_id != binding.container_id {
        return Err(refused(
            "workspace guest identity names another container than the verified binding",
        ));
    }
    if identity.vm_boot_id.trim().is_empty() || identity.vm_boot_id.contains('\0') {
        return Err(refused("workspace guest reported no VM boot identity"));
    }
    let target = ContainerIdentity {
        container_id: identity.container_id.clone(),
        vm_boot_id: identity.vm_boot_id.clone(),
        backend: binding.backend_label().to_owned(),
    };
    let created_by = match &inputs.target {
        TargetEvidence::Observed(expected) => {
            if expected != &target {
                return Err(refused(
                    "workspace target differs from the identity the decision froze",
                ));
            }
            None
        }
        TargetEvidence::Absent { project, backend } => {
            if project != &plan.policy.project_name || backend != &plan.backend_label {
                return Err(refused(
                    "workspace target evidence names another project or backend",
                ));
            }
            Some(created_by)
        }
    };
    deadline.remaining()?;
    // The image platform is the backend's own report about the image; the loader
    // family is the image root the guest reports. Neither the guest's `uname` nor
    // this host's architecture selects the artifact.
    let reported = verified(
        observation.image_platform(policy, &binding.container_id, deadline),
        "workspace image platform",
    )?;
    let loader = loader_family(&identity.libc)?;
    let platform = ImagePlatform {
        os: reported.os.clone(),
        architecture: reported.architecture.clone(),
        variant: reported.variant.clone(),
        loader,
    };
    platform
        .validate()
        .map_err(|error| refused(error.to_string()))?;
    // The guest's machine type is diagnostic, but a guest that disagrees with the
    // image it runs is a conflicting observation, not a platform to publish.
    if guest_machine(&identity.platform)? != platform.architecture.as_str() {
        return Err(refused(format!(
            "workspace guest reports machine type `{}` while its image reports `{}`",
            identity.platform, platform.architecture
        )));
    }
    let family =
        LinuxWorkspaceTarget::from_image(&platform.architecture, &identity.libc).map_err(io)?;
    let artifact_platform = family.image_platform();
    if artifact_platform != platform {
        return Err(refused(
            "pinned artifact family and observed image platform differ",
        ));
    }
    let execution_available = native_execution(&platform)?;
    deadline.remaining()?;
    let artifact = verified(
        observation.artifact(policy, family, deadline),
        "workspace guest artifact",
    )?;
    // One absolute deadline for the digest, so an artifact that cannot be
    // authenticated inside the publication refuses instead of overrunning it.
    let binary = verify_guest_artifact_until(&artifact, family, Some(deadline.at())).map_err(io)?;
    let digest = binary
        .digest
        .strip_prefix("sha256:")
        .and_then(Digest::parse_hex)
        .ok_or_else(|| refused("workspace artifact digest is not canonical"))?;
    let binding_facts = BoundPlatform {
        target: target.clone(),
        platform: platform.clone(),
        artifact_platform,
        artifact: GuestObject {
            path: format!("/run/effigy/objects/bin/{digest}"),
            digest,
        },
        artifact_version: binary.version.clone(),
        execution_available,
    };
    // The same frozen materialisation the decision recorded, with the identity
    // this observation just proved.
    let context = WorkspaceContextInput {
        generation: 0,
        host: WorkspaceHostIdentity {
            version: effigy_core::build_info::active_version(),
            protocol: CHANNEL_PROTOCOL_VERSION,
        },
        binary: binary.clone(),
        activation: WorkspaceActivationIdentity {
            host_user: plan.guest.host_user.clone(),
            checkout_id: plan.guest.checkout_id.clone(),
            environment: WorkspaceEnvironmentIdentity {
                resource_id: plan.guest.environment_resource.clone(),
                generation: reservation_generation.to_string(),
            },
            container: identity.clone(),
        },
        session_run_id: None,
        session_generation: None,
        paths: plan.guest.paths.clone(),
        backend: WorkspaceBackendDescriptor {
            kind: plan.backend_label.clone(),
            profile: plan.guest.profile.clone(),
            runtime: plan.guest.runtime.clone(),
            vm_type: plan.guest.vm_type.clone(),
        },
        sources: plan.guest.sources.clone(),
        routes: plan.guest.routes.clone(),
        channel: WorkspaceChannelLocator::default(),
    };
    let snapshots = snapshots(&plan.snapshots)?;
    deadline.remaining()?;
    let secret = ReservedEnrolmentSecret::mint()?;
    // The injected production probe, built for the identity just verified. It is
    // owned here, so the publication closure can hand the producer an input with
    // no borrow of a local.
    let probe = probe(&target).map_err(io)?;
    let guest = observation.guest(policy, &target, deadline)?;
    Ok(PreparedReservedPublication {
        guest,
        artifact,
        targets: vec![TargetCandidate {
            identity: target.clone(),
            project: plan.policy.project_name.clone(),
            created_by,
        }],
        platform: binding_facts,
        alias: plan.alias.clone(),
        context,
        snapshots,
        secret,
        target,
        probe,
        reservation_generation,
        deadline: deadline.at(),
    })
}

/// The C library the image root ships, from the loader the guest observed
/// (`/lib/ld-musl-*.so.1` against a glibc loader). An image with neither is
/// unproven rather than glibc by default.
fn loader_family(libc: &str) -> Result<LoaderFamily, EnvError> {
    match libc.trim().to_ascii_lowercase().as_str() {
        "gnu" | "glibc" => Ok(LoaderFamily::Gnu),
        "musl" => Ok(LoaderFamily::Musl),
        other => Err(refused(format!(
            "workspace image reports no provable C library (`{other}`), so no guest artifact can \
             be proven to run in it"
        ))),
    }
}

/// The machine type a guest's own `uname` reported, in the backend's spelling.
/// This is diagnostic evidence only: it fences the target identity and
/// cross-checks the image, and it never selects an artifact.
fn guest_machine(platform: &str) -> Result<&'static str, EnvError> {
    match platform.trim() {
        "x86_64" | "amd64" => Ok("amd64"),
        "aarch64" | "arm64" => Ok("arm64"),
        other => Err(refused(format!(
            "workspace guest reports machine type `{other}`, which has no pinned Linux guest \
             artifact"
        ))),
    }
}

/// Whether this host can execute the image natively.
///
/// The contract requires native execution or verified emulation on this backend,
/// and only the native case is observable today: the backend reports an image
/// built for this process's own machine type, and the image's own binaries ran in
/// the exact target (the loader observation above). An image built for another
/// machine type needs emulation evidence no owner produces yet (`Q-018`), so it
/// refuses rather than claiming execution.
fn native_execution(platform: &ImagePlatform) -> Result<bool, EnvError> {
    let host = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => {
            return Err(unavailable(format!(
                "this host architecture `{other}` cannot verify guest execution"
            )))
        }
    };
    if platform.architecture != host {
        return Err(unavailable(format!(
            "workspace image runs `{}` on a `{host}` host: emulated execution has no verified \
             observation owner, so no artifact may be claimed executable",
            platform.architecture
        )));
    }
    Ok(true)
}

/// The immutable source snapshots this context names, as immutable guest objects.
fn snapshots(snapshots: &[(String, Vec<u8>)]) -> Result<Vec<(GuestObject, Vec<u8>)>, EnvError> {
    snapshots
        .iter()
        .map(|(digest, bytes)| {
            let parsed = Digest::parse_hex(digest)
                .ok_or_else(|| refused("workspace source snapshot has no canonical digest"))?;
            if parsed != Digest::of(bytes) {
                return Err(refused(
                    "workspace source snapshot bytes differ from the digest it names",
                ));
            }
            Ok((
                GuestObject {
                    path: snapshot_path(digest),
                    digest: parsed,
                },
                bytes.clone(),
            ))
        })
        .collect()
}

fn verified<T>(observed: Observed<T>, what: &str) -> Result<T, EnvError> {
    match observed {
        Observed::Verified(value, _) => Ok(value),
        Observed::Unknown(reason) => Err(unavailable(format!("{what} is unknown: {reason}"))),
        Observed::Absent(_) => Err(unavailable(format!("{what} is absent"))),
    }
}

fn canonical(value: &serde_json::Value) -> Result<Vec<u8>, EnvError> {
    serde_json::to_vec(value).map_err(|error| {
        unavailable(format!(
            "cannot serialize workspace publication facts: {error}"
        ))
    })
}

/// The one enrolment secret a reserved publication may stage: 32 bytes from the
/// OS, written as the canonical lowercase 64-hex form. It is never logged, never
/// derived from a request id, a pid or a clock, and it is not the channel
/// capability.
pub(in crate::runner) struct ReservedEnrolmentSecret(String);

impl ReservedEnrolmentSecret {
    fn mint() -> Result<Self, EnvError> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| {
            unavailable("the host has no cryptographic entropy, so no enrolment may be issued")
        })?;
        let mut text = String::with_capacity(64);
        for byte in bytes {
            text.push(char::from_digit((byte >> 4) as u32, 16).expect("nibble"));
            text.push(char::from_digit((byte & 0x0f) as u32, 16).expect("nibble"));
        }
        Ok(Self(text))
    }

    #[cfg(test)]
    pub(in crate::runner) fn canonical(text: &str) -> Option<Self> {
        Digest::parse_hex(text)?;
        Some(Self(text.to_owned()))
    }

    pub(in crate::runner) fn expose(&self) -> String {
        self.0.clone()
    }
}

impl std::fmt::Debug for ReservedEnrolmentSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReservedEnrolmentSecret(<redacted>)")
    }
}

#[cfg(test)]
#[path = "reserved_input_tests.rs"]
mod tests;
