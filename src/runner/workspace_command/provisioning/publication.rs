//! Publish and verify immutable guest executable and context generations.

use super::artifact_acquisition::{
    check_acquired_artifact, prepare_workspace_artifact, HostWorkspaceArtifact,
};
use super::artifact_plan::LinuxWorkspaceTarget;
use super::artifact_receipt::{digest_matches, verify_guest_artifact};
use super::guest_io::{render_workspace_effigy_staging_path, ComposeWorkspaceGuest};
use super::permission_prep::ensure_workspace_permissions_ready;
use super::sources::{
    environment_generation, host_user, operator_workspace_for_policy, workspace_path_map,
    workspace_routes, workspace_source_closure, OperatorWorkspace,
};
use crate::runner::container_command::bounded_phase_error;
use crate::runner::exec_command::{BoundedCommandBudget, BoundedCommandPhase};
use crate::runner::workspace_client::context::{
    binary_dir, binary_path, context_dir, context_path, read_published_context,
    read_published_generation, snapshot_dir, snapshot_path, ContextReadError, GuestBinaryIdentity,
    GuestWorkspace, WorkspaceActivationIdentity, WorkspaceBackendDescriptor,
    WorkspaceChannelLocator, WorkspaceContainerIdentity, WorkspaceContext, WorkspaceContextInput,
    WorkspaceEnrolmentCredential, WorkspaceEnvironmentIdentity, WorkspaceHostIdentity,
    WorkspacePathMap, BINARY_CURRENT_PATH, BINARY_ROOT, CHANNEL_PROTOCOL_VERSION,
    CONTEXT_CURRENT_PATH, ENROL_CREDENTIAL_PATH, ENROL_ROOT, HOST_REPREPARE_COMMAND,
};
use crate::runner::workspace_command::{
    WorkspaceOpProgress, PROBE_CONTAINER_IDENTITY, PROBE_GUEST_PUBLICATION,
};
use crate::runner::RunnerError;
use effigy_containers::EffectiveContainerPolicy;
use effigy_ui::steps::ProgressFrame;
use std::path::{Path, PathBuf};
/// The stable entry path a handoff or terminal runs. It is a symlink into the
/// digest-addressed store, replaced atomically and never edited in place.
const CONTAINER_WORKSPACE_EFFIGY_ENTRY_PATH: &str = "/usr/local/bin/effigy";
const CONTAINER_WORKSPACE_EFFIGY_ACTIVE_VERSION_PATH: &str = "/usr/local/bin/effigy.active-version";
const CONTAINER_WORKSPACE_EFFIGY_INSTALL_IDENTITY_PATH: &str =
    "/usr/local/bin/effigy.install-identity";

/// What one container-mode Prepare published, for the caller and for lane B.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct PreparedWorkspaceGuest {
    pub(in crate::runner) activation: WorkspaceActivationIdentity,
    pub(in crate::runner) binary: GuestBinaryIdentity,
    pub(in crate::runner) context_generation: u64,
    pub(in crate::runner) paths: WorkspacePathMap,
}

/// The host facts a Prepare needs that the environment engine resolved.
pub(in crate::runner) struct WorkspacePrepareFacts<'a> {
    pub(in crate::runner) repo_root: &'a Path,
    pub(in crate::runner) policy: &'a EffectiveContainerPolicy,
    /// `None` in hybrid mode: nothing is provisioned (§8.1).
    pub(in crate::runner) operator_workspace: Option<OperatorWorkspace>,
    /// The session run lane B admitted for this environment (§8.7).
    pub(in crate::runner) session_run_id: Option<String>,
    pub(in crate::runner) session_generation: Option<u64>,
    /// The narrow enrolment credential lane B minted for direct terminals (§8.8).
    pub(in crate::runner) enrolment: Option<WorkspaceEnrolmentCredential>,
    pub(in crate::runner) channel: WorkspaceChannelLocator,
    pub(in crate::runner) backend: WorkspaceBackendDescriptor,
    /// The container identity the host verified before publishing: the full
    /// backend id, plus the boot and platform the guest reported.
    pub(in crate::runner) container: WorkspaceContainerIdentity,
    /// The activation owner's typed fence (§8.2), supplied while its gate is
    /// held. Its generation is the one the completed activation will publish.
    pub(in crate::runner) environment: effigy_kernel::run::Fence,
    /// The install identity of the produced artifact this Prepare consumed. It
    /// is the producer's own identity, never minted by the consumer.
    pub(in crate::runner) artifact_install_identity: Option<String>,
}

