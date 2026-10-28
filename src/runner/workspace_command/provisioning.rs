use effigy_containers::{load_workspace_ownership_plan, EffectiveContainerPolicy};
use effigy_core::repo_markers::has_task_manifest;
use effigy_core::shell::shell_quote;
use effigy_manifest::TASK_MANIFEST_FILE;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runner::command_context::current_working_dir;
use crate::runner::container_command::{bounded_failure_error, bounded_phase_error};
use crate::runner::exec_command::{
    copy_file_into_service, copy_file_into_service_bounded, BoundedCapture, BoundedCommandBudget,
    BoundedCommandPhase,
};
use crate::runner::workspace_client::context::{
    binary_dir, binary_path, context_dir, context_path, read_published_context,
    read_published_generation, render_digest, ContextReadError, GuestBinaryIdentity, GuestReader,
    GuestWorkspace, WorkspaceActivationIdentity, WorkspaceBackendDescriptor,
    WorkspaceChannelLocator, WorkspaceContainerIdentity, WorkspaceContext, WorkspaceContextInput,
    WorkspaceEnrolmentCredential, WorkspaceEnvironmentIdentity, WorkspaceHostIdentity,
    WorkspacePathMap, WorkspacePinnedSource, WorkspaceRoute, BINARY_CURRENT_PATH, BINARY_ROOT,
    CHANNEL_PROTOCOL_VERSION, CONTEXT_CURRENT_PATH, ENROL_CREDENTIAL_PATH, ENROL_ROOT,
    HOST_REPREPARE_COMMAND,
};

#[cfg(test)]
use crate::runner::exec_command::BoundedCaptureFailure;

use super::RunnerError;

const CONTAINER_WORKSPACE_EFFIGY_STAGING_PATH_PREFIX: &str = "/tmp/effigy-host";
/// The stable entry path a handoff or terminal runs. It is a symlink into the
/// digest-addressed store, replaced atomically and never edited in place.
const CONTAINER_WORKSPACE_EFFIGY_ENTRY_PATH: &str = "/usr/local/bin/effigy";
const CONTAINER_WORKSPACE_EFFIGY_ACTIVE_VERSION_PATH: &str = "/usr/local/bin/effigy.active-version";
const CONTAINER_WORKSPACE_EFFIGY_INSTALL_IDENTITY_PATH: &str =
    "/usr/local/bin/effigy.install-identity";
const EFFIGY_RELEASE_REPO_BASE_URL: &str = "https://github.com/inflatable-cookie/effigy";
pub(super) const EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV: &str =
    "EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE";
