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
//! generation `current` names, and the SHA-256 of the executable that is
//! running. Version and protocol stay the document's own claims, checked by the
//! v1 parser; the running binary's *self-reported* version is not compared (see
//! [`published_legacy_at`]). It grants no session, no credential and no channel,
//! because the v1 document names none.
//!
//! Because the placement is verified, a declared task whose steps are bound to
//! this workspace container runs **here**: this process is already inside the
//! container the host would have exec'd into, so running the steps locally is
//! the same placement, and needs none of the host authority a v1 document
//! cannot carry (planner ruling, 2026-11-16; [`classify_task`]). A task that
//! needs any host facility refuses by name instead.
use std::collections::BTreeSet;

use effigy_cli::TaskInvocation;
use effigy_manifest::{
    resolve_task_execution_binding, ManifestManagedRun, ManifestManagedRunStep, ManifestTask,
    ManifestTaskRunIn, ResolvedTaskExecutionBinding, ResolvedWorkspaceBinding, TaskManifest,
};

use super::context::v2::SCHEMA as V2_SCHEMA;
use super::context::{
    checkout_relative, declared_schema, read_published_body, sha256_text, CheckoutReader,
    ContextReadError, ContextRefusal, GuestReader, WorkspaceContainerIdentity, WorkspaceContext,
    COMPOSED_MANIFEST_KIND, CONTEXT_SCHEMA, HOST_REPREPARE_COMMAND,
};
use super::identity::{GuestIdentity, GuestIdentityRefusal, GuestSelfObservation};
use super::RunningBinary;

use crate::runner::error::RunnerError;

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
    host_authority_refusal(published, None)
}

/// [`host_authority_unavailable`] for a declared task, naming the fact that
/// keeps it off this container.
pub(crate) fn host_authority_unavailable_for(
    published: &PublishedLegacyGuestContext,
    reason: &str,
) -> String {
    host_authority_refusal(published, Some(reason))
}

fn host_authority_refusal(published: &PublishedLegacyGuestContext, reason: Option<&str>) -> String {
    format!(
        "this container runs the workspace's production context (legacy schema `{CONTEXT_SCHEMA}`, \
         generation {}{}), which pins no admitted workspace session and no host authority channel, \
         so this command has to run on the host in `{}`. Re-preparing the workspace \
         (`{HOST_REPREPARE_COMMAND}`) republishes the same legacy document and cannot grant host \
         authority",
        published.generation,
        reason
            .map(|reason| format!(", and {reason}"))
            .unwrap_or_default(),
        published.context.paths.host_checkout
    )
}

/// What a legacy production guest does with one declared task.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LegacyTask {
    /// The task's steps are bound to the workspace container this process is
    /// already in and need no host facility: they run here.
    ContainerLocal,
    /// The task needs host authority. The reason names the declaration that
    /// keeps it off this container.
    HostAuthority(String),
}

/// Decide one declared task's placement for a production guest.
///
/// Everything here is read from the context's pinned composed manifest, never
/// from the writable checkout, so the placement decision is made from the bytes
/// the host published and the guest digest-verified. A referenced task is
/// checked too: a container-bound task whose steps call a host-bound task still
/// needs host authority.
pub(crate) fn classify_task(
    published: &PublishedLegacyGuestContext,
    guest: &mut dyn GuestReader,
    checkout: &mut dyn CheckoutReader,
    task: &TaskInvocation,
) -> Result<LegacyTask, RunnerError> {
    let manifest = pinned_task_manifest(published, guest, checkout)?;
    let Some(declaration) = manifest.tasks.get(&task.name) else {
        return Ok(LegacyTask::HostAuthority(format!(
            "`{}` is not a task in the manifest this workspace's context pins",
            task.name
        )));
    };
    let Some(reason) = host_facility(&manifest, &task.name, declaration) else {
        return Ok(LegacyTask::ContainerLocal);
    };
    Ok(LegacyTask::HostAuthority(reason))
}

/// The manifest the context pins: the rendered composed document when the
/// publication wrote one (the ordinary production path always does), else the
/// pinned checkout manifest. Its digest is checked against the pin before it is
/// read as a task manifest.
fn pinned_task_manifest(
    published: &PublishedLegacyGuestContext,
    guest: &mut dyn GuestReader,
    checkout: &mut dyn CheckoutReader,
) -> Result<TaskManifest, RunnerError> {
    let pin = published
        .context
        .sources
        .iter()
        .find(|pin| pin.kind == COMPOSED_MANIFEST_KIND)
        .or_else(|| {
            published
                .context
                .sources
                .iter()
                .find(|pin| pin.kind == "manifest")
        })
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "the workspace context generation {} pins no manifest, so this guest cannot place \
                 a task; run the command on the host in `{}`",
                published.generation, published.context.paths.host_checkout
            ))
        })?;
    let snapshot = pin.snapshot.as_deref().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "the pinned source `{}` names no guest-readable snapshot; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            pin.name
        ))
    })?;
    let checkout_root = published.context.paths.guest_checkout.trim_end_matches('/');
    let bytes = match checkout_relative(snapshot, checkout_root) {
        Some(relative) => checkout.read_checkout_file(relative)?,
        None => guest.read_file(snapshot)?,
    }
    .ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "the pinned manifest snapshot `{snapshot}` is missing; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace"
        ))
    })?;
    if sha256_text(&bytes) != pin.digest {
        return Err(RunnerError::task_invocation(format!(
            "the pinned manifest snapshot `{snapshot}` is {} but the context pins {}; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            sha256_text(&bytes),
            pin.digest
        )));
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        RunnerError::task_invocation(format!(
            "the pinned manifest snapshot `{snapshot}` is not UTF-8; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace"
        ))
    })?;
    toml::from_str::<TaskManifest>(&text).map_err(|error| {
        RunnerError::task_invocation(format!(
            "the pinned manifest is not readable as a task manifest ({error}), so this guest cannot \
             place a task; run the command on the host in `{}`",
            published.context.paths.host_checkout
        ))
    })
}

