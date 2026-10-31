//! The C publication sequence. Backend facts are observations; the session,
//! fence, staged ticket, locator and commit receipt come from their real owners.

use super::*;
use effigy_containers::environment::{EnvError, WorkspaceActivation};
use effigy_kernel::id::{Digest, Generation};
use effigy_kernel::observe::{Evidence, Observed, UnknownReason};
use effigy_kernel::run::{GuestPublicationReceipt, RunError};
use effigy_kernel::workspace_session::{
    BoundPlatform, ContainerIdentity, GuestObject, GuestSelection, SelectionObjects,
    SelectionObligation, SelectionReceipt, SelectorChange, SelectorReader, SessionRef,
    TargetCandidate, WorkspacePublicationReceipt, BINARY_SELECTOR, CONTEXT_SELECTOR,
    ENTRY_SELECTOR,
};
use std::time::{Duration, Instant};

use crate::runner::environment::workspace::{WorkspacePublication, WorkspaceSelectionIo};
use crate::runner::workspace_client::context::v2::WorkspaceContextV2;
use crate::runner::workspace_client::identity::GuestSelfObservation;
use crate::runner::workspace_transport::qualification::{
    prepare_channel, ChannelAliasPlan, ChannelPreparation, GuestChannelProbe,
};

pub(in crate::runner) trait WorkspaceGuestProbe: GuestChannelProbe {
    /// Re-read this exact full document in the staged guest, independently
    /// compare placement and executable, then perform a pinned TLS-only dial.
    fn context_dial(
        &mut self,
        target: &WorkspaceContainerIdentity,
        object: &GuestObject,
        context: &GuestObject,
        budget: Duration,
    ) -> Observed<crate::runner::workspace_transport::qualification::GuestDialEvidence>;
    fn identity(
        &mut self,
        target: &WorkspaceContainerIdentity,
        object: &GuestObject,
        budget: Duration,
    ) -> Observed<GuestSelfObservation>;
}

type CredentialFactory<'a> =
    Box<dyn FnOnce(&SessionRef, u64) -> Result<WorkspaceEnrolmentCredential, EnvError> + 'a>;

/// The producer's input. Everything here is owned except the injected probe:
/// the artifact path and the observed alias plan are the builder's own values,
/// so one built input never borrows the state that gave it those facts and no
/// caller is left holding a half-alive builder.
pub(in crate::runner) struct ReservedGuestInput<'a> {
    pub(in crate::runner) guest: Box<dyn GuestWorkspace + 'a>,
    pub(in crate::runner) artifact: PathBuf,
    pub(in crate::runner) targets: Observed<Vec<TargetCandidate>>,
    pub(in crate::runner) platform: Observed<BoundPlatform>,
    pub(in crate::runner) alias: ChannelAliasPlan,
    pub(in crate::runner) probe: &'a mut dyn WorkspaceGuestProbe,
    pub(in crate::runner) context: WorkspaceContextInput,
    pub(in crate::runner) snapshots: Vec<(GuestObject, Vec<u8>)>,
    pub(in crate::runner) credential: CredentialFactory<'a>,
    /// The one absolute deadline of the whole publication. It is the same clock
    /// the planner, the backend observations and the guest staging read, so a
    /// delay between observation and this call cannot extend any of them.
    pub(in crate::runner) deadline: Instant,
}

impl std::fmt::Debug for ReservedGuestInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReservedGuestInput")
            .field("artifact", &self.artifact)
            .finish_non_exhaustive()
    }
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
fn evidence() -> Evidence {
    Evidence::new("exact reserved guest publication".into(), None).expect("fixed evidence")
}