static WORKSPACE_EFFIGY_STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);
const LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(300);
const LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LinuxWorkspaceTarget {
    X86_64Gnu,
    Aarch64Gnu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinuxWorkspaceArtifactSource {
    Auto,
    Local,
    Download,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct EffigySourceConfig {
    pub(super) repo_root: String,
}

impl LinuxWorkspaceTarget {
    pub(super) fn release_triple(self) -> &'static str {
        match self {
            Self::X86_64Gnu => "x86_64-unknown-linux-gnu",
            Self::Aarch64Gnu => "aarch64-unknown-linux-gnu",
        }
    }

    pub(super) fn from_machine(machine: &str) -> Option<Self> {
        match machine.trim() {
            "x86_64" | "amd64" => Some(Self::X86_64Gnu),
            "aarch64" | "arm64" => Some(Self::Aarch64Gnu),
            _ => None,
        }
    }

    pub(super) fn artifact_relative_path(self) -> PathBuf {
        PathBuf::from(".effigy/linux-release/artifacts")
            .join(format!("effigy-{}", self.release_triple()))
    }

    pub(super) fn cache_relative_dir(self) -> PathBuf {
        PathBuf::from(".effigy/workspace-bin/linux").join(self.release_triple())
    }
}

pub(super) fn ensure_workspace_permissions_ready(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    let Some(_user) = policy.workspace_user.as_deref() else {
        return Ok(());
    };
    let plan = load_workspace_ownership_plan(policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if plan.targets.is_empty() {
        return Ok(());
    }
    // Mounts are part of the service configuration the one apply reconciles,
    // so permission repair no longer waits on a separate drift probe.
    let allow_repair = true;
    let perm_plan = plan_workspace_permission_prep(&plan.owned_disposable_paths());
    let progress_label = render_workspace_permission_progress_label(&perm_plan);
    let mut progress = super::WorkspaceTransientProgressReporter::new(
        repo_override.is_some(),
        &progress_label,
        false,
    );
    let mut backend = super::permissions::compose_backend(workspace_repo_root, policy);
    super::permissions::ensure_workspace_permissions_ready_with(
        policy,
        container_name,
        allow_repair,
        &mut backend,
    )
    .inspect_err(|_| progress.finish(false))?;
    progress.finish(true);
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WorkspacePermissionMode {
    Recursive,
    Shallow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkspacePermissionTarget {
    pub(super) path: String,
    pub(super) mode: WorkspacePermissionMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkspacePermissionPlan {
    pub(super) targets: Vec<WorkspacePermissionTarget>,
    pub(super) skipped_nested: usize,
    pub(super) shallow_dependency_trees: usize,
}

pub(super) fn plan_workspace_permission_prep(targets: &[String]) -> WorkspacePermissionPlan {
    let mut sorted = targets.to_vec();
    sorted.sort_by(|left, right| {
        path_depth(left)
            .cmp(&path_depth(right))
            .then_with(|| left.cmp(right))
    });

    let mut kept: Vec<WorkspacePermissionTarget> = Vec::new();
    let mut skipped_nested = 0usize;
    let mut shallow_dependency_trees = 0usize;

    for path in sorted {
        if kept.iter().any(|existing| {
            existing.mode == WorkspacePermissionMode::Recursive
                && path_is_under(&path, &existing.path)
        }) {
            skipped_nested += 1;
            continue;
        }

        let mode = if is_dependency_tree_path(&path)
            && kept.iter().any(|existing| {
                existing.mode == WorkspacePermissionMode::Recursive
                    && is_dependency_tree_path(&existing.path)
                    && dependency_tree_is_under_authoritative_root(&path, &existing.path)
            }) {
            shallow_dependency_trees += 1;
            WorkspacePermissionMode::Shallow
        } else {
            WorkspacePermissionMode::Recursive
        };

        kept.push(WorkspacePermissionTarget { path, mode });
    }

    WorkspacePermissionPlan {
        targets: kept,
        skipped_nested,
        shallow_dependency_trees,
    }
}

fn render_workspace_permission_progress_label(plan: &WorkspacePermissionPlan) -> String {
    let recursive = plan
        .targets
        .iter()
        .filter(|target| target.mode == WorkspacePermissionMode::Recursive)
        .count();
    let shallow = plan
        .targets
        .iter()
        .filter(|target| target.mode == WorkspacePermissionMode::Shallow)
        .count();
    if plan.skipped_nested == 0 && plan.shallow_dependency_trees == 0 {
        return format!(
            "preparing workspace permissions ({recursive} path{})",
            if recursive == 1 { "" } else { "s" }
        );
    }
    format!(
        "preparing workspace permissions ({recursive} recursive, {shallow} shallow; skipped {} nested)",
        plan.skipped_nested
    )
}

pub(super) fn is_dependency_tree_path(path: &str) -> bool {
    let trimmed = path.trim_end_matches('/');
    matches!(
        Path::new(trimmed)
            .file_name()
            .and_then(|name| name.to_str()),
        Some("node_modules" | "vendor")
    )
}

fn dependency_tree_is_under_authoritative_root(candidate: &str, authoritative: &str) -> bool {
    let Some(root_parent) = parent_unix_path(authoritative) else {
        return false;
    };
    let Some(candidate_parent) = parent_unix_path(candidate) else {
        return false;
    };
    // Only a strictly nested tree (candidate parent inside the authoritative
    // tree's parent scope) may rely on the authoritative recursive scan.
    // Same-parent siblings (Composer `vendor` next to JS `node_modules` as
    // separate volumes) are distinct mount scopes: neither scan descends into
    // the other, so each sibling keeps its own recursive repair.
    path_is_under(candidate_parent, root_parent)
}

fn parent_unix_path(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some(("", _)) => Some("/"),
        Some((parent, _)) if !parent.is_empty() => Some(parent),
        _ => None,
    }
}

fn path_is_under(path: &str, parent: &str) -> bool {
    let path = path.trim_end_matches('/');
    let parent = parent.trim_end_matches('/');
    if parent.is_empty() || path == parent {
        return false;
    }
    if parent == "/" {
        return path.starts_with('/');
    }
    path.starts_with(parent) && path.as_bytes().get(parent.len()) == Some(&b'/')
}

fn path_depth(path: &str) -> usize {
    path.trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .count()
}

/// The operator workspace a container-mode environment declares (§8.1).
///
/// Hybrid mode declares none. The workspace is never inferred from an
/// environment name, a mode string or a user-exported variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct OperatorWorkspace {
    pub(in crate::runner) user: String,
    pub(in crate::runner) home: String,
    /// Where the checkout is mounted inside the container.
    pub(in crate::runner) guest_checkout: String,
}

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
    /// The narrow enrolment credential lane B minted for direct terminals (§8.8).
    pub(in crate::runner) enrolment: Option<WorkspaceEnrolmentCredential>,
    pub(in crate::runner) channel: WorkspaceChannelLocator,
    pub(in crate::runner) backend: WorkspaceBackendDescriptor,
}

/// Publish the version-locked guest executable and immutable context for one
/// container-mode workspace.
///
/// Nothing here refreshes a pinned source, enrols a session or writes host
/// state: every step is a file effect in the guest, and a hybrid environment
/// (no operator workspace) gets no guest binary, context, listener or
/// credential at all.
pub(in crate::runner) fn prepare_workspace_guest(
    guest: &mut dyn GuestWorkspace,
    artifact: &Path,
    identity: WorkspaceContainerIdentity,
    facts: WorkspacePrepareFacts<'_>,
) -> Result<Option<PreparedWorkspaceGuest>, RunnerError> {
    let Some(workspace) = facts.operator_workspace.as_ref() else {
        return Ok(None);
    };
    let binary = pinned_guest_binary(artifact)?;
    let paths = workspace_path_map(facts.repo_root, workspace)?;
    let activation = WorkspaceActivationIdentity {
        host_user: host_user(),
        checkout_id: crate::runner::task_engine::checkout::mint(facts.repo_root)?.text(),
        environment: WorkspaceEnvironmentIdentity {
            resource_id: format!("env:{}", facts.policy.name),
            generation: environment_generation(facts.repo_root, facts.policy)?,
        },
        container: identity,
    };
    let mut context = WorkspaceContext::new(WorkspaceContextInput {
        generation: 1,
        host: WorkspaceHostIdentity {
            version: effigy_core::build_info::active_version(),
            protocol: CHANNEL_PROTOCOL_VERSION,
        },
        binary: binary.clone(),
        activation: activation.clone(),
        session_run_id: facts.session_run_id,
        paths: paths.clone(),
        backend: facts.backend,
        sources: workspace_source_pins(facts.repo_root, facts.policy)?,
        routes: workspace_routes(facts.policy),
        channel: facts.channel,
    });
    let context_generation = publish_context_generation(guest, &mut context)?;
    install_pinned_guest_binary(
        guest,
        artifact,
        &binary,
        &expected_workspace_effigy_install_identity(facts.repo_root)?,
    )?;
    if let Some(enrolment) = facts.enrolment.as_ref() {
        publish_enrolment_credential(guest, &workspace.user, enrolment)?;
    }
    Ok(Some(PreparedWorkspaceGuest {
        activation,
        binary,
        context_generation,
        paths,
    }))
}

/// Prepare from the resolved policy: the workspace handoff's Prepare path.
pub(super) fn ensure_workspace_provisioning_ready(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    if std::env::var_os("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF").is_none() {
        let operator_workspace = operator_workspace_for_policy(workspace_repo_root, policy)?;
        if let Some(workspace) = operator_workspace.as_ref() {
            let mut guest = ComposeWorkspaceGuest::unbounded(workspace_repo_root, policy);
            let identity = guest.probe_container_identity()?;
            let target =
                LinuxWorkspaceTarget::from_machine(&identity.platform).ok_or_else(|| {
                    RunnerError::task_invocation(format!(
                        "workspace container image platform `{}` has no pinned Linux guest \
                         artifact; run `{HOST_REPREPARE_COMMAND}` with a supported image",
                        identity.platform
                    ))
                })?;
            let artifact = ensure_linux_workspace_effigy_artifact(workspace_repo_root, target)?;
            prepare_workspace_guest(
                &mut guest,
                &artifact,
                identity,
                WorkspacePrepareFacts {
                    repo_root: workspace_repo_root,
                    policy,
                    operator_workspace: Some(workspace.clone()),
                    // The session run and its enrolment credential belong to
                    // the workspace session lane; this Prepare publishes the
                    // context they are bound to and the host republishes it
                    // when the session is admitted (§8.2, §8.7).
                    session_run_id: None,
                    enrolment: None,
                    channel: WorkspaceChannelLocator::default(),
                    backend: WorkspaceBackendDescriptor {
                        kind: crate::runner::environment::backend(workspace_repo_root, policy)
                            .kind
                            .label()
                            .to_owned(),
                        profile: policy.profile.clone(),
                        runtime: None,
                        vm_type: None,
                    },
                },
            )?;
        }
    }
    ensure_workspace_permissions_ready(workspace_repo_root, policy, container_name, repo_override)
}

/// Entry path: verify the pinned guest executable the published context names.
///
/// This is not a Prepare. It never downloads, builds or reinstalls the guest
/// (§8.9); a workspace without a prepared context, or with one that pins
/// another version or a missing executable, refuses and names the host
/// re-prepare command.
pub(super) fn ensure_workspace_effigy_available_for_policy(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    _repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    if std::env::var_os("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF").is_some() {
        return Ok(());
    }
    let mut guest = ComposeWorkspaceGuest::unbounded(workspace_repo_root, policy);
    verify_pinned_guest_binary(&mut guest)
}

pub(super) fn ensure_workspace_effigy_available_for_policy_bounded(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    target_id: &str,
    budget: &mut BoundedCommandBudget,
) -> Result<(), RunnerError> {
    let verified = {
        let mut guest =
            ComposeWorkspaceGuest::bounded(workspace_repo_root, policy, target_id, budget);
        verify_pinned_guest_binary(&mut guest)
    };
    verified.map_err(|error| {
        // A bounded capture failure (deadline, stream limit) already carries
        // its own phase detail; only a verification refusal is restated here.
        if error.json_error_details().is_some() {
            return error;
        }
        // Bounded failures carry only static text, so the refusal names the
        // phase and the host re-prepare rather than a version, a digest or a
        // path from the container.
        bounded_phase_error(
            policy,
            policy.primary_service.as_str(),
            budget,
            BoundedCommandPhase::WorkspacePreparation,
            "workspace_preparation_failed",
            "bounded container command could not verify the prepared workspace guest; run \
             `effigy container up` on the host to re-prepare the workspace",
            None,
        )
    })
}

/// Check the published context against the host's own version and the guest's
/// installed executable. Pure reads: no write, no build, no download.
pub(super) fn verify_pinned_guest_binary(guest: &mut dyn GuestReader) -> Result<(), RunnerError> {
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
    if guest
        .read_file(&binary_path(&digest))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .is_none()
    {
        return Err(RunnerError::task_invocation(format!(
            "workspace executable `{}` that context generation {} pins is missing; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            binary_path(&digest),
            published.generation
        )));
    }
    Ok(())
}

/// Keep a guest transport or capture failure (which carries its own bounded
/// detail) and restate only a published-content refusal.
fn context_read_error(error: ContextReadError) -> RunnerError {
    match error {
        ContextReadError::Guest(error) => error,
        ContextReadError::Refused(refusal) => RunnerError::task_invocation(refusal.to_string()),
    }
}

/// Publish one complete generation and switch `current`.
///
/// A published generation is never edited: an identical document (ignoring the
/// generation it was built as) reuses the generation that already carries it,
/// and any change writes a new directory.
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
    let generation = current.map_or(1, |generation| generation.saturating_add(1));
    context.generation = generation;
    let body = context.render()?;
    guest.ensure_dir(&context_dir(generation), 0o755)?;
    guest.write_file(&context_path(generation), &body, 0o444)?;
    // Sealed after the document is written: the workspace user can read the
    // generation and cannot change it.
    guest.ensure_dir(&context_dir(generation), 0o555)?;
    guest.replace_pointer(CONTEXT_CURRENT_PATH, &generation.to_string())?;
    Ok(generation)
}

/// Install the pinned artifact into the guest-private store and switch the
/// pointers new invocations resolve.
///
/// A digest-addressed directory is written once and never rewritten, so a run
/// that resolved an older generation keeps the exact bytes it started with
/// (§8.9 "never replaces the live executable under an active run").
fn install_pinned_guest_binary(
    guest: &mut dyn GuestWorkspace,
    artifact: &Path,
    binary: &GuestBinaryIdentity,
    install_identity: &Option<String>,
) -> Result<(), RunnerError> {
    let executable = binary_path(&binary.digest);
    if guest.read_file(&executable)?.is_none() {
        let staging = render_workspace_effigy_staging_path();
        guest.ensure_dir(BINARY_ROOT, 0o755)?;
        guest.ensure_dir(&binary_dir(&binary.digest), 0o755)?;
        guest.stage_host_file(artifact, &staging)?;
        guest.install_executable(&staging, &executable)?;
        guest.ensure_dir(&binary_dir(&binary.digest), 0o555)?;
    }
    guest.replace_pointer(BINARY_CURRENT_PATH, &binary.digest)?;
    guest.replace_link(CONTAINER_WORKSPACE_EFFIGY_ENTRY_PATH, &executable)?;
    // Receipts the existing handoff and status probes read.
    guest.replace_pointer(
        CONTAINER_WORKSPACE_EFFIGY_ACTIVE_VERSION_PATH,
        &binary.version,
    )?;
    if let Some(install_identity) = install_identity.as_deref() {
        guest.replace_pointer(
            CONTAINER_WORKSPACE_EFFIGY_INSTALL_IDENTITY_PATH,
            install_identity,
        )?;
    }
    Ok(())
}

/// Install the narrow enrolment credential for direct terminals (§8.8).
///
/// It authorises session enrolment only, never an effect, and is readable only
/// by the workspace user.
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

/// The version, digest and protocol the host pinned for the guest (§8.9).
fn pinned_guest_binary(artifact: &Path) -> Result<GuestBinaryIdentity, RunnerError> {
    let version = read_trimmed_workspace_effigy_active_version(
        &workspace_effigy_active_version_file(artifact),
    )?
    .ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "linux workspace artifact `{}` records no version; run \
             `{HOST_REPREPARE_COMMAND}` on the host to rebuild it",
            artifact.display()
        ))
    })?;
    let host_version = effigy_core::build_info::active_version();
    if version != host_version {
        return Err(RunnerError::task_invocation(format!(
            "linux workspace artifact `{}` is version {} but this host runs {}; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            artifact.display(),
            version,
            host_version
        )));
    }
    let digest = linux_workspace_artifact_digest(artifact, || true)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot read linux workspace artifact `{}`: {error}",
                artifact.display()
            ))
        })?
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "linux workspace artifact `{}` is missing; run `{HOST_REPREPARE_COMMAND}` on the \
                 host to re-prepare the workspace",
                artifact.display()
            ))
        })?;
    Ok(GuestBinaryIdentity {
        version,
        digest: render_digest(digest),
        protocol: CHANNEL_PROTOCOL_VERSION,
    })
}