/// The first host facility `root` or a task its steps reference needs, phrased
/// as a clause that reads after "no host authority channel, and …". `None`
/// means the whole referenced graph is container-bound and needs nothing from
/// the host.
fn host_facility(
    manifest: &TaskManifest,
    root: &str,
    declaration: &ManifestTask,
) -> Option<String> {
    let mut pending = Vec::new();
    pending.push((root.to_owned(), declaration.clone()));
    let mut seen = BTreeSet::new();
    while let Some((name, task)) = pending.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        if let Some(reason) = declared_host_facility(manifest, &name, &task) {
            return Some(reason);
        }
        for target in referenced_tasks(&task) {
            match manifest.tasks.get(&target) {
                Some(referenced) => pending.push((target, referenced.clone())),
                None => {
                    return Some(format!(
                        "`{name}` runs `{target}`, which the pinned manifest does not declare"
                    ))
                }
            }
        }
    }
    None
}

/// One task's own declaration, as the fact that keeps it off this container.
fn declared_host_facility(
    manifest: &TaskManifest,
    name: &str,
    task: &ManifestTask,
) -> Option<String> {
    if task.run_in == Some(ManifestTaskRunIn::Host) {
        return Some(format!("`{name}` declares `run_in = \"host\"`"));
    }
    if task.container_lifecycle == Some(true) {
        return Some(format!(
            "`{name}` manages the container lifecycle (`container_lifecycle`)"
        ));
    }
    if task.secrets.is_some() {
        return Some(format!("`{name}` needs host secret delivery (`secrets`)"));
    }
    if task.gateway == Some(true) {
        return Some(format!(
            "`{name}` changes the gateway route table (`gateway`)"
        ));
    }
    if task.admission.is_some() {
        return Some(format!(
            "`{name}` needs the host heavy-workload admission gate (`admission`)"
        ));
    }
    if task.durable {
        return Some(format!(
            "`{name}` declares durable descendants that need retained host authority (`durable`)"
        ));
    }
    if task.health_wait == Some(true)
        || task.ready_message.is_some()
        || task.stay_in_shell == Some(true)
        || !task.concurrent.is_empty()
    {
        return Some(format!(
            "`{name}` is a managed session task the host supervises"
        ));
    }
    match resolve_task_execution_binding(manifest, name, task) {
        Ok(Some(ResolvedTaskExecutionBinding::Workspace(binding))) => {
            if is_default_workspace(manifest, &binding) {
                None
            } else {
                Some(format!(
                    "`{name}` is bound to workspace `{}.{}`, not the workspace container this \
                     guest was prepared for",
                    binding.system, binding.workspace
                ))
            }
        }
        Ok(Some(ResolvedTaskExecutionBinding::Host)) => {
            Some(format!("`{name}` is bound to the host"))
        }
        Ok(None) => Some(format!(
            "`{name}` declares no workspace container binding, so it would run on the host"
        )),
        Err(error) => Some(format!("`{name}` has no runnable placement ({error})")),
    }
}

/// Whether a binding names the manifest's default system and workspace. The
/// context was published for the workspace the environment resolved as its
/// default, so that is the one this container is.
fn is_default_workspace(manifest: &TaskManifest, binding: &ResolvedWorkspaceBinding) -> bool {
    let Some(systems) = manifest.systems.as_ref() else {
        return false;
    };
    let default_system = systems.default.as_ref().or_else(|| {
        // A sole declared system is the implicit default, matching the
        // manifest's own binding resolution.
        let mut entries = systems.systems.keys();
        match (entries.next(), entries.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        }
    });
    let Some(default_system) = default_system else {
        return false;
    };
    if binding.system != *default_system {
        return false;
    }
    systems
        .systems
        .get(default_system)
        .and_then(|system| system.default_workspace.as_ref())
        == Some(&binding.workspace)
}

/// The tasks this task's own steps call.
fn referenced_tasks(task: &ManifestTask) -> Vec<String> {
    let Some(run) = task.run.as_ref() else {
        return Vec::new();
    };
    let steps = match run {
        ManifestManagedRun::Command(_) => return Vec::new(),
        ManifestManagedRun::Sequence(steps) => steps,
    };
    steps
        .iter()
        .filter_map(|step| match step {
            ManifestManagedRunStep::Command(_) => None,
            ManifestManagedRunStep::Step(table) => table.task.clone(),
        })
        .collect()
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