pub(in crate::runner) fn prepare_reserved_guest<'a>(
    activation: &mut WorkspaceActivation<'static>,
    mut input: ReservedGuestInput<'a>,
) -> Result<WorkspacePublication<'a>, EnvError> {
    // The caller's absolute deadline, not a fresh budget measured from now.
    let until = input.deadline;
    let remaining = || {
        until
            .checked_duration_since(Instant::now())
            .filter(|budget| !budget.is_zero())
            .ok_or_else(|| unavailable("workspace publication deadline expired"))
    };
    let reservation = activation.reservation().clone();
    let ctx_plan = {
        // The owner already holds Env; this observation takes no new gate.
        let generation = input
            .context
            .activation
            .environment
            .generation
            .parse::<u64>()
            .map_err(|_| refused("workspace publication has no claimed environment generation"))?;
        if generation != reservation.generation().get() {
            return Err(refused(
                "workspace publication targets another environment generation",
            ));
        }
        activation.current_plan()?
    };
    let effigy_kernel::id::Scope::Checkout(checkout) = ctx_plan.environment().scope() else {
        return Err(refused("workspace environment has no checkout scope"));
    };
    if input.context.activation.checkout_id != checkout.text() {
        return Err(refused(
            "workspace host checkout differs from the admitted environment",
        ));
    }
    let ticket = activation
        .environment_mut()
        .intend_workspace_publication(&ctx_plan)?;
    let result = (|| {
        remaining()?;
        let target = activation
            .environment_mut()
            .bind_target(&reservation, input.targets)?;
        let binding = activation
            .environment_mut()
            .bind_platform(&reservation, input.platform)?;
        if binding.target != target {
            return Err(refused("image and apply target differ"));
        }
        let object = staging::stage_binary(
            activation.environment_mut(),
            &reservation,
            input.guest.as_mut(),
            input.artifact.as_path(),
            Observed::Verified(binding.clone(), evidence()),
        )?;
        let machine = match binding.platform.architecture.as_str() {
            "amd64" => "x86_64",
            "arm64" => "aarch64",
            _ => return Err(refused("image architecture is unsupported")),
        };
        let libc = match binding.platform.loader {
            effigy_kernel::workspace_session::LoaderFamily::Gnu => "gnu",
            effigy_kernel::workspace_session::LoaderFamily::Musl => "musl",
        };
        let container = WorkspaceContainerIdentity {
            container_id: target.container_id.clone(),
            vm_boot_id: target.vm_boot_id.clone(),
            platform: machine.into(),
            libc: libc.into(),
        };
        let observed = match input.probe.identity(&container, &object, remaining()?) {
            Observed::Verified(observed, _) => observed,
            Observed::Unknown(reason) => {
                return Err(unavailable(format!(
                    "guest-local identity is unavailable: {reason}"
                )))
            }
            Observed::Absent(_) => return Err(unavailable("guest-local identity is absent")),
        };
        observed
            .compare(&container)
            .map_err(|error| refused(error.to_string()))?;
        // No credential is minted or staged until the independent guest agrees.
        let generation = allocate_generation(input.guest.as_mut()).map_err(io)?;
        let credential = (input.credential)(activation.session(), generation)?;
        if credential.environment_generation != reservation.generation().get().to_string()
            || credential.container_id != target.container_id
        {
            return Err(refused("enrolment credential targets another activation"));
        }
        let credential_bytes = credential.file_body().into_bytes();
        let credential_object = GuestObject {
            path: format!("/run/effigy/objects/enrol/{generation}"),
            digest: Digest::of(&credential_bytes),
        };
        let supervisor_id = activation.supervisor_id().to_owned();
        let session = activation.session().clone();
        let mut channel = prepare_channel(
            ChannelPreparation {
                container_mode: true,
                plan: &input.alias,
                target: &container,
                session: &session,
                supervisor: &supervisor_id,
            },
            activation.session_run_mut(),
            input.probe,
            remaining()?,
        )
        .map_err(|error| unavailable(error.to_string()))?
        .ok_or_else(|| refused("container-mode publication has no qualified channel"))?;
        for (snapshot, bytes) in &input.snapshots {
            staging::stage_document(input.guest.as_mut(), snapshot, bytes, 0o444).map_err(io)?;
        }
        for source in &input.context.sources {
            if let Some(path) = source
                .snapshot
                .as_deref()
                .filter(|path| path.starts_with("/run/effigy/"))
            {
                let expected = source
                    .digest
                    .strip_prefix("sha256:")
                    .and_then(Digest::parse_hex)
                    .ok_or_else(|| refused("snapshot has no canonical digest"))?;
                if input
                    .guest
                    .file_sha256(path)
                    .map_err(io)?
                    .as_deref()
                    .and_then(Digest::parse_hex)
                    .as_ref()
                    != Some(&expected)
                {
                    return Err(refused("required source snapshot is missing or differs"));
                }
            }
        }
        let plan = activation.current_plan()?;
        input.context.generation = generation;
        input.context.binary = GuestBinaryIdentity {
            version: binding.artifact_version,
            digest: format!("sha256:{}", object.digest.hex()),
            protocol: CHANNEL_PROTOCOL_VERSION,
        };
        input.context.activation.container = container;
        input.context.activation.environment = WorkspaceEnvironmentIdentity {
            resource_id: plan.environment().name().to_string(),
            generation: plan.environment_generation().get().to_string(),
        };
        let mut objects = SelectionObjects {
            binary: object,
            context: GuestObject {
                path: context_path(generation),
                digest: Digest([0; 32]),
            },
            credential: credential_object,
            context_generation: Generation::new(generation),
            credential_generation: Generation::new(generation),
        };
        let document = WorkspaceContextV2::prepared(
            &plan,
            input.context,
            &objects,
            &reservation
                .inputs()
                .ok_or_else(|| refused("publication has no frozen inputs"))?
                .selection,
            &channel.locator,
        )
        .map_err(|error| refused(error.to_string()))?;
        let bytes = document.render().map_err(io)?;
        objects.context.digest = Digest::of(&bytes);
        staging::stage_document(
            input.guest.as_mut(),
            &objects.credential,
            &credential_bytes,
            0o600,
        )
        .map_err(io)?;
        input
            .guest
            .set_owner(&objects.credential.path, &document.paths.user)
            .map_err(io)?;
        staging::stage_document(input.guest.as_mut(), &objects.context, &bytes, 0o444)
            .map_err(io)?;
        let parsed = WorkspaceContextV2::parse(
            &input
                .guest
                .read_file(&objects.context.path)
                .map_err(io)?
                .ok_or_else(|| unavailable("staged full v2 context is absent"))?,
        )
        .map_err(|error| refused(error.to_string()))?;
        if parsed != document {
            return Err(refused("guest parsed another immutable context"));
        }
        let full_probe = match input.probe.context_dial(
            channel.locator.target(),
            &objects.binary,
            &objects.context,
            remaining()?,
        ) {
            Observed::Verified(reply, _) if reply.target == *channel.locator.target() => reply,
            Observed::Verified(_, _) => {
                return Err(refused("full-context probe observed another target"))
            }
            Observed::Unknown(reason) => {
                return Err(unavailable(format!(
                    "full guest context probe is unavailable: {reason}"
                )))
            }
            Observed::Absent(_) => return Err(unavailable("full guest context probe is absent")),
        };
        // Requalify against the already-published launch. The merged owner API
        // refuses any changed target/peer/listener/pin instead of rebinding it.
        let guest_target = effigy_kernel::workspace_session::GuestLocatorTarget {
            container_id: full_probe.target.container_id.clone(),
            vm_boot_id: full_probe.target.vm_boot_id.clone(),
        };
        channel
            .launch
            .qualify_guest(
                activation.session_run_mut(),
                guest_target.clone(),
                full_probe.peer.ip(),
                |challenge| {
                    challenge.verified(
                        &guest_target,
                        full_probe.peer,
                        &document.locator.endpoint,
                        &full_probe.pin,
                    )
                },
            )
            .map_err(|error| EnvError::Kernel(error.into()))?;
        let publication = WorkspacePublicationReceipt {
            publication: GuestPublicationReceipt {
                environment: plan.fence(),
                container_id: target.container_id.clone(),
                vm_boot_id: target.vm_boot_id.clone(),
                artifact_digest: document.artifact_digest.clone(),
                context_generation: generation,
            },
            activation_run: activation.environment_mut().id(),
            session: plan.binding().clone(),
        };
        let mut reader = OwnedSelectionIo {
            guest: input.guest,
            target,
        };
        let switches = [BINARY_SELECTOR, ENTRY_SELECTOR, CONTEXT_SELECTOR]
            .into_iter()
            .map(|path| {
                let expected = match reader.read_selector(path) {
                    Observed::Verified(value, _) => Some(value),
                    Observed::Absent(_) => None,
                    Observed::Unknown(reason) => return Err(unavailable(reason.to_string())),
                };
                let next = if path == CONTEXT_SELECTOR {
                    generation.to_string()
                } else {
                    objects.binary.path.clone()
                };
                Ok(SelectorChange {
                    path: path.into(),
                    expected,
                    next,
                })
            })
            .collect::<Result<Vec<_>, EnvError>>()?
            .try_into()
            .map_err(|_| refused("workspace selection needs three selectors"))?;
        let selection = GuestSelection {
            backend: binding.target.backend,
            objects,
            switches,
        };
        let receipt = SelectionReceipt::new(
            &plan,
            publication.clone(),
            selection,
            &bytes,
            activation.session_run_mut(),
            &mut channel.launch,
            &mut reader,
        )
        .map_err(EnvError::Kernel)?;
        remaining()?;
        activation
            .environment_mut()
            .acknowledge_workspace_publication(&plan, &ticket, publication)?;
        Ok(WorkspacePublication {
            receipt,
            channel,
            guest: Box::new(reader),
        })
    })();
    if let Err(error) = &result {
        activation.environment_mut().unknown_workspace_publication(
            &ticket,
            UnknownReason::Inconsistent(error.to_string()),
        )?;
    }
    result
}