fn operator_workspace_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<OperatorWorkspace>, RunnerError> {
    let (Some(user), Some(home)) = (policy.workspace_user.clone(), policy.workspace_home.clone())
    else {
        return Ok(None);
    };
    let guest_checkout =
        effigy_containers::load_container_exec_working_dir(repo_root, Some(&policy.name))
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(Some(OperatorWorkspace {
        user,
        home,
        guest_checkout: guest_checkout.display().to_string(),
    }))
}

fn workspace_path_map(
    repo_root: &Path,
    workspace: &OperatorWorkspace,
) -> Result<WorkspacePathMap, RunnerError> {
    let host_checkout = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    Ok(WorkspacePathMap {
        host_checkout: host_checkout.display().to_string(),
        guest_checkout: workspace.guest_checkout.clone(),
        workdir: workspace.guest_checkout.clone(),
        user: workspace.user.clone(),
        home: workspace.home.clone(),
    })
}

fn host_user() -> String {
    std::env::var("USER")
        .ok()
        .map(|user| user.trim().to_owned())
        .filter(|user| !user.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// The environment generation: the rendered snapshot this environment applies,
/// or the manifest that produced it (§8.2).
fn environment_generation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<String, RunnerError> {
    let snapshot = policy.render.as_ref().map(|render| render.path.clone());
    let source = snapshot.unwrap_or_else(|| repo_root.join(TASK_MANIFEST_FILE));
    match linux_workspace_artifact_digest(&source, || true) {
        Ok(Some(digest)) => Ok(render_digest(digest)),
        Ok(None) => Err(RunnerError::task_invocation(format!(
            "cannot read the environment snapshot `{}` to prepare the workspace context",
            source.display()
        ))),
        Err(error) => Err(RunnerError::task_invocation(format!(
            "cannot read the environment snapshot `{}`: {error}",
            source.display()
        ))),
    }
}

fn workspace_source_pins(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<WorkspacePinnedSource>, RunnerError> {
    let mut pins = Vec::new();
    let manifest = repo_root.join(TASK_MANIFEST_FILE);
    if let Some(digest) = linux_workspace_artifact_digest(&manifest, || true)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    {
        pins.push(WorkspacePinnedSource {
            kind: "manifest".to_owned(),
            name: TASK_MANIFEST_FILE.to_owned(),
            digest: render_digest(digest),
            snapshot: None,
        });
    }
    if let Some(render) = policy.render.as_ref() {
        if let Some(digest) = linux_workspace_artifact_digest(&render.path, || true)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        {
            pins.push(WorkspacePinnedSource {
                kind: "snapshot".to_owned(),
                name: "compose".to_owned(),
                digest: render_digest(digest),
                snapshot: Some(render.path.display().to_string()),
            });
        }
    }
    Ok(pins)
}

fn workspace_routes(policy: &EffectiveContainerPolicy) -> Vec<WorkspaceRoute> {
    policy
        .dns_routes
        .iter()
        .map(|route| WorkspaceRoute {
            declared_domain: route.declared().to_owned(),
            domain: route.domain.clone(),
            service: route.service.clone(),
            port: route.port,
            tls: route.tls,
        })
        .collect()
}

pub(super) fn render_workspace_effigy_staging_path() -> String {
    let counter = WORKSPACE_EFFIGY_STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{prefix}-{pid}-{counter}",
        prefix = CONTAINER_WORKSPACE_EFFIGY_STAGING_PATH_PREFIX,
        pid = std::process::id(),
        counter = counter,
    )
}

pub(super) fn read_trimmed_workspace_effigy_active_version(
    path: &Path,
) -> Result<Option<String>, RunnerError> {
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read workspace effigy active version file `{}`: {error}",
            path.display()
        ))
    })?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed.to_owned()))
}

