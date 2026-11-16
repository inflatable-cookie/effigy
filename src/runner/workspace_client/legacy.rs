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
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use effigy_cli::TaskInvocation;
use effigy_manifest::{
    resolve_task_execution_binding, ManifestManagedRun, ManifestManagedRunStep, ManifestTask,
    ManifestTaskRunIn, ResolvedTaskExecutionBinding, ResolvedWorkspaceContainer, TaskManifest,
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
    host_authority_refusal(
        published.generation,
        &published.context.paths.host_checkout,
        None,
    )
}

/// [`host_authority_unavailable`] for a declared task, naming the fact that
/// keeps it off this container.
pub(crate) fn host_authority_unavailable_for(
    published: &PublishedLegacyGuestContext,
    reason: &str,
) -> String {
    host_authority_refusal(
        published.generation,
        &published.context.paths.host_checkout,
        Some(reason),
    )
}

fn host_authority_refusal(generation: u64, host_checkout: &str, reason: Option<&str>) -> String {
    format!(
        "this container runs the workspace's production context (legacy schema `{CONTEXT_SCHEMA}`, \
         generation {}{}), which pins no admitted workspace session and no host authority channel, \
         so this command has to run on the host in `{host_checkout}`. Re-preparing the workspace \
         (`{HOST_REPREPARE_COMMAND}`) republishes the same legacy document and cannot grant host \
         authority",
        generation,
        reason
            .map(|reason| format!(", and {reason}"))
            .unwrap_or_default(),
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
    let prepared = prepared_container(published)?;
    let Some(reason) = host_facility(&manifest, &task.name, declaration, prepared) else {
        return Ok(LegacyTask::ContainerLocal);
    };
    Ok(LegacyTask::HostAuthority(reason))
}

/// The container this context was prepared for, from the environment resource
/// `env:<policy name>` the publication records. A context that does not name one
/// cannot be checked against a task's binding, so it refuses rather than guess.
pub(crate) fn prepared_container(
    published: &PublishedLegacyGuestContext,
) -> Result<&str, RunnerError> {
    published
        .context
        .activation
        .environment
        .resource_id
        .strip_prefix("env:")
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "the workspace context generation {} names no prepared container \
                 (`{}`), so this guest cannot tell a container-bound task from a host one; run the \
                 command on the host in `{}`",
                published.generation,
                published.context.activation.environment.resource_id,
                published.context.paths.host_checkout
            ))
        })
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
    prepared_container: &str,
) -> Option<String> {
    let mut pending = Vec::new();
    pending.push((root.to_owned(), declaration.clone()));
    let mut seen = BTreeSet::new();
    while let Some((name, task)) = pending.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        if let Some(reason) = declared_host_facility(manifest, &name, &task, prepared_container) {
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
    prepared_container: &str,
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
            match binding.container.as_ref() {
                Some(ResolvedWorkspaceContainer::Named(container))
                    if container == prepared_container =>
                {
                    None
                }
                Some(ResolvedWorkspaceContainer::Named(container)) => Some(format!(
                    "`{name}` is bound to container `{container}`, not the `{prepared_container}` \
                     container this guest was prepared for"
                )),
                Some(ResolvedWorkspaceContainer::Inline(inline)) => Some(format!(
                    "`{name}` is bound to inline workspace container `{}`, not the \
                     `{prepared_container}` container this guest was prepared for",
                    inline.synthetic_name
                )),
                None => Some(format!(
                    "`{name}`'s workspace `{}.{}` declares no backing container",
                    binding.system, binding.workspace
                )),
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

/// The one legacy placement this process accepted for a local task run.
///
/// The guest classifies from the immutable composed snapshot, then the ordinary
/// dispatch resolves the definition from the checkout. This record is what lets
/// the dispatch re-check the definition it actually resolved, so a checkout
/// manifest that changed after publication, or after classification, cannot
/// substitute a task (planner ruling 2026-11-16; review finding 1).
#[derive(Debug, Clone)]
pub(crate) struct LocalTaskPlacement {
    /// The container the context was prepared for (the `env:<name>` resource).
    pub(crate) container: String,
    /// The context generation the placement was proved under.
    pub(crate) generation: u64,
    /// The host checkout the refusal names.
    pub(crate) host_checkout: String,
    /// Every checkout-visible file the context pins: the root manifest, the
    /// source lock and each included manifest, with the path it mounts at.
    pub(crate) checkout_pins: Vec<(PathBuf, String)>,
    /// The checkout manifest the ordinary dispatch will read.
    pub(crate) manifest_path: PathBuf,
    /// The task definitions the context pinned, as the runner will compare them.
    pub(crate) pinned_tasks: BTreeMap<String, ManifestTask>,
}

thread_local! {
    /// One command runs on one thread, and the dispatch that follows the guest
    /// entry is the same call stack, so the placement is thread-scoped: a
    /// parallel test or another command on another thread never inherits it.
    static LOCAL_TASK_PLACEMENT: RefCell<Option<LocalTaskPlacement>> = const { RefCell::new(None) };
}

/// Record the placement this process may run a task under, after refusing a
/// checkout manifest that no longer matches its pin.
pub(crate) fn remember_local_task_placement(
    published: &PublishedLegacyGuestContext,
    guest: &mut dyn GuestReader,
    checkout: &mut dyn CheckoutReader,
) -> Result<LocalTaskPlacement, RunnerError> {
    let pinned_manifest = pinned_task_manifest(published, guest, checkout)?;
    let checkout_root = published.context.paths.guest_checkout.trim_end_matches('/');
    let checkout_pins = published
        .context
        .sources
        .iter()
        .filter_map(|pin| {
            let relative = checkout_relative(pin.snapshot.as_deref()?, checkout_root)?;
            Some((
                Path::new(&published.context.paths.guest_checkout).join(relative),
                pin.digest.clone(),
            ))
        })
        .collect::<Vec<_>>();
    let placement = LocalTaskPlacement {
        container: prepared_container(published)?.to_owned(),
        generation: published.generation,
        host_checkout: published.context.paths.host_checkout.clone(),
        checkout_pins,
        manifest_path: Path::new(&published.context.paths.guest_checkout).join("effigy.toml"),
        pinned_tasks: pinned_manifest.tasks,
    };
    verify_checkout_manifest_through(checkout, &placement)?;
    LOCAL_TASK_PLACEMENT.with(|slot| *slot.borrow_mut() = Some(placement.clone()));
    Ok(placement)
}

/// Forget the recorded placement. The guest entry records one placement per
/// command; a host that keeps the process alive across commands (and a proof
/// that shares a thread with another one) clears it here.
pub(crate) fn forget_local_task_placement() {
    LOCAL_TASK_PLACEMENT.with(|slot| *slot.borrow_mut() = None);
}

/// The placement recorded for this process, if it is a production guest running
/// a container-bound task.
pub(crate) fn local_task_placement() -> Option<LocalTaskPlacement> {
    LOCAL_TASK_PLACEMENT.with(|slot| slot.borrow().clone())
}

/// The guest entry's check: every checkout-visible file the context pinned is
/// still the bytes that were pinned. A checkout that changed after Prepare
/// refuses before the run records a placement, whatever file changed.
fn verify_checkout_manifest_through(
    checkout: &mut dyn CheckoutReader,
    placement: &LocalTaskPlacement,
) -> Result<(), RunnerError> {
    let checkout_root = placement.manifest_path.parent().unwrap_or(Path::new("/"));
    for (path, pinned) in &placement.checkout_pins {
        let relative = path.strip_prefix(checkout_root).unwrap_or(path);
        let relative = relative.to_string_lossy().replace('\\', "/");
        let bytes = checkout.read_checkout_file(&relative)?;
        let Some(bytes) = bytes else {
            return Err(RunnerError::task_invocation(format!(
                "the pinned checkout file `{}` this task would run from is missing; run the command \
                 on the host in `{}`",
                path.display(),
                placement.host_checkout
            )));
        };
        let actual = sha256_text(&bytes);
        if actual != *pinned {
            return Err(RunnerError::task_invocation(format!(
                "the pinned checkout file `{}` is {actual} but workspace context generation {} pins \
                 {pinned}, so this container would run a task definition the host never approved; \
                 run the command on the host in `{}`, or re-prepare the workspace on the host",
                path.display(),
                placement.generation,
                placement.host_checkout
            )));
        }
    }
    Ok(())
}

/// The dispatch's check, on the real mounted paths, immediately before it
/// prepares sources or resolves the task.
pub(crate) fn verify_checkout_inputs() -> Result<(), RunnerError> {
    let Some(placement) = local_task_placement() else {
        return Ok(());
    };
    for (path, pinned) in &placement.checkout_pins {
        let bytes = std::fs::read(path).map_err(|error| {
            RunnerError::task_invocation(format!(
                "the pinned checkout file `{}` this task would run from cannot be read: {error}; \
                 run the command on the host in `{}`",
                path.display(),
                placement.host_checkout
            ))
        })?;
        let actual = sha256_text(&bytes);
        if actual != *pinned {
            return Err(RunnerError::task_invocation(format!(
                "the pinned checkout file `{}` is {actual} but workspace context generation {} pins \
                 {pinned}, so this container would run a task definition the host never approved; \
                 run the command on the host in `{}`, or re-prepare the workspace on the host",
                path.display(),
                placement.generation,
                placement.host_checkout
            )));
        }
    }
    Ok(())
}

/// Re-check a task the resolving runner actually loaded, before it runs.
///
/// This is the authoritative check: it sees the definition this run resolved
/// from the checkout, so nothing can be substituted between the guest's
/// classification and the run. A task whose declaration (or whose referenced
/// graph) needs a host facility, or that is bound to another container, refuses
/// here even when the guest's earlier classification allowed it.
pub(crate) fn verify_local_task(
    manifest: &TaskManifest,
    task_name: &str,
    task: &ManifestTask,
) -> Result<(), RunnerError> {
    let Some(placement) = local_task_placement() else {
        return Ok(());
    };
    // The definition that runs must be the one the context pinned, task by task
    // through every referenced task. The comparison is on the resolved
    // declarations, so a changed root manifest or a changed included file
    // refuses here instead of running an unapproved command, and the
    // machine-local bundle-input layer cannot make an unchanged definition look
    // changed.
    let mut pending = vec![(task_name.to_owned(), task.clone())];
    let mut seen = BTreeSet::new();
    while let Some((name, resolved)) = pending.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(pinned) = placement.pinned_tasks.get(&name) else {
            return Err(RunnerError::task_invocation(format!(
                "`{name}` is not a task in the manifest workspace context generation {} pins, so \
                 this container will not run it; run the command on the host in `{}`",
                placement.generation, placement.host_checkout
            )));
        };
        if *pinned != resolved {
            return Err(RunnerError::task_invocation(format!(
                "the definition of `{name}` is not the one workspace context generation {} pins: \
                 the checkout, or a file it includes, changed after publication, so this container \
                 will not run it — run the command on the host in `{}`, or re-prepare the \
                 workspace on the host",
                placement.generation, placement.host_checkout
            )));
        }
        for target in referenced_tasks(&resolved) {
            match manifest.tasks.get(&target) {
                Some(referenced) => pending.push((target, referenced.clone())),
                None => {
                    return Err(RunnerError::task_invocation(format!(
                        "`{name}` runs `{target}`, which this checkout no longer declares; run the \
                         command on the host in `{}`",
                        placement.host_checkout
                    )))
                }
            }
        }
    }
    let Some(reason) = host_facility(manifest, task_name, task, &placement.container) else {
        return Ok(());
    };
    Err(RunnerError::task_invocation(host_authority_refusal(
        placement.generation,
        &placement.host_checkout,
        Some(&reason),
    )))
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