pub(in crate::runner) struct LiveWorkspaceEntry {
    pub(in crate::runner) environment: WorkspaceEnvironmentIdentity,
}

/// Publish the pinned executable, snapshots, context and optional credential
/// into the declared guest (§8.2, §8.9). No source refresh, enrolment or host
/// state write occurs; hybrid mode publishes nothing.
pub(in crate::runner) fn prepare_workspace_guest(
    guest: &mut dyn GuestWorkspace,
    artifact: &Path,
    target: LinuxWorkspaceTarget,
    facts: WorkspacePrepareFacts<'_>,
) -> Result<Option<PreparedWorkspaceGuest>, RunnerError> {
    let Some(workspace) = facts.operator_workspace.as_ref() else {
        return Ok(None);
    };
    let progress = WorkspaceOpProgress::start(false, PROBE_GUEST_PUBLICATION, false);
    progress.update(|_| ProgressFrame::new("verifying guest artifact"));
    let result = (|| {
        let binary = verify_guest_artifact(artifact, target)?;
        let paths = workspace_path_map(facts.repo_root, workspace)?;
        let activation = WorkspaceActivationIdentity {
            host_user: host_user(),
            checkout_id: crate::runner::task_engine::checkout::mint(facts.repo_root)?.text(),
            environment: WorkspaceEnvironmentIdentity {
                resource_id: format!("env:{}", facts.policy.name),
                generation: facts.environment.generation.get().to_string(),
            },
            container: facts.container.clone(),
        };
        let closure = workspace_source_closure(facts.repo_root, &paths)?;
        let mut context = WorkspaceContext::new(WorkspaceContextInput {
            generation: 1,
            host: WorkspaceHostIdentity {
                version: effigy_core::build_info::active_version(),
                protocol: CHANNEL_PROTOCOL_VERSION,
            },
            binary: binary.clone(),
            activation: activation.clone(),
            session_run_id: facts.session_run_id.clone(),
            session_generation: facts.session_generation,
            paths: paths.clone(),
            backend: facts.backend.clone(),
            sources: closure.pins,
            routes: workspace_routes(facts.policy),
            channel: facts.channel.clone(),
        });
        // Publish executable and snapshots before the context that names them.
        progress.update(|_| {
            ProgressFrame::new("installing pinned guest binary")
                .event(Some("guest artifact verified".to_owned()))
        });
        install_pinned_guest_binary(
            guest,
            artifact,
            &binary,
            facts.artifact_install_identity.as_deref(),
        )?;
        progress.update(|_| {
            ProgressFrame::new("publishing source snapshots")
                .event(Some("guest binary installed".to_owned()))
        });
        publish_source_snapshots(guest, &closure.snapshots)?;
        // A restart or replacement outside Effigy's gate may race container I/O.
        // Never publish a context/receipt for a different identity than we probed.
        progress.update(|_| {
            ProgressFrame::new("probing workspace container identity")
                .waiting_for(PROBE_CONTAINER_IDENTITY)
        });
        let observed = guest.probe_container_identity()?;
        if !activation.container.container_matches(&observed)
            || !activation.container.boot_matches(&observed)
        {
            return Err(RunnerError::task_invocation(format!(
                "workspace identity changed during publication; run `{HOST_REPREPARE_COMMAND}` on the host"
            )));
        }
        progress.update(|_| {
            ProgressFrame::new("publishing workspace context")
                .detail_event(format!("container `{}`", observed.container_id))
                .waiting_for(PROBE_GUEST_PUBLICATION)
        });
        let context_generation = publish_context_generation(guest, &mut context)?;
        if let Some(enrolment) = facts.enrolment.as_ref() {
            progress.update(|_| {
                ProgressFrame::new("publishing enrolment credential")
                    .event(Some("workspace context published".to_owned()))
            });
            publish_enrolment_credential(guest, &workspace.user, enrolment)?;
        }
        Ok(Some(PreparedWorkspaceGuest {
            activation,
            binary,
            context_generation,
            paths,
        }))
    })();
    match &result {
        Ok(_) => progress.finish_clear(),
        Err(error) => progress.fail_error(error),
    }
    result
}