fn run_workspace_compose_capture_bounded(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    target_id: &str,
    args: Vec<OsString>,
    budget: &mut BoundedCommandBudget,
) -> Result<BoundedCapture, RunnerError> {
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_args(
        repo_root,
        policy,
        args,
        effigy_containers::ContainerAction::Exec,
        "workspace preparation",
    )
    .map_err(|_| {
        bounded_phase_error(
            policy,
            policy.primary_service.as_str(),
            budget,
            BoundedCommandPhase::WorkspacePreparation,
            "workspace_preparation_failed",
            "bounded container command could not resolve workspace preparation",
            None,
        )
    })?;
    // Bounded preparation targets the container whose ownership the caller
    // already verified; the Compose service alias can name a replacement.
    let exec_args = crate::runner::exec_command::verified_target_exec_args(&plan, target_id)
        .map_err(|_| {
            bounded_phase_error(
                policy,
                policy.primary_service.as_str(),
                budget,
                BoundedCommandPhase::WorkspacePreparation,
                "workspace_preparation_failed",
                "bounded container command could not resolve workspace preparation",
                None,
            )
        })?;
    let plan = effigy_runtime::container_manager::runtime_invocation_plan(
        repo_root,
        policy,
        "docker",
        &exec_args,
        effigy_containers::ContainerAction::Exec,
        "workspace preparation",
    )
    .map_err(|_| {
        bounded_phase_error(
            policy,
            policy.primary_service.as_str(),
            budget,
            BoundedCommandPhase::WorkspacePreparation,
            "workspace_preparation_failed",
            "bounded container command could not resolve workspace preparation",
            None,
        )
    })?;
    budget
        .run_command(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            None,
            &[],
            BoundedCommandPhase::WorkspacePreparation,
        )
        .map_err(|failure| {
            bounded_failure_error(policy, policy.primary_service.as_str(), budget, failure)
        })
}

/// The container-mode guest filesystem, reached through Compose exec and copy.
enum GuestCommand<'a> {
    Service,
    Target {
        target_id: &'a str,
        budget: &'a mut BoundedCommandBudget,
    },
}

struct ComposeWorkspaceGuest<'a> {
    repo_root: &'a Path,
    policy: &'a EffectiveContainerPolicy,
    command: GuestCommand<'a>,
}

impl<'a> ComposeWorkspaceGuest<'a> {
    fn unbounded(repo_root: &'a Path, policy: &'a EffectiveContainerPolicy) -> Self {
        Self {
            repo_root,
            policy,
            command: GuestCommand::Service,
        }
    }

    fn bounded(
        repo_root: &'a Path,
        policy: &'a EffectiveContainerPolicy,
        target_id: &'a str,
        budget: &'a mut BoundedCommandBudget,
    ) -> Self {
        Self {
            repo_root,
            policy,
            command: GuestCommand::Target { target_id, budget },
        }
    }

    fn run_script(&mut self, script: &str, as_root: bool) -> Result<BoundedCapture, RunnerError> {
        let mut compose_args =
            effigy_containers::compose::compose_args(self.policy, ["exec", "-T"]);
        if as_root {
            compose_args.extend([OsString::from("-u"), OsString::from("0")]);
        }
        compose_args.extend([
            OsString::from(self.policy.primary_service.as_str()),
            OsString::from("sh"),
            OsString::from("-lc"),
            OsString::from(script),
        ]);
        match &mut self.command {
            GuestCommand::Service => {
                let output = crate::runner::exec_command::run_compose_exec(
                    self.repo_root,
                    self.policy,
                    &compose_args,
                    true,
                    "workspace guest command",
                )?;
                Ok(BoundedCapture {
                    status: output.status,
                    stdout: output.stdout,
                    stderr: output.stderr,
                })
            }
            GuestCommand::Target { target_id, budget } => run_workspace_compose_capture_bounded(
                self.repo_root,
                self.policy,
                target_id,
                compose_args,
                budget,
            ),
        }
    }

    fn stage(&mut self, host_path: &Path, guest_path: &str) -> Result<(), RunnerError> {
        match &mut self.command {
            GuestCommand::Service => copy_file_into_service(
                self.repo_root,
                self.policy,
                self.policy.primary_service.as_str(),
                host_path,
                guest_path,
            ),
            GuestCommand::Target { target_id, budget } => copy_file_into_service_bounded(
                self.repo_root,
                self.policy,
                self.policy.primary_service.as_str(),
                target_id,
                host_path,
                guest_path,
                budget,
            ),
        }
    }
}