fn allocate_generation(guest: &mut dyn GuestWorkspace) -> Result<u64, RunnerError> {
    let mut generation = read_published_generation(guest)
        .map_err(context_read_error)?
        .unwrap_or(0);
    for _ in 0..1024 {
        generation = generation.checked_add(1).ok_or_else(|| {
            RunnerError::task_invocation("workspace context generations exhausted")
        })?;
        if guest.read_file(&context_path(generation))?.is_none()
            && guest
                .read_file(&format!("/run/effigy/objects/enrol/{generation}"))?
                .is_none()
        {
            return Ok(generation);
        }
    }
    Err(RunnerError::task_invocation(
        "workspace has too many retained context orphans to allocate safely",
    ))
}

struct OwnedSelectionIo<'a> {
    guest: Box<dyn GuestWorkspace + 'a>,
    target: ContainerIdentity,
}
impl SelectorReader for OwnedSelectionIo<'_> {
    fn target(&self) -> ContainerIdentity {
        self.target.clone()
    }
    fn read_selector(&mut self, path: &str) -> Observed<String> {
        staging::GuestSelectionReader {
            guest: self.guest.as_mut(),
            target: self.target.clone(),
        }
        .read_selector(path)
    }
    fn read_object(&mut self, path: &str) -> Observed<Digest> {
        staging::GuestSelectionReader {
            guest: self.guest.as_mut(),
            target: self.target.clone(),
        }
        .read_object(path)
    }
}
impl WorkspaceSelectionIo for OwnedSelectionIo<'_> {
    fn switch_recovery(
        &mut self,
        guard: &effigy_kernel::gate::GateGuard,
        obligation: &SelectionObligation,
    ) -> Result<(), EnvError> {
        staging::switch_recovery(
            guard,
            obligation,
            &mut staging::GuestSelectionReader {
                guest: self.guest.as_mut(),
                target: self.target.clone(),
            },
        )
    }
    fn target_liveness(&mut self) -> Observed<()> {
        match self.guest.probe_container_identity() {
            Ok(identity)
                if identity.container_id == self.target.container_id
                    && identity.vm_boot_id == self.target.vm_boot_id =>
            {
                Observed::Verified((), evidence())
            }
            Ok(_) => Observed::Absent(effigy_kernel::observe::AbsenceProof::ContainerGone {
                id: self.target.container_id.clone(),
            }),
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        }
    }
    fn switch(
        &mut self,
        run: &mut effigy_containers::environment::EnvRun<'_>,
        obligation: &SelectionObligation,
    ) -> Result<(), EnvError> {
        staging::switch_selection(
            run,
            obligation,
            &mut staging::GuestSelectionReader {
                guest: self.guest.as_mut(),
                target: self.target.clone(),
            },
        )
    }
}

#[cfg(test)]
#[path = "reserved_guest_tests.rs"]
mod tests;

/// Re-open I/O for the obligation's exact target; this carries no authority to
/// switch without the owner's live Run/launch and an acquired Env guard.
#[allow(dead_code)] // Serialized recovery caller is owned by the serving integration.
pub(in crate::runner) fn selection_io<'a>(
    guest: Box<dyn GuestWorkspace + 'a>,
    target: ContainerIdentity,
) -> Box<dyn WorkspaceSelectionIo + 'a> {
    Box::new(OwnedSelectionIo { guest, target })
}