/// Open an activation for the handoff, or reuse the caller's owner below.
/// Acquire its artifact before admission; hybrid mode publishes nothing.
pub(super) fn publish_workspace_guest(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<PreparedWorkspaceGuest>, RunnerError> {
    if std::env::var_os("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF").is_some() {
        return Ok(None);
    }
    if operator_workspace_for_policy(repo_root, policy)?.is_none() {
        return Ok(None);
    }
    // Acquire before admission; publication consumes only prepared artifacts.
    let artifact = prepare_workspace_artifact(repo_root, policy)?;
    let ctx = crate::runner::environment::context(repo_root)?;
    let env = crate::runner::environment::env_ref(policy)?;
    let mut prepared = None;
    effigy_containers::environment::activate_with(&ctx, &env, &[], |run| {
        prepared = publish_workspace_guest_in(repo_root, policy, run, artifact.as_ref()).map_err(
            |error| effigy_containers::environment::EnvError::Invalid(error.to_string()),
        )?;
        Ok(())
    })
    .map_err(crate::runner::environment::error)?;
    Ok(prepared)
}

/// The activation run commits the publication intent before this publisher
/// performs any guest I/O, and records its receipt before attaching holders.
pub(super) fn publish_workspace_guest_in(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    run: &mut effigy_containers::environment::EnvRun<'_>,
    artifact: Option<&HostWorkspaceArtifact>,
) -> Result<Option<PreparedWorkspaceGuest>, RunnerError> {
    if std::env::var_os("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF").is_some() {
        return Ok(None);
    }
    let Some(operator_workspace) = operator_workspace_for_policy(repo_root, policy)? else {
        return Ok(None);
    };
    let artifact = artifact.ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "the workspace guest artifact was not prepared before activation; run `{HOST_REPREPARE_COMMAND}` on the host"
        ))
    })?;
    let expected = effigy_containers::environment::EnvRef::for_policy(policy)
        .map_err(crate::runner::environment::error)?;
    if run.fence().resource != expected.resource {
        return Err(RunnerError::task_invocation(
            "workspace publication belongs to another environment owner",
        ));
    }
    let mut prepared = None;
    run.publish_guest_with(|fence| {
        let result =
            publish_workspace_guest_effect(repo_root, policy, operator_workspace, artifact, fence)
                .map_err(|error| {
                    effigy_containers::environment::BackendError::Uncertain(error.to_string())
                })?;
        let receipt = effigy_kernel::run::GuestPublicationReceipt {
            environment: fence.clone(),
            container_id: result.activation.container.container_id.clone(),
            vm_boot_id: result.activation.container.vm_boot_id.clone(),
            artifact_digest: result.binary.digest.clone(),
            context_generation: result.context_generation,
        };
        prepared = Some(result);
        Ok(receipt)
    })
    .map_err(crate::runner::environment::error)?;
    Ok(prepared)
}

fn publish_workspace_guest_effect(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    operator_workspace: OperatorWorkspace,
    artifact: &HostWorkspaceArtifact,
    fence: &effigy_kernel::run::Fence,
) -> Result<PreparedWorkspaceGuest, RunnerError> {
    let mut guest = ComposeWorkspaceGuest::unbounded(repo_root, policy)?;
    // All guest I/O uses the host-verified full backend ID.
    let progress = WorkspaceOpProgress::start(false, PROBE_CONTAINER_IDENTITY, false);
    progress.update(|_| ProgressFrame::new("probing workspace container identity"));
    let bound = match guest.probe_container_identity() {
        Ok(bound) => bound,
        Err(error) => {
            progress.fail_error(&error);
            return Err(error);
        }
    };
    progress.update(|_| {
        ProgressFrame::new("probing workspace container identity")
            .detail_event(format!("container `{}`", bound.container_id))
    });
    progress.finish_clear();
    let target = LinuxWorkspaceTarget::from_image(&bound.platform, &bound.libc)?;
    check_acquired_artifact(artifact, target)?;
    prepare_workspace_guest(
        &mut guest,
        &artifact.artifact,
        target,
        WorkspacePrepareFacts {
            repo_root,
            policy,
            operator_workspace: Some(operator_workspace),
            // Session admission owns enrolment and republishes this context (§8.2, §8.7).
            session_run_id: None,
            session_generation: None,
            enrolment: None,
            channel: WorkspaceChannelLocator::default(),
            backend: WorkspaceBackendDescriptor {
                kind: crate::runner::environment::backend(repo_root, policy)
                    .kind
                    .label()
                    .to_owned(),
                profile: policy.profile.clone(),
                runtime: None,
                vm_type: None,
            },
            container: bound,
            // Use the held owner's publication fence, not a later record read.
            environment: fence.clone(),
            artifact_install_identity: Some(artifact.install_identity.clone()),
        },
    )?
    .ok_or_else(|| RunnerError::task_invocation("declared workspace publication returned no guest"))
}