impl GuestReader for ComposeWorkspaceGuest<'_> {
    fn read_file(&mut self, path: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        let script = format!("cat {} 2>/dev/null || exit 3", shell_quote(path));
        let output = self.run_script(&script, false)?;
        if output.status.success() {
            return Ok(Some(output.stdout));
        }
        if output.status.code() == Some(3) {
            return Ok(None);
        }
        Err(RunnerError::task_invocation(format!(
            "cannot read `{path}` in workspace container `{}`: {}",
            self.policy.name,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

impl GuestWorkspace for ComposeWorkspaceGuest<'_> {
    fn ensure_dir(&mut self, path: &str, mode: u32) -> Result<(), RunnerError> {
        let script = format!(
            "mkdir -p {path} && chmod {mode:04o} {path}",
            path = shell_quote(path)
        );
        self.run_root(&script, "make a guest directory")
    }

    fn write_file(&mut self, path: &str, bytes: &[u8], mode: u32) -> Result<(), RunnerError> {
        let staging = render_workspace_effigy_staging_path();
        let host_path = stage_host_bytes(bytes)?;
        let staged = self.stage(&host_path, &staging);
        let _ = std::fs::remove_file(&host_path);
        staged?;
        let script = format!(
            "install -m {mode:04o} {staging} {path} && rm -f {staging}",
            staging = shell_quote(&staging),
            path = shell_quote(path),
        );
        self.run_root(&script, "publish a guest file")
    }

    fn set_owner(&mut self, path: &str, owner: &str) -> Result<(), RunnerError> {
        let script = format!(
            "chown {owner} {path}",
            owner = shell_quote(owner),
            path = shell_quote(path)
        );
        self.run_root(&script, "give a guest file to the workspace user")
    }

    fn stage_host_file(&mut self, host_path: &Path, guest_path: &str) -> Result<(), RunnerError> {
        self.stage(host_path, guest_path)
    }

    fn install_executable(&mut self, staging: &str, path: &str) -> Result<(), RunnerError> {
        let script = format!(
            "install -m 0555 {staging} {path} && rm -f {staging}",
            staging = shell_quote(staging),
            path = shell_quote(path),
        );
        self.run_root(&script, "install the pinned guest executable")
    }

    fn replace_pointer(&mut self, path: &str, target: &str) -> Result<(), RunnerError> {
        let script = format!(
            "printf '%s\\n' {target} > {path}.tmp && mv -f {path}.tmp {path}",
            target = shell_quote(target),
            path = shell_quote(path),
        );
        self.run_root(&script, "switch a guest pointer")
    }

    fn replace_link(&mut self, path: &str, target: &str) -> Result<(), RunnerError> {
        let script = format!(
            "ln -sfn {target} {path}.tmp && mv -Tf {path}.tmp {path}",
            target = shell_quote(target),
            path = shell_quote(path),
        );
        self.run_root(&script, "switch the guest entry executable")
    }

    fn probe_container_identity(&mut self) -> Result<WorkspaceContainerIdentity, RunnerError> {
        let output = self.run_script(
            "printf 'boot=%s\\n' \"$(cat /proc/sys/kernel/random/boot_id)\"; \
             printf 'hostname=%s\\n' \"$(hostname)\"; \
             printf 'platform=%s\\n' \"$(uname -m)\"",
            false,
        )?;
        if !output.status.success() {
            return Err(RunnerError::task_invocation(format!(
                "cannot read the workspace container identity for `{}`: {}",
                self.policy.name,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let rendered = String::from_utf8_lossy(&output.stdout);
        let field = |name: &str| {
            rendered
                .lines()
                .find_map(|line| line.trim().strip_prefix(name)?.strip_prefix('='))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let container_id = field("hostname");
        let vm_boot_id = field("boot");
        let platform = field("platform");
        match (container_id, vm_boot_id, platform) {
            (Some(container_id), Some(vm_boot_id), Some(platform)) => {
                Ok(WorkspaceContainerIdentity {
                    container_id,
                    vm_boot_id,
                    platform,
                })
            }
            _ => Err(RunnerError::task_invocation(format!(
                "workspace container `{}` did not report its identity; run \
                 `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
                self.policy.name
            ))),
        }
    }
}

impl ComposeWorkspaceGuest<'_> {
    fn run_root(&mut self, script: &str, what: &str) -> Result<(), RunnerError> {
        let output = self.run_script(script, true)?;
        if output.status.success() {
            return Ok(());
        }
        Err(RunnerError::task_invocation(format!(
            "cannot {what} in workspace container `{}`: {}",
            self.policy.name,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// Stage bytes for a host → container copy. The file is private to this
/// Prepare run and removed as soon as the copy finishes.
fn stage_host_bytes(bytes: &[u8]) -> Result<PathBuf, RunnerError> {
    let path = std::env::temp_dir().join(format!(
        "effigy-workspace-context-{}-{}",
        std::process::id(),
        WORKSPACE_EFFIGY_STAGING_COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::write(&path, bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot stage workspace context bytes at `{}`: {error}",
            path.display()
        ))
    })?;
    Ok(path)
}
pub(super) fn ensure_linux_workspace_effigy_artifact(
    workspace_repo_root: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<PathBuf, RunnerError> {
    let host_binary = std::env::current_exe().map_err(RunnerError::Cwd)?;
    match configured_linux_workspace_artifact_source()? {
        LinuxWorkspaceArtifactSource::Download => {
            return ensure_downloaded_linux_workspace_effigy_artifact(target);
        }
        LinuxWorkspaceArtifactSource::Local | LinuxWorkspaceArtifactSource::Auto => {}
    }
    if let Some(effigy_repo_root) = resolve_local_effigy_repo_root(workspace_repo_root)? {
        persist_effigy_source_repo_root(&effigy_repo_root)?;
        return ensure_local_linux_workspace_effigy_artifact(
            &host_binary,
            &effigy_repo_root,
            target,
        );
    }

    ensure_downloaded_linux_workspace_effigy_artifact(target)
}

/// Bounded artifact acquisition and its rehearsals.
///
/// The bounded entry path (`exec`) now verifies the pinned guest instead of
/// acquiring one, so these have no product caller: they remain the rehearsal
/// proofs for the artifact builder's deadline and freshness behaviour.
#[cfg(test)]
pub(super) fn ensure_local_linux_workspace_effigy_artifact_bounded(
    host_binary: &Path,
    effigy_repo_root: &Path,
    target: LinuxWorkspaceTarget,
    policy: &EffectiveContainerPolicy,
    budget: &mut BoundedCommandBudget,
) -> Result<PathBuf, RunnerError> {
    let artifact_path = effigy_repo_root.join(target.artifact_relative_path());
    let freshness_anchor =
        resolve_local_workspace_effigy_freshness_anchor(host_binary, effigy_repo_root);
    let install_identity = workspace_effigy_local_install_identity(&freshness_anchor)?;

    if !linux_workspace_artifact_lock_path(effigy_repo_root).exists()
        && linux_workspace_effigy_artifact_is_current(&artifact_path, target, &install_identity)
    {
        return bind_verified_linux_workspace_effigy_artifact(&artifact_path, &install_identity);
    }
    if wait_for_in_progress_linux_workspace_artifact_build_bounded(
        effigy_repo_root,
        &install_identity,
        &artifact_path,
        target,
        policy,
        budget,
    )? {
        return bind_verified_linux_workspace_effigy_artifact(&artifact_path, &install_identity);
    }
    super::emit_workspace_info(
        "building linux effigy artifact for workspace container access",
        false,
    );
    let previous_digest = linux_workspace_artifact_digest_bounded(&artifact_path, policy, budget)?;
    let started = std::time::SystemTime::now();
    let built = run_linux_workspace_effigy_artifact_build_bounded(
        host_binary,
        effigy_repo_root,
        target,
        policy,
        budget,
    )?;
    if built
        && linux_workspace_artifact_produced_since(&artifact_path, target, started)
        && linux_workspace_artifact_digest_bounded(&artifact_path, policy, budget)?
            .is_some_and(|digest| Some(digest) != previous_digest)
    {
        return bind_verified_linux_workspace_effigy_artifact(&artifact_path, &install_identity);
    }
    if !built
        && wait_for_in_progress_linux_workspace_artifact_build_bounded(
            effigy_repo_root,
            &install_identity,
            &artifact_path,
            target,
            policy,
            budget,
        )?
    {
        return bind_verified_linux_workspace_effigy_artifact(&artifact_path, &install_identity);
    }
    // Bounded errors carry only static text, so the mismatch is named by target
    // and the rebuild command rather than by local paths or identities.
    Err(bounded_phase_error(
        policy,
        policy.primary_service.as_str(),
        budget,
        BoundedCommandPhase::WorkspacePreparation,
        "workspace_preparation_failed",
        match (built, target) {
            (true, LinuxWorkspaceTarget::X86_64Gnu) => {
                "bounded container command build left no current workspace artifact; rebuild with EFFIGY_LINUX_RELEASE_TRIPLE=x86_64-unknown-linux-gnu effigy workspace:linux:artifact"
            }
            (true, LinuxWorkspaceTarget::Aarch64Gnu) => {
                "bounded container command build left no current workspace artifact; rebuild with EFFIGY_LINUX_RELEASE_TRIPLE=aarch64-unknown-linux-gnu effigy workspace:linux:artifact"
            }
            (false, LinuxWorkspaceTarget::X86_64Gnu) => {
                "bounded container command could not refresh the stale workspace artifact; rebuild with EFFIGY_LINUX_RELEASE_TRIPLE=x86_64-unknown-linux-gnu effigy workspace:linux:artifact"
            }
            (false, LinuxWorkspaceTarget::Aarch64Gnu) => {
                "bounded container command could not refresh the stale workspace artifact; rebuild with EFFIGY_LINUX_RELEASE_TRIPLE=aarch64-unknown-linux-gnu effigy workspace:linux:artifact"
            }
        },
        None,
    ))
}

pub(super) fn resolve_local_effigy_repo_root(
    workspace_repo_root: &Path,
) -> Result<Option<PathBuf>, RunnerError> {
    let current_exe = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let cwd = current_working_dir()?;
    Ok(resolve_local_effigy_repo_root_from_paths(
        workspace_repo_root,
        current_exe.parent(),
        Some(cwd.as_path()),
    ))
}

pub(super) fn resolve_local_effigy_repo_root_from_paths(
    workspace_repo_root: &Path,
    current_exe_parent: Option<&Path>,
    cwd: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(effigy_repo_root) = discover_effigy_repo_root(current_exe_parent) {
        return Some(effigy_repo_root);
    }

    if let Some(effigy_repo_root) = discover_effigy_repo_root(cwd) {
        return Some(effigy_repo_root);
    }

    if let Some(effigy_repo_root) = sibling_effigy_repo_root(workspace_repo_root) {
        return Some(effigy_repo_root);
    }

    configured_effigy_repo_root()
}

pub(super) fn configured_linux_workspace_artifact_source(
) -> Result<LinuxWorkspaceArtifactSource, RunnerError> {
    let Some(raw) = std::env::var_os(EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV) else {
        return Ok(LinuxWorkspaceArtifactSource::Auto);
    };
    let normalized = raw.to_string_lossy().trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "auto" => Ok(LinuxWorkspaceArtifactSource::Auto),
        "local" => Ok(LinuxWorkspaceArtifactSource::Local),
        "download" | "github" | "release" => Ok(LinuxWorkspaceArtifactSource::Download),
        _ => Err(RunnerError::task_invocation(format!(
            "{EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV} must be one of `auto`, `local`, or `download`; got `{normalized}`"
        ))),
    }
}

pub(super) fn sibling_effigy_repo_root(workspace_repo_root: &Path) -> Option<PathBuf> {
    for ancestor in workspace_repo_root.ancestors().skip(1) {
        for candidate in nearby_effigy_repo_candidates(ancestor) {
            if looks_like_effigy_repo_root(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

pub(super) fn nearby_effigy_repo_candidates(base: &Path) -> [PathBuf; 2] {
    [base.join("effigy"), base.join("projects").join("effigy")]
}

pub(super) fn configured_effigy_repo_root() -> Option<PathBuf> {
    let path = effigy_source_config_path().ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let config = toml::from_str::<EffigySourceConfig>(&raw).ok()?;
    let repo_root = PathBuf::from(config.repo_root);
    looks_like_effigy_repo_root(&repo_root).then_some(repo_root)
}

pub(super) fn persist_effigy_source_repo_root(repo_root: &Path) -> Result<(), RunnerError> {
    let path = effigy_source_config_path()?;
    let parent = path.parent().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "effigy source config path has no parent: `{}`",
            path.display()
        ))
    })?;
    std::fs::create_dir_all(parent).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create effigy source config directory `{}`: {error}",
            parent.display()
        ))
    })?;
    let body = toml::to_string_pretty(&EffigySourceConfig {
        repo_root: repo_root.display().to_string(),
    })
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to serialize effigy source config for `{}`: {error}",
            repo_root.display()
        ))
    })?;
    std::fs::write(&path, body).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write effigy source config `{}`: {error}",
            path.display()
        ))
    })
}

