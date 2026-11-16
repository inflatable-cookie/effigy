//! The production workspace context: the legacy `effigy.workspace-context.v1`
//! document the ordinary (non-qualified) publication writes (052 §8.2, §8.11).
//!
//! Two publications exist while v0.15 qualifies the workspace client:
//!
//! - the **qualified** path (`workspace_command::reserved_guest`) creates one
//!   admitted session, one qualified listener and one strict full v2 document,
//!   and a guest reads it strictly through [`super::context::v2`];
//! - the **ordinary production** path
//!   (`workspace_command::provisioning::publication`) publishes the pinned
//!   executable, the source snapshots and this legacy v1 document. Production
//!   enablement of the v2 path is Tom's review gate (effigy#626), so v1 is what
//!   every production workspace container carries today.
//!
//! This reader is deliberately separate and explicitly named. A v1 document is
//! never fed to the strict v2 reader and no v2 field is inferred from it: it
//! parses the v1 document under the v1 rules it was published with, and checks
//! exactly the identity facts a v1 document carries — the container id, VM
//! boot, image platform and libc from an independent guest observation, the
//! host and guest version and protocol it pins, the generation `current` names,
//! and the version and digest of the executable that is running. It grants no
//! session, no credential and no channel, because the v1 document names none.
use super::context::v2::SCHEMA as V2_SCHEMA;
use super::context::{
    declared_schema, read_published_body, ContextReadError, ContextRefusal, GuestReader,
    WorkspaceContainerIdentity, WorkspaceContext, CONTEXT_SCHEMA, HOST_REPREPARE_COMMAND,
};
use super::identity::{GuestIdentity, GuestIdentityRefusal, GuestSelfObservation};
use super::RunningBinary;

/// One legacy production context this guest already checked against its own
/// independent identity observation and the executable it is running as.
#[derive(Debug)]
pub(crate) struct PublishedLegacyGuestContext {
    pub(crate) generation: u64,
    pub(crate) context: WorkspaceContext,
    pub(crate) observation: GuestSelfObservation,
}

/// Read and check one already-read legacy document.
pub(crate) fn published_legacy_at(
    bytes: &[u8],
    generation: u64,
    identity: &mut dyn GuestIdentity,
    binary: &RunningBinary,
) -> Result<PublishedLegacyGuestContext, ContextReadError> {
    // The v1 parser refuses another schema, an unknown field and a non-v1
    // protocol before any of the checks below, so nothing is reinterpreted.
    let context = WorkspaceContext::parse(bytes)?;
    if context.generation != generation {
        return Err(ContextRefusal::unavailable(format!(
            "workspace context generation {generation} carries generation {}; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            context.generation
        ))
        .into());
    }
    // The one executable fact a guest can establish: the bytes it is running
    // are the pinned artifact. Production installs the artifact's own
    // `effigy.active-version` sidecar beside the stable entry link, not beside
    // the digest-addressed executable that link resolves to, so the running
    // binary's self-reported version is the artifact's packaged version and is
    // never a proxy for the pin. The digest is the exact claim, and the version
    // and protocol the guest reports are the context's own (checked by
    // `WorkspaceContext::parse`).
    if binary.digest != context.binary.digest {
        return Err(ContextRefusal::version_mismatch(format!(
            "the prepared workspace pins guest effigy {} ({}) but the executable running here is {}",
            context.binary.version, context.binary.digest, binary.digest
        ))
        .into());
    }
    let observed = identity.observe().map_err(|reason| {
        let message = format!(
            "guest-local identity is unproven ({reason}), so this container cannot show that the \
             production context generation {generation} was prepared for it; run the command on \
             the host"
        );
        ContextReadError::Refused(match reason {
            GuestIdentityRefusal::Mismatch => ContextRefusal::identity_mismatch(message),
            _ => ContextRefusal::unavailable(message),
        })
    })?;
    let live = WorkspaceContainerIdentity {
        container_id: observed.container_id.clone(),
        vm_boot_id: observed.vm_boot_id.clone(),
        platform: observed.platform.clone(),
        libc: observed.libc.clone(),
    };
    let claimed = &context.activation.container;
    if !claimed.matches(&live) {
        return Err(ContextRefusal::identity_mismatch(format!(
            "the production context generation {generation} belongs to container `{}` on VM boot \
             `{}` ({}), but this is container `{}` on VM boot `{}` ({})",
            claimed.container_id,
            claimed.vm_boot_id,
            claimed.platform,
            observed.container_id,
            observed.vm_boot_id,
            observed.platform
        ))
        .into());
    }
    Ok(PublishedLegacyGuestContext {
        generation,
        context,
        observation: observed,
    })
}

/// The refusal a production guest gives a command that needs host authority.
///
/// The legacy document carries no admitted session, no credential and no
/// channel, so this is not a re-preparation gap: re-preparing republishes the
/// same legacy document. The host CLI is what actually serves the command.
pub(crate) fn host_authority_unavailable(published: &PublishedLegacyGuestContext) -> String {
    format!(
        "this container runs the workspace's production context (legacy schema `{CONTEXT_SCHEMA}`, \
         generation {}), which pins no admitted workspace session and no host authority channel, \
         so a command that needs host authority has to run on the host in `{}`. Re-preparing the \
         workspace (`{HOST_REPREPARE_COMMAND}`) republishes the same legacy document and cannot \
         grant host authority",
        published.generation, published.context.paths.host_checkout
    )
}

/// One published generation, routed by the schema it declares.
///
/// The routing is explicit so a document is only ever read by the reader that
/// owns its schema. The strict v2 reader keeps every guarantee it had; the
/// legacy reader is this module, and an unknown schema refuses instead of being
/// guessed at (§8.11).
#[derive(Debug)]
pub(crate) enum PublishedContext {
    Strict(Box<super::context::v2::PublishedGuestContext>),
    Legacy(Box<PublishedLegacyGuestContext>),
}

/// Read the one published generation and dispatch on its declared schema.
pub(crate) fn read_published(
    guest: &mut dyn GuestReader,
    identity: &mut dyn GuestIdentity,
) -> Result<Option<PublishedContext>, ContextReadError> {
    let Some(published) = read_published_body(guest)? else {
        return Ok(None);
    };
    let schema = declared_schema(&published.bytes)?;
    let generation = published.generation;
    match schema.as_str() {
        V2_SCHEMA => super::context::v2::published_guest_at(&published.bytes, generation, identity)
            .map(|context| Some(PublishedContext::Strict(Box::new(context)))),
        CONTEXT_SCHEMA => {
            let binary = RunningBinary::observe().map_err(ContextReadError::Guest)?;
            published_legacy_at(&published.bytes, generation, identity, &binary)
                .map(|context| Some(PublishedContext::Legacy(Box::new(context))))
        }
        schema => Err(ContextRefusal::unavailable(format!(
            "the published workspace context declares schema `{schema}`, which this build does not \
             read; run the command on the host, or re-prepare the workspace on the host \
             (`{HOST_REPREPARE_COMMAND}`) to publish `{V2_SCHEMA}` or `{CONTEXT_SCHEMA}`"
        ))
        .into()),
    }
}