/// Prepare from the resolved policy: the workspace handoff's Prepare path.
pub(super) fn ensure_workspace_provisioning_ready(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    publish_workspace_guest(workspace_repo_root, policy)?;
    ensure_workspace_permissions_ready(workspace_repo_root, policy, container_name, repo_override)
}

/// Verify the prepared executable and its live identity at entry (§8.9).
/// Entry never acquires, builds, reinstalls or runs a producer. Missing or stale
/// context names host re-prepare; a hybrid environment needs no guest.
pub(super) fn ensure_workspace_effigy_available_for_policy(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    _repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    if std::env::var_os("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF").is_some() {
        return Ok(());
    }
    if operator_workspace_for_policy(workspace_repo_root, policy)?.is_none() {
        return Ok(());
    }
    let live = LiveWorkspaceEntry {
        environment: environment_generation(policy)?,
    };
    let mut guest = ComposeWorkspaceGuest::unbounded(workspace_repo_root, policy)?;
    verify_pinned_guest_binary(&mut guest, &live)
}

pub(super) fn ensure_workspace_effigy_available_for_policy_bounded(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    target_id: &str,
    budget: &mut BoundedCommandBudget,
) -> Result<(), RunnerError> {
    if operator_workspace_for_policy(workspace_repo_root, policy)?.is_some() {
        let live = LiveWorkspaceEntry {
            environment: environment_generation(policy)?,
        };
        let verified = {
            let mut guest =
                ComposeWorkspaceGuest::bounded(workspace_repo_root, policy, target_id, budget);
            verify_pinned_guest_binary(&mut guest, &live)
        };
        if let Err(error) = verified {
            // Preserve bounded-capture detail; restate only verification refusals.
            if error.json_error_details().is_some() {
                return Err(error);
            }
            // Keep bounded diagnostics static; never echo container-controlled values.
            return Err(bounded_phase_error(
                policy,
                policy.primary_service.as_str(),
                budget,
                BoundedCommandPhase::WorkspacePreparation,
                "workspace_preparation_failed",
                "bounded container command could not verify the prepared workspace guest; run \
                 `effigy container up` on the host to re-prepare the workspace",
                None,
            ));
        }
    }
    Ok(())
}