pub(super) fn effigy_source_config_path() -> Result<PathBuf, RunnerError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        RunnerError::task_invocation("HOME is not set; cannot resolve effigy source config path")
    })?;
    Ok(PathBuf::from(home).join(".effigy").join("source.toml"))
}

pub(super) fn discover_effigy_repo_root(start: Option<&Path>) -> Option<PathBuf> {
    let mut current = start?;
    loop {
        if looks_like_effigy_repo_root(current) {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

pub(super) fn looks_like_effigy_repo_root(path: &Path) -> bool {
    has_task_manifest(path)
        && path.join("config/tasks.toml").is_file()
        && path.join("config/containers.toml").is_file()
}

pub(super) fn ensure_local_linux_workspace_effigy_artifact(
    host_binary: &Path,
    effigy_repo_root: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<PathBuf, RunnerError> {
    let artifact_path = effigy_repo_root.join(target.artifact_relative_path());
    let freshness_anchor =
        resolve_local_workspace_effigy_freshness_anchor(host_binary, effigy_repo_root);
    let install_identity = workspace_effigy_local_install_identity(&freshness_anchor)?;

    // Only an artifact bound to the current host identity is handed off. Stale,
    // unknown, or wrong-target bytes are never reused to avoid a rebuild.
    if !linux_workspace_artifact_lock_path(effigy_repo_root).exists()
        && linux_workspace_effigy_artifact_is_current(&artifact_path, target, &install_identity)
    {
        return bind_verified_linux_workspace_effigy_artifact(&artifact_path, &install_identity);
    }
    if wait_for_in_progress_linux_workspace_artifact_build(
        effigy_repo_root,
        &install_identity,
        &artifact_path,
        target,
    )? {
        return bind_verified_linux_workspace_effigy_artifact(&artifact_path, &install_identity);
    }
    super::emit_workspace_info(
        "building linux effigy artifact for workspace container access",
        false,
    );
    let previous_digest =
        linux_workspace_artifact_digest(&artifact_path, || true).map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot inspect previous workspace artifact output: {error}"
            ))
        })?;
    // Bytes already built for this exact version (for example by a direct
    // `workspace:linux:artifact`, which writes no install identity) may come
    // back unchanged from a no-op rebuild; they are the current build's output.
    // Touched bytes from another version stay refused.
    let previous_was_current_version =
        read_trimmed_identity_file(&workspace_effigy_active_version_file(&artifact_path))
            .as_deref()
            == Some(effigy_core::build_info::active_version().as_str());
    let started = std::time::SystemTime::now();
    match run_linux_workspace_effigy_artifact_build(host_binary, effigy_repo_root, target) {
        Ok(())
            if linux_workspace_artifact_produced_since(&artifact_path, target, started)
                && linux_workspace_artifact_digest(&artifact_path, || true).is_ok_and(
                    |digest| {
                        digest.is_some()
                            && (digest != previous_digest || previous_was_current_version)
                    },
                ) =>
        {
            bind_verified_linux_workspace_effigy_artifact(&artifact_path, &install_identity)
        }
        Ok(()) => Err(RunnerError::task_invocation(format!(
            "linux workspace artifact build succeeded but did not produce a current artifact\n{}",
            describe_linux_workspace_artifact_refusal(
                &artifact_path,
                target,
                &install_identity,
                effigy_repo_root,
            )
        ))),
        Err(error) => {
            if wait_for_in_progress_linux_workspace_artifact_build(
                effigy_repo_root,
                &install_identity,
                &artifact_path,
                target,
            )? {
                return bind_verified_linux_workspace_effigy_artifact(
                    &artifact_path,
                    &install_identity,
                );
            }
            Err(linux_workspace_artifact_offline_hint(
                error,
                &describe_linux_workspace_artifact_refusal(
                    &artifact_path,
                    target,
                    &install_identity,
                    effigy_repo_root,
                ),
            ))
        }
    }
}

/// Bind the active-version and install-identity sidecars to a verified artifact.
/// Callers must have proven the artifact current, or that a supported build or
/// download just produced it; the sidecars are never written for stale bytes.
fn bind_verified_linux_workspace_effigy_artifact(
    artifact_path: &Path,
    install_identity: &str,
) -> Result<PathBuf, RunnerError> {
    ensure_workspace_effigy_active_version_file(
        artifact_path,
        &effigy_core::build_info::active_version(),
    )?;
    ensure_workspace_effigy_install_identity_file(artifact_path, install_identity)?;
    Ok(artifact_path.to_path_buf())
}

/// A prepared artifact is current only when its bytes are bound to the expected
/// host identity by the identity sidecar and its rehearsal receipt names the
/// target. Timestamps and the presence of the file alone do not qualify.
pub(super) fn linux_workspace_effigy_artifact_is_current(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
    expected_identity: &str,
) -> bool {
    artifact_path.is_file()
        && linux_workspace_effigy_rehearsal_receipt_matches_target(artifact_path, target)
        && read_trimmed_identity_file(&workspace_effigy_active_version_file(artifact_path))
            .as_deref()
            == Some(effigy_core::build_info::active_version().as_str())
        && read_trimmed_identity_file(&workspace_effigy_install_identity_file(artifact_path))
            .as_deref()
            == Some(expected_identity.trim())
}

/// An in-memory comparison rejects unchanged output. This digest never becomes
/// an install identity or provenance receipt, and current reuse does not hash.
fn linux_workspace_artifact_digest(
    artifact_path: &Path,
    mut within_deadline: impl FnMut() -> bool,
) -> std::io::Result<Option<[u8; 32]>> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = match std::fs::File::open(artifact_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut digest = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        if !within_deadline() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "artifact inspection deadline expired",
            ));
        }
        let read = file.read(&mut bytes)?;
        if read == 0 {
            return Ok(Some(digest.finalize().into()));
        }
        digest.update(&bytes[..read]);
    }
}

/// Bounded artifact acquisition and its rehearsals.
///
/// The bounded entry path (`exec`) now verifies the pinned guest instead of
/// acquiring one, so these have no product caller: they remain the rehearsal
/// proofs for the artifact builder's deadline and freshness behaviour.
#[cfg(test)]
fn linux_workspace_artifact_digest_bounded(
    artifact_path: &Path,
    policy: &EffectiveContainerPolicy,
    budget: &BoundedCommandBudget,
) -> Result<Option<[u8; 32]>, RunnerError> {
    linux_workspace_artifact_digest(artifact_path, || budget.remaining_time().is_some()).map_err(
        |error| {
            if error.kind() == std::io::ErrorKind::TimedOut {
                bounded_failure_error(
                    policy,
                    &policy.primary_service,
                    budget,
                    budget.boundary_refusal(
                        BoundedCommandPhase::WorkspacePreparation,
                        BoundedCaptureFailure::Deadline,
                    ),
                )
            } else {
                bounded_phase_error(
                    policy,
                    &policy.primary_service,
                    budget,
                    BoundedCommandPhase::WorkspacePreparation,
                    "workspace_preparation_failed",
                    "bounded container command could not inspect workspace artifact output",
                    None,
                )
            }
        },
    )
}

/// A successful build only counts when it rewrote the artifact and its receipt
/// for this target. An exit status alone can leave older output in place.
pub(super) fn linux_workspace_artifact_produced_since(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
    started: std::time::SystemTime,
) -> bool {
    let written = std::fs::metadata(artifact_path)
        .and_then(|metadata| metadata.modified())
        .is_ok_and(|modified| modified >= started);
    written
        && read_trimmed_identity_file(&workspace_effigy_active_version_file(artifact_path))
            .as_deref()
            == Some(effigy_core::build_info::active_version().as_str())
        && linux_workspace_effigy_rehearsal_receipt_matches_target(artifact_path, target)
}

pub(super) fn read_trimmed_identity_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
}

/// Name what is stale and the supported rebuild. Reports host and artifact
/// versions and identities, with `unknown` where a sidecar is absent.
pub(super) fn describe_linux_workspace_artifact_refusal(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
    expected_identity: &str,
    effigy_repo_root: &Path,
) -> String {
    let artifact_version =
        read_trimmed_identity_file(&workspace_effigy_active_version_file(artifact_path))
            .unwrap_or_else(|| "unknown".to_owned());
    let artifact_identity =
        read_trimmed_identity_file(&workspace_effigy_install_identity_file(artifact_path))
            .unwrap_or_else(|| "unknown".to_owned());
    let receipt = if linux_workspace_effigy_rehearsal_receipt_matches_target(artifact_path, target)
    {
        "matches target"
    } else {
        "missing or names another target"
    };
    format!(
        "linux workspace artifact `{path}` is not current for `{triple}`: host version `{host}`, host identity `{expected_identity}`, artifact version `{artifact_version}`, artifact identity `{artifact_identity}`, rehearsal receipt {receipt}; rebuild with `EFFIGY_LINUX_RELEASE_TRIPLE={triple} effigy workspace:linux:artifact` from `{root}`",
        path = artifact_path.display(),
        triple = target.release_triple(),
        host = effigy_core::build_info::active_version(),
        root = effigy_repo_root.display(),
    )
}

pub(super) fn linux_workspace_artifact_offline_hint(
    error: RunnerError,
    refusal: &str,
) -> RunnerError {
    RunnerError::task_invocation(format!(
        "{error}; {refusal}. A rebuild needs Docker Hub for `ubuntu:22.04` / `effigy-linux-release-builder`: pre-load those images for an offline rebuild, run the rebuild command above when Hub is reachable, or set `{EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV}=download` to use a GitHub release binary for this version"
    ))
}

pub(super) fn linux_workspace_artifact_lock_path(effigy_repo_root: &Path) -> PathBuf {
    effigy_repo_root
        .join(".effigy/locks")
        .join("task-workspace-linux-artifact.lock")
}

pub(super) fn wait_for_in_progress_linux_workspace_artifact_build(
    effigy_repo_root: &Path,
    expected_identity: &str,
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<bool, RunnerError> {
    let lock_path = linux_workspace_artifact_lock_path(effigy_repo_root);
    if !lock_path.exists() {
        return Ok(false);
    }
    let deadline = std::time::Instant::now() + LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_TIMEOUT;
    loop {
        if std::time::Instant::now() >= deadline {
            return Err(RunnerError::task_invocation(format!(
                "timed out waiting for in-progress linux workspace artifact build in `{}` to finish; lock `{}` is still present",
                effigy_repo_root.display(),
                lock_path.display()
            )));
        }
        if !lock_path.exists() {
            return Ok(linux_workspace_effigy_artifact_is_current(
                artifact_path,
                target,
                expected_identity,
            ));
        }
        std::thread::sleep(LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_POLL_INTERVAL);
    }
}

/// Bounded artifact acquisition and its rehearsals.
///
/// The bounded entry path (`exec`) now verifies the pinned guest instead of
/// acquiring one, so these have no product caller: they remain the rehearsal
/// proofs for the artifact builder's deadline and freshness behaviour.
#[cfg(test)]
fn wait_for_in_progress_linux_workspace_artifact_build_bounded(
    effigy_repo_root: &Path,
    expected_identity: &str,
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
    policy: &EffectiveContainerPolicy,
    budget: &mut BoundedCommandBudget,
) -> Result<bool, RunnerError> {
    let lock_path = linux_workspace_artifact_lock_path(effigy_repo_root);
    if !lock_path.exists() {
        return Ok(false);
    }
    let wait_deadline = std::time::Instant::now() + LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_TIMEOUT;
    loop {
        let Some(remaining) = budget.remaining_time() else {
            return Err(bounded_failure_error(
                policy,
                policy.primary_service.as_str(),
                budget,
                budget.boundary_refusal(
                    BoundedCommandPhase::WorkspacePreparation,
                    BoundedCaptureFailure::Deadline,
                ),
            ));
        };
        if std::time::Instant::now() >= wait_deadline {
            return Err(bounded_phase_error(
                policy,
                policy.primary_service.as_str(),
                budget,
                BoundedCommandPhase::WorkspacePreparation,
                "workspace_preparation_timeout",
                "bounded container command timed out waiting for workspace artifact preparation",
                None,
            ));
        }
        if !lock_path.exists() {
            return Ok(linux_workspace_effigy_artifact_is_current(
                artifact_path,
                target,
                expected_identity,
            ));
        }
        std::thread::sleep(remaining.min(LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_POLL_INTERVAL));
    }
}

pub(super) fn resolve_local_workspace_effigy_freshness_anchor(
    host_binary: &Path,
    effigy_repo_root: &Path,
) -> PathBuf {
    let candidates = [
        effigy_repo_root.join(".local-install/bin/effigy"),
        effigy_repo_root.join("target/debug/effigy"),
    ];
    newest_existing_path(&candidates).unwrap_or_else(|| host_binary.to_path_buf())
}

pub(super) fn newest_existing_path(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .filter_map(|path| {
            std::fs::metadata(path)
                .ok()?
                .modified()
                .ok()
                .map(|modified| (modified, path.clone()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

pub(super) fn ensure_downloaded_linux_workspace_effigy_artifact(
    target: LinuxWorkspaceTarget,
) -> Result<PathBuf, RunnerError> {
    let cache_path = linux_workspace_effigy_cache_path(target)?;
    let install_identity = workspace_effigy_release_install_identity();
    if linux_workspace_release_cache_is_current(&cache_path, &install_identity) {
        ensure_workspace_effigy_active_version_file(
            &cache_path,
            &effigy_core::build_info::active_version(),
        )?;
        ensure_workspace_effigy_install_identity_file(&cache_path, &install_identity)?;
        return Ok(cache_path);
    }

    let parent = cache_path.parent().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "workspace linux effigy cache path has no parent: `{}`",
            cache_path.display()
        ))
    })?;
    std::fs::create_dir_all(parent).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create workspace linux effigy cache directory `{}`: {error}",
            parent.display()
        ))
    })?;

    let url = linux_workspace_effigy_release_url(target);
    super::emit_workspace_info(
        &format!("downloading linux effigy release artifact from `{url}`"),
        false,
    );
    download_linux_workspace_effigy_release(&url, &cache_path)?;
    ensure_workspace_effigy_active_version_file(
        &cache_path,
        &effigy_core::build_info::active_version(),
    )?;
    ensure_workspace_effigy_install_identity_file(&cache_path, &install_identity)?;
    Ok(cache_path)
}

pub(super) fn workspace_effigy_active_version_file(binary: &Path) -> PathBuf {
    binary.with_extension("active-version")
}