/// Compare published pins with the host version, executable bytes and live
/// container, boot and environment generation. Reads only.
pub(super) fn verify_pinned_guest_binary(
    guest: &mut dyn GuestWorkspace,
    live: &LiveWorkspaceEntry,
) -> Result<(), RunnerError> {
    let published = read_published_context(guest)
        .map_err(context_read_error)?
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "this workspace has no prepared guest context; run `{HOST_REPREPARE_COMMAND}` on \
                 the host to prepare it"
            ))
        })?;
    let context = &published.context;
    let host_version = effigy_core::build_info::active_version();
    if context.host.version != host_version || context.binary.version != host_version {
        return Err(RunnerError::task_invocation(format!(
            "workspace context generation {} pins effigy {} (host {}) but this host runs {}; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            published.generation, context.binary.version, context.host.version, host_version
        )));
    }
    if context.binary.protocol != CHANNEL_PROTOCOL_VERSION {
        return Err(RunnerError::task_invocation(format!(
            "workspace context generation {} pins channel protocol {} but this client speaks {}; \
             run `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            published.generation, context.binary.protocol, CHANNEL_PROTOCOL_VERSION
        )));
    }
    let digest = context.binary.digest.clone();
    let pinned = guest
        .read_file(BINARY_CURRENT_PATH)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned());
    if pinned.as_deref() != Some(digest.as_str()) {
        return Err(RunnerError::task_invocation(format!(
            "workspace executable pointer `{BINARY_CURRENT_PATH}` names {} but context generation \
             {} pins {}; run `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            pinned.as_deref().unwrap_or("nothing"),
            published.generation,
            digest
        )));
    }
    // Refuse changed container, VM boot or image identity before any effect.
    let observed = guest.probe_container_identity()?;
    let prepared_for = &context.activation.container;
    if !prepared_for.container_matches(&observed) || !prepared_for.boot_matches(&observed) {
        return Err(RunnerError::task_invocation(format!(
            "workspace context generation {} was prepared for container `{}` on VM boot `{}` \
             ({}), but this is container `{}` on VM boot `{}` ({}); run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            published.generation,
            prepared_for.container_id,
            prepared_for.vm_boot_id,
            prepared_for.platform,
            observed.container_id,
            observed.vm_boot_id,
            observed.platform,
        )));
    }
    // Reactivation advances the environment fence even when files are unchanged.
    if context.activation.environment.generation != live.environment.generation
        || context.activation.environment.resource_id != live.environment.resource_id
    {
        return Err(RunnerError::task_invocation(format!(
            "workspace context generation {} was prepared for environment `{}` generation `{}`, \
             but `{}` is now at generation `{}`; run `{HOST_REPREPARE_COMMAND}` on the host to \
             re-prepare the workspace",
            published.generation,
            context.activation.environment.resource_id,
            context.activation.environment.generation,
            live.environment.resource_id,
            live.environment.generation,
        )));
    }
    let executable = binary_path(&digest);
    let installed = guest
        .file_sha256(&executable)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    match installed {
        None => Err(RunnerError::task_invocation(format!(
            "workspace executable `{executable}` that context generation {} pins is missing; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            published.generation
        ))),
        Some(digest_hex) if !digest_matches(&digest, &digest_hex) => {
            Err(RunnerError::task_invocation(format!(
                "workspace executable `{executable}` is sha256:{digest_hex} but context generation \
                 {} pins {digest}; run `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the \
                 workspace",
                published.generation
            )))
        }
        Some(_) => {
            // Authenticate the stable entry as well as its immutable target.
            let entry = CONTAINER_WORKSPACE_EFFIGY_ENTRY_PATH;
            let target = guest.read_link(entry)?;
            if target.as_deref() != Some(executable.as_str()) {
                return Err(RunnerError::task_invocation(format!(
                    "workspace entry `{entry}` does not point to pinned executable `{executable}`; \
                     run `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace"
                )));
            }
            let installed = guest.file_sha256(entry)?;
            if !installed
                .as_deref()
                .is_some_and(|hex| digest_matches(&digest, hex))
            {
                return Err(RunnerError::task_invocation(format!(
                    "workspace entry `{entry}` does not contain pinned executable {digest}; run \
                     `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace"
                )));
            }
            Ok(())
        }
    }
}

/// Keep a guest transport or capture failure (which carries its own bounded
/// detail) and restate only a published-content refusal.
pub(super) fn context_read_error(error: ContextReadError) -> RunnerError {
    match error {
        ContextReadError::Guest(error) => error,
        ContextReadError::Refused(refusal) => RunnerError::task_invocation(refusal.to_string()),
    }
}

/// Publish a complete immutable generation, then switch `current`. Identical
/// content reuses its generation; changed content gets a new directory.
fn publish_context_generation(
    guest: &mut dyn GuestWorkspace,
    context: &mut WorkspaceContext,
) -> Result<u64, RunnerError> {
    let current = read_published_generation(guest).map_err(context_read_error)?;
    if let Some(current) = current {
        context.generation = current;
        let existing = guest.read_file(&context_path(current))?;
        if existing.as_deref() == Some(context.render()?.as_slice()) {
            return Ok(current);
        }
    }
    let mut generation = current.unwrap_or(0);
    let body = loop {
        generation = generation.checked_add(1).ok_or_else(|| {
            RunnerError::task_invocation("workspace context generation exhausted")
        })?;
        context.generation = generation;
        let body = context.render()?;
        match guest.read_file(&context_path(generation))? {
            Some(existing) if existing == body => {
                guest.replace_pointer(CONTEXT_CURRENT_PATH, &generation.to_string())?;
                return Ok(generation);
            }
            Some(_) => continue, // retained orphan from an interrupted publication
            None => break body,
        }
    };
    guest.ensure_dir(&context_dir(generation), 0o755)?;
    guest.write_file(&context_path(generation), &body, 0o444)?;
    // Seal the generation against workspace-user writes.
    guest.ensure_dir(&context_dir(generation), 0o555)?;
    guest.replace_pointer(CONTEXT_CURRENT_PATH, &generation.to_string())?;
    Ok(generation)
}