pub(super) fn workspace_effigy_install_identity_file(binary: &Path) -> PathBuf {
    binary.with_extension("install-identity")
}

fn ensure_workspace_effigy_active_version_file(
    binary: &Path,
    active_version: &str,
) -> Result<(), RunnerError> {
    let version_file = workspace_effigy_active_version_file(binary);
    std::fs::write(&version_file, format!("{active_version}\n")).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write workspace effigy active version file `{}`: {error}",
            version_file.display()
        ))
    })
}

fn ensure_workspace_effigy_install_identity_file(
    binary: &Path,
    install_identity: &str,
) -> Result<(), RunnerError> {
    let identity_file = workspace_effigy_install_identity_file(binary);
    std::fs::write(&identity_file, format!("{install_identity}\n")).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write workspace effigy install identity file `{}`: {error}",
            identity_file.display()
        ))
    })
}

pub(super) fn expected_workspace_effigy_install_identity(
    workspace_repo_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let host_binary = std::env::current_exe().map_err(RunnerError::Cwd)?;
    match configured_linux_workspace_artifact_source()? {
        LinuxWorkspaceArtifactSource::Download => {
            return Ok(Some(workspace_effigy_release_install_identity()));
        }
        LinuxWorkspaceArtifactSource::Local | LinuxWorkspaceArtifactSource::Auto => {}
    }
    let Some(effigy_repo_root) = resolve_local_effigy_repo_root(workspace_repo_root)? else {
        return Ok(Some(workspace_effigy_release_install_identity()));
    };
    persist_effigy_source_repo_root(&effigy_repo_root)?;
    let freshness_anchor =
        resolve_local_workspace_effigy_freshness_anchor(&host_binary, &effigy_repo_root);
    Ok(Some(workspace_effigy_local_install_identity(
        &freshness_anchor,
    )?))
}

pub(super) fn workspace_effigy_release_install_identity() -> String {
    format!("release:v{}", effigy_core::build_info::active_version())
}

pub(super) fn workspace_effigy_local_install_identity(
    freshness_anchor: &Path,
) -> Result<String, RunnerError> {
    let metadata = std::fs::metadata(freshness_anchor).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to stat workspace effigy freshness anchor `{}`: {error}",
            freshness_anchor.display()
        ))
    })?;
    let modified = metadata.modified().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read workspace effigy freshness anchor mtime `{}`: {error}",
            freshness_anchor.display()
        ))
    })?;
    let modified_nanos = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "workspace effigy freshness anchor mtime predates unix epoch `{}`: {error}",
                freshness_anchor.display()
            ))
        })?
        .as_nanos();
    Ok(format!(
        "local:v{}:{}:{}",
        effigy_core::build_info::active_version(),
        metadata.len(),
        modified_nanos
    ))
}

/// A cached release binary is reused only when its identity sidecar names this
/// release. Unlabelled bytes are downloaded again instead of being stamped current.
pub(super) fn linux_workspace_release_cache_is_current(
    cache_path: &Path,
    install_identity: &str,
) -> bool {
    cache_path.is_file()
        && read_trimmed_identity_file(&workspace_effigy_active_version_file(cache_path)).as_deref()
            == Some(effigy_core::build_info::active_version().as_str())
        && read_trimmed_identity_file(&workspace_effigy_install_identity_file(cache_path))
            .as_deref()
            == Some(install_identity)
}

pub(super) fn linux_workspace_effigy_cache_path(
    target: LinuxWorkspaceTarget,
) -> Result<PathBuf, RunnerError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        RunnerError::task_invocation(
            "HOME is not set; cannot resolve workspace linux effigy cache path",
        )
    })?;
    Ok(Path::new(&home)
        .join(target.cache_relative_dir())
        .join(format!("v{}", env!("CARGO_PKG_VERSION")))
        .join(format!("effigy-{}", target.release_triple())))
}

pub(super) fn linux_workspace_effigy_release_url(target: LinuxWorkspaceTarget) -> String {
    format!(
        "{}/releases/download/v{}/effigy-{}",
        EFFIGY_RELEASE_REPO_BASE_URL,
        env!("CARGO_PKG_VERSION"),
        target.release_triple()
    )
}

pub(super) fn download_linux_workspace_effigy_release(
    url: &str,
    dest: &Path,
) -> Result<(), RunnerError> {
    let response = reqwest::blocking::Client::builder()
        .build()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to initialize release download client for `{url}`: {error}"
            ))
        })?
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to download linux effigy release artifact from `{url}`: {error}"
            ))
        })?;

    let tmp_path = dest.with_extension("tmp");
    let bytes = response.bytes().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read linux effigy release artifact payload from `{url}`: {error}"
        ))
    })?;
    std::fs::write(&tmp_path, &bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write linux effigy release artifact to `{}`: {error}",
            tmp_path.display()
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&tmp_path)
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to stat linux effigy release artifact `{}`: {error}",
                    tmp_path.display()
                ))
            })?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&tmp_path, permissions).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to mark linux effigy release artifact executable `{}`: {error}",
                tmp_path.display()
            ))
        })?;
    }
    std::fs::rename(&tmp_path, dest).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to move linux effigy release artifact into cache `{}`: {error}",
            dest.display()
        ))
    })?;
    Ok(())
}

pub(super) fn linux_workspace_effigy_rehearsal_receipt_matches_target(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
) -> bool {
    let Some(artifacts_dir) = artifact_path.parent() else {
        return false;
    };
    let receipt_path = artifacts_dir.join("rehearsal.txt");
    let Ok(raw) = std::fs::read_to_string(receipt_path) else {
        return false;
    };
    raw.lines()
        .any(|line| line.trim() == format!("release_triple={}", target.release_triple()))
}

pub(super) fn run_linux_workspace_effigy_artifact_build(
    host_binary: &Path,
    effigy_repo_root: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<(), RunnerError> {
    let mut command = std::process::Command::new(host_binary);
    command
        .arg("workspace:linux:artifact")
        .env("EFFIGY_LINUX_RELEASE_TRIPLE", target.release_triple())
        .current_dir(effigy_repo_root);
    // The artifact build is Effigy's own child: it reads the session grant
    // from a descriptor, never from its environment or argv (effigy#338).
    crate::runner::secret_session::grant_command(&mut command);
    let status = command.status().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to launch linux workspace artifact build in `{}`: {error}",
            effigy_repo_root.display()
        ))
    })?;
    if status.success() {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "linux workspace artifact build failed in `{}` with status {}",
        effigy_repo_root.display(),
        status
    )))
}

/// Bounded artifact acquisition and its rehearsals.
///
/// The bounded entry path (`exec`) now verifies the pinned guest instead of
/// acquiring one, so these have no product caller: they remain the rehearsal
/// proofs for the artifact builder's deadline and freshness behaviour.
#[cfg(test)]
fn run_linux_workspace_effigy_artifact_build_bounded(
    host_binary: &Path,
    effigy_repo_root: &Path,
    target: LinuxWorkspaceTarget,
    policy: &EffectiveContainerPolicy,
    budget: &mut BoundedCommandBudget,
) -> Result<bool, RunnerError> {
    let args = [OsString::from("workspace:linux:artifact")];
    // This bounded release build is Effigy's own child invocation. It inherits
    // no session access and no passphrase: it resolves its own access exactly
    // as a top-level invocation does (effigy#338).
    let env = vec![(
        OsString::from("EFFIGY_LINUX_RELEASE_TRIPLE"),
        OsString::from(target.release_triple()),
    )];
    let output = budget
        .run_command(
            effigy_repo_root,
            host_binary.as_os_str(),
            &args,
            None,
            &env,
            BoundedCommandPhase::WorkspacePreparation,
        )
        .map_err(|failure| {
            bounded_failure_error(policy, policy.primary_service.as_str(), budget, failure)
        })?;
    #[cfg(test)]
    if !output.status.success() {
        // The public bounded error intentionally omits captured bytes. Keep
        // private fixture failures diagnosable without changing that surface.
        eprintln!(
            "private artifact build stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.status.success())
}