/// Install once into the digest-addressed guest store, then switch entry
/// pointers. Older runs retain the exact executable bytes they resolved (§8.9).
fn install_pinned_guest_binary(
    guest: &mut dyn GuestWorkspace,
    artifact: &Path,
    binary: &GuestBinaryIdentity,
    install_identity: Option<&str>,
) -> Result<(), RunnerError> {
    let executable = binary_path(&binary.digest);
    let installed = guest.file_sha256(&executable)?;
    if installed
        .as_deref()
        .is_some_and(|hex| !digest_matches(&binary.digest, hex))
    {
        return Err(RunnerError::task_invocation(format!(
            "immutable workspace executable `{executable}` has different bytes; run `{HOST_REPREPARE_COMMAND}` with a fresh container"
        )));
    }
    if !installed
        .as_deref()
        .is_some_and(|hex| digest_matches(&binary.digest, hex))
    {
        let staging = render_workspace_effigy_staging_path();
        guest.ensure_dir(BINARY_ROOT, 0o755)?;
        guest.ensure_dir(&binary_dir(&binary.digest), 0o755)?;
        guest.stage_host_file(artifact, &staging)?;
        guest.install_executable(&staging, &executable)?;
        guest.ensure_dir(&binary_dir(&binary.digest), 0o555)?;
        // The bytes the guest will run must be the bytes the host pinned.
        let copied = guest.file_sha256(&executable)?;
        if !copied
            .as_deref()
            .is_some_and(|hex| digest_matches(&binary.digest, hex))
        {
            return Err(RunnerError::task_invocation(format!(
                "installed workspace executable `{executable}` does not match the pinned digest \
                 {}; the container refused the copy",
                binary.digest
            )));
        }
    }
    guest.replace_pointer(BINARY_CURRENT_PATH, &binary.digest)?;
    guest.replace_link(CONTAINER_WORKSPACE_EFFIGY_ENTRY_PATH, &executable)?;
    // Receipts the existing handoff and status probes read.
    guest.replace_pointer(
        CONTAINER_WORKSPACE_EFFIGY_ACTIVE_VERSION_PATH,
        &binary.version,
    )?;
    if let Some(install_identity) = install_identity {
        guest.replace_pointer(
            CONTAINER_WORKSPACE_EFFIGY_INSTALL_IDENTITY_PATH,
            install_identity,
        )?;
    }
    Ok(())
}

/// Publish the enrolment-only credential readable solely by its workspace user (§8.8).
fn publish_enrolment_credential(
    guest: &mut dyn GuestWorkspace,
    workspace_user: &str,
    credential: &WorkspaceEnrolmentCredential,
) -> Result<(), RunnerError> {
    guest.ensure_dir(ENROL_ROOT, 0o700)?;
    guest.write_file(
        ENROL_CREDENTIAL_PATH,
        credential.file_body().as_bytes(),
        0o600,
    )?;
    guest.set_owner(ENROL_ROOT, workspace_user)?;
    guest.set_owner(ENROL_CREDENTIAL_PATH, workspace_user)?;
    Ok(())
}

/// Publish immutable snapshots before the context that names their digests.
fn publish_source_snapshots(
    guest: &mut dyn GuestWorkspace,
    snapshots: &[(String, Vec<u8>)],
) -> Result<(), RunnerError> {
    for (digest, bytes) in snapshots {
        let directory = snapshot_dir(digest);
        let path = snapshot_path(digest);
        guest.ensure_dir(&directory, 0o755)?;
        match guest.file_sha256(&path)? {
            Some(hex) if !digest_matches(digest, &hex) => return Err(RunnerError::task_invocation(
                format!("immutable workspace snapshot `{path}` has different bytes; run `{HOST_REPREPARE_COMMAND}` with a fresh container")
            )),
            Some(_) => {},
            None => guest.write_file(&path, bytes, 0o444)?,
        }
        guest.ensure_dir(&directory, 0o555)?;
    }
    Ok(())
}
