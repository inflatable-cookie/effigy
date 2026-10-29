use effigy_containers::{load_workspace_ownership_plan, EffectiveContainerPolicy};
use effigy_core::repo_markers::has_task_manifest;
use effigy_core::shell::shell_quote;
use effigy_manifest::TASK_MANIFEST_FILE;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runner::command_context::current_working_dir;
use crate::runner::container_command::{bounded_failure_error, bounded_phase_error};
use crate::runner::exec_command::{
    copy_file_into_service, copy_file_into_service_bounded, run_command_capture_until,
    BoundedCapture, BoundedCommandBudget, BoundedCommandPhase,
};
use crate::runner::workspace_client::context::{
    binary_dir, binary_path, context_dir, context_path, read_published_context,
    read_published_generation, render_digest, sha256, snapshot_dir, snapshot_path,
    ContextReadError, GuestBinaryIdentity, GuestReader, GuestWorkspace,
    WorkspaceActivationIdentity, WorkspaceBackendDescriptor, WorkspaceChannelLocator,
    WorkspaceContainerIdentity, WorkspaceContext, WorkspaceContextInput,
    WorkspaceEnrolmentCredential, WorkspaceEnvironmentIdentity, WorkspaceHostIdentity,
    WorkspacePathMap, WorkspacePinnedSource, WorkspaceRoute, BINARY_CURRENT_PATH, BINARY_ROOT,
    CHANNEL_PROTOCOL_VERSION, COMPOSED_MANIFEST_KIND, CONTEXT_CURRENT_PATH, ENROL_CREDENTIAL_PATH,
    ENROL_ROOT, HOST_REPREPARE_COMMAND,
};

#[cfg(test)]
use crate::runner::exec_command::BoundedCaptureFailure;

use super::RunnerError;

const CONTAINER_WORKSPACE_EFFIGY_STAGING_PATH_PREFIX: &str = "/tmp/effigy-host";
/// The published checksum the release download authenticates its bytes with.
const RELEASE_CHECKSUM_SUFFIX: &str = ".sha256";
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
#[cfg(test)]
const LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(300);
#[cfg(test)]
const LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LinuxWorkspaceTarget {
    X86_64Gnu,
    Aarch64Gnu,
    X86_64Musl,
    Aarch64Musl,
}

/// Where a Prepare may find the produced guest artifact (§8.9).
///
/// No variant builds one: `Local` consumes this checkout's produced artifact or
/// refuses, `Download` takes the release artifact, and `Auto` prefers the local
/// one and falls back to a current release download.
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
            Self::X86_64Musl => "x86_64-unknown-linux-musl",
            Self::Aarch64Musl => "aarch64-unknown-linux-musl",
        }
    }

    /// The artifact family an image needs: its platform **and** its C library
    /// (§8.9). An artifact built against the other C library does not run, so a
    /// guest whose libc this host cannot prove has no selectable artifact.
    pub(super) fn from_image(machine: &str, libc: &str) -> Result<Self, RunnerError> {
        let machine = match machine.trim() {
            "x86_64" | "amd64" => "x86_64",
            "aarch64" | "arm64" => "aarch64",
            other => {
                return Err(RunnerError::task_invocation(format!(
                    "workspace container image platform `{other}` has no pinned Linux guest \
                     artifact; run `{HOST_REPREPARE_COMMAND}` with a supported image"
                )))
            }
        };
        let libc = match libc.trim().to_ascii_lowercase().as_str() {
            "gnu" | "glibc" => "gnu",
            "musl" => "musl",
            other => {
                return Err(RunnerError::task_invocation(format!(
                    "the workspace container image reports no provable C library (`{other}`), so \
                     no guest artifact can be proven to run in it; an artifact matches the \
                     image's libc, never a guess. Run `{HOST_REPREPARE_COMMAND}` with an image \
                     that ships a glibc or musl loader"
                )))
            }
        };
        Ok(match (machine, libc) {
            ("x86_64", "gnu") => Self::X86_64Gnu,
            ("aarch64", "gnu") => Self::Aarch64Gnu,
            ("x86_64", "musl") => Self::X86_64Musl,
            _ => Self::Aarch64Musl,
        })
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
    let progress = super::WorkspaceOpProgress::start(
        repo_override.is_some(),
        super::PROBE_PERMISSION_PREP,
        false,
    );
    progress.update(
        progress_label,
        Some((0, perm_plan.targets.len() as u64)),
        None,
    );
    let mut backend = super::permissions::compose_backend(workspace_repo_root, policy);
    match super::permissions::ensure_workspace_permissions_ready_with(
        policy,
        container_name,
        allow_repair,
        &mut backend,
    ) {
        Ok(()) => {
            progress.finish_clear();
            Ok(())
        }
        Err(error) => {
            progress.fail_error(&error);
            Err(error)
        }
    }
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

/// The live facts an entry check compares the published context to.
pub(in crate::runner) struct LiveWorkspaceEntry {
    /// The environment record's activation generation, read after activation.
    pub(in crate::runner) environment: WorkspaceEnvironmentIdentity,
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
    target: LinuxWorkspaceTarget,
    facts: WorkspacePrepareFacts<'_>,
) -> Result<Option<PreparedWorkspaceGuest>, RunnerError> {
    let Some(workspace) = facts.operator_workspace.as_ref() else {
        return Ok(None);
    };
    let progress = super::WorkspaceOpProgress::start(false, super::PROBE_GUEST_PUBLICATION, false);
    progress.update("verifying guest artifact", None, None);
    let binary = match verify_guest_artifact(artifact, target) {
        Ok(binary) => binary,
        Err(error) => {
            progress.fail_error(&error);
            return Err(error);
        }
    };
    let paths = match workspace_path_map(facts.repo_root, workspace) {
        Ok(paths) => paths,
        Err(error) => {
            progress.fail_error(&error);
            return Err(error);
        }
    };
    let result = (|| {
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
            paths: paths.clone(),
            backend: facts.backend.clone(),
            sources: closure.pins,
            routes: workspace_routes(facts.policy),
            channel: facts.channel.clone(),
        });
        // The executable and the rendered snapshots are published before the
        // context that names them, so an entry never resolves a generation whose
        // executable or snapshot is still missing.
        progress.update(
            "installing pinned guest binary",
            None,
            Some("guest artifact verified".to_owned()),
        );
        install_pinned_guest_binary(
            guest,
            artifact,
            &binary,
            facts.artifact_install_identity.as_deref(),
        )?;
        progress.update(
            "publishing source snapshots",
            None,
            Some("guest binary installed".to_owned()),
        );
        publish_source_snapshots(guest, &closure.snapshots)?;
        // A restart or replacement outside Effigy's gate may race container I/O.
        // Never publish a context/receipt for a different identity than we probed.
        progress.now_waiting_for(super::PROBE_CONTAINER_IDENTITY);
        progress.update("probing workspace container identity", None, None);
        let observed = guest.probe_container_identity()?;
        if !activation.container.container_matches(&observed)
            || !activation.container.boot_matches(&observed)
        {
            return Err(RunnerError::task_invocation(format!(
                "workspace identity changed during publication; run `{HOST_REPREPARE_COMMAND}` on the host"
            )));
        }
        progress.now_waiting_for(super::PROBE_GUEST_PUBLICATION);
        progress.update(
            "publishing workspace context",
            None,
            Some(format!("container `{}`", observed.container_id)),
        );
        let context_generation = publish_context_generation(guest, &mut context)?;
        if let Some(enrolment) = facts.enrolment.as_ref() {
            progress.update(
                "publishing enrolment credential",
                None,
                Some("workspace context published".to_owned()),
            );
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

/// Publish the guest context and pinned executable for one container-mode
/// workspace: the host's Prepare/activation path (§8.2, §8.9).
///
/// The workspace handoff and already-running task path open an activation
/// here. Container up and a task's fresh activation call the same publisher
/// inside their existing owner run. Hybrid mode publishes nothing.
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
    let ctx = crate::runner::environment::context(repo_root)?;
    let env = crate::runner::environment::env_ref(policy)?;
    let mut prepared = None;
    effigy_containers::environment::activate_with(&ctx, &env, &[], |run| {
        prepared = publish_workspace_guest_in(repo_root, policy, run).map_err(|error| {
            effigy_containers::environment::EnvError::Invalid(error.to_string())
        })?;
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
) -> Result<Option<PreparedWorkspaceGuest>, RunnerError> {
    if std::env::var_os("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF").is_some() {
        return Ok(None);
    }
    let Some(operator_workspace) = operator_workspace_for_policy(repo_root, policy)? else {
        return Ok(None);
    };
    let expected = effigy_containers::environment::EnvRef::for_policy(policy)
        .map_err(crate::runner::environment::error)?;
    if run.fence().resource != expected.resource {
        return Err(RunnerError::task_invocation(
            "workspace publication belongs to another environment owner",
        ));
    }
    let mut prepared = None;
    run.publish_guest_with(|fence| {
        let result = publish_workspace_guest_effect(repo_root, policy, operator_workspace, fence)
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
    fence: &effigy_kernel::run::Fence,
) -> Result<PreparedWorkspaceGuest, RunnerError> {
    let mut guest = ComposeWorkspaceGuest::unbounded(repo_root, policy)?;
    // The host verified the backend's full id when it bound this guest. Every
    // read, probe and publication below targets that exact id, never an alias.
    let progress = super::WorkspaceOpProgress::start(false, super::PROBE_CONTAINER_IDENTITY, false);
    progress.update("probing workspace container identity", None, None);
    let bound = match guest.probe_container_identity() {
        Ok(bound) => bound,
        Err(error) => {
            progress.fail_error(&error);
            return Err(error);
        }
    };
    progress.update(
        "probing workspace container identity",
        None,
        Some(format!("container `{}`", bound.container_id)),
    );
    progress.finish_clear();
    let target = LinuxWorkspaceTarget::from_image(&bound.platform, &bound.libc)?;
    let (artifact, artifact_install_identity) =
        produced_linux_workspace_effigy_artifact(repo_root, target)?;
    prepare_workspace_guest(
        &mut guest,
        &artifact,
        target,
        WorkspacePrepareFacts {
            repo_root,
            policy,
            operator_workspace: Some(operator_workspace),
            // The session run and its enrolment credential belong to the
            // workspace session lane; this Prepare publishes the context they
            // are bound to and the host republishes it when the session is
            // admitted (§8.2, §8.7).
            session_run_id: None,
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
            // This owner-run fence is the generation activation will publish,
            // never a second read of an already-settled activation.
            environment: fence.clone(),
            artifact_install_identity,
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

/// Entry path: verify the pinned guest executable and the live identity the
/// published context is bound to.
///
/// This is not a Prepare. It never downloads, builds or reinstalls the guest
/// (§8.9); a workspace without a prepared context, or with one that pins
/// another version, container, VM boot or environment generation, refuses and
/// names the host re-prepare command. A hybrid environment (no declared
/// operator workspace) has no guest and provisions none.
pub(super) fn ensure_workspace_effigy_available_for_policy(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    _repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    if std::env::var_os("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF").is_some() {
        return Ok(());
    }
    // The mode boundary is the declared workspace, the same one Prepare uses: a
    // catalog service's default workspace user does not make this container
    // mode, so it does not require a guest either (§8.1).
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
    // Hybrid environments are not container-mode workspaces and have no guest
    // to verify; the bounded path keeps the same declared-workspace boundary.
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
            // A bounded capture failure (deadline, stream limit) already carries
            // its own phase detail; only a verification refusal is restated here.
            if error.json_error_details().is_some() {
                return Err(error);
            }
            // Bounded failures carry only static text, so the refusal names the
            // phase and the host re-prepare rather than a version, a digest or a
            // path from the container.
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

/// Check the published context against the host's own version, the guest's
/// installed executable, and the live container, VM boot and environment
/// generation it was prepared for. Pure reads: no write, no build, no download.
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
    // The identity fence: the container the guest can observe about itself, the
    // VM boot it belongs to and the image platform. A replaced container, a
    // reboot that retained the filesystem, or another image refuses here before
    // any effect.
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
    // The environment fence is the environment record's activation generation,
    // read after activation: a re-activation with unchanged files still moves it.
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
            // Handoff launches the stable entry, so authenticating only the
            // immutable object cannot authorize that invocation.
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

/// Whether a computed lowercase hex digest is the one a `sha256:<hex>` pin
/// names.
fn digest_matches(pinned: &str, computed_hex: &str) -> bool {
    pinned
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.eq_ignore_ascii_case(computed_hex.trim()))
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
/// Authenticate one guest artifact against the receipt its producer wrote.
///
/// The receipt names the target and the digest the bytes must have. It is
/// written by the artifact producer (the release download or the artifact
/// build), never by this verifier, so a sidecar alone cannot bless bytes.
fn verify_guest_artifact(
    artifact: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<GuestBinaryIdentity, RunnerError> {
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
    let digest = render_digest(digest);
    let receipt = workspace_artifact_receipt(artifact)?;
    if receipt.get("release_triple").map(String::as_str) != Some(target.release_triple()) {
        return Err(RunnerError::task_invocation(format!(
            "linux workspace artifact `{}` has no receipt naming target `{}`; run \
             `EFFIGY_LINUX_RELEASE_TRIPLE={} effigy workspace:linux:artifact` on the host",
            artifact.display(),
            target.release_triple(),
            target.release_triple()
        )));
    }
    let recorded = receipt.get("sha256").ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "linux workspace artifact receipt for `{}` names no digest, so its bytes cannot be \
             authenticated; rebuild it with `EFFIGY_LINUX_RELEASE_TRIPLE={} effigy \
             workspace:linux:artifact`",
            artifact.display(),
            target.release_triple()
        ))
    })?;
    if !digest_matches(&digest, recorded) {
        return Err(RunnerError::task_invocation(format!(
            "linux workspace artifact `{}` is {digest} but its receipt records sha256:{recorded}; \
             rebuild it with `EFFIGY_LINUX_RELEASE_TRIPLE={} effigy workspace:linux:artifact`",
            artifact.display(),
            target.release_triple()
        )));
    }
    Ok(GuestBinaryIdentity {
        version,
        digest,
        protocol: CHANNEL_PROTOCOL_VERSION,
    })
}

/// Each target's producer receipt lives beside its own binary, so producing
/// another target cannot invalidate a still-current local artifact.
pub(super) fn workspace_artifact_receipt_path(artifact: &Path) -> PathBuf {
    let mut path = artifact.as_os_str().to_owned();
    path.push(".receipt");
    PathBuf::from(path)
}

/// The `key=value` receipt an artifact producer writes beside its artifact.
fn workspace_artifact_receipt(artifact: &Path) -> Result<BTreeMap<String, String>, RunnerError> {
    let path = workspace_artifact_receipt_path(artifact);
    let raw = std::fs::read_to_string(&path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot read linux workspace artifact receipt `{}`: {error}",
            path.display()
        ))
    })?;
    let mut fields = BTreeMap::new();
    for line in raw.lines() {
        if let Some((key, value)) = line.trim().split_once('=') {
            fields.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    Ok(fields)
}

pub(super) fn operator_workspace_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<OperatorWorkspace>, RunnerError> {
    // The mode boundary is the environment's *declared* operator workspace,
    // never a mode name, an environment name or a catalog service's default
    // user. §8.1: a hybrid environment that happens to resolve a user gets no
    // guest.
    if !environment_declares_operator_workspace(repo_root, policy)? {
        return Ok(None);
    }
    let user = policy
        .workspace_user
        .as_deref()
        .map(str::trim)
        .filter(|user| !user.is_empty());
    let home = policy
        .workspace_home
        .as_deref()
        .map(str::trim)
        .filter(|home| Path::new(home).is_absolute());
    let (Some(user), Some(home)) = (user, home) else {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` declares an operator workspace but has no usable workspace identity; \
             configure a non-empty `user` and absolute `home` in its system/workspace declaration, \
             then run `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            policy.name,
        )));
    };
    let guest_checkout =
        effigy_containers::load_container_exec_working_dir(repo_root, Some(&policy.name))
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(Some(OperatorWorkspace {
        user: user.to_owned(),
        home: home.to_owned(),
        guest_checkout: guest_checkout.display().to_string(),
    }))
}

/// Whether a `[systems.*]` workspace names this container (§8.1).
///
/// This asks the manifest resolver the containers engine itself uses, so a
/// catalog service's default workspace identity never promotes a hybrid
/// environment to container mode.
fn environment_declares_operator_workspace(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    let manifest =
        crate::runner::manifest::load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    effigy_containers::resolve_operator_workspace_for_container(&manifest, Some(&policy.name))
        .map(|workspace| workspace.is_some())
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
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

/// The environment engine's activation generation (§8.2, §8.11).
///
/// The environment record advances one generation per activation, so a
/// re-activation with unchanged files still moves the fence. A workspace with
/// no activated environment has nothing to fence against.
pub(super) fn environment_generation(
    policy: &EffectiveContainerPolicy,
) -> Result<WorkspaceEnvironmentIdentity, RunnerError> {
    let record = crate::runner::environment::record(policy).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "environment `{}` has no activation record, so the workspace context has no \
             environment generation to fence; run `{HOST_REPREPARE_COMMAND}` on the host",
            policy.name
        ))
    })?;
    if record.state != effigy_containers::environment::EnvState::Active {
        return Err(RunnerError::task_invocation(format!(
            "environment `{}` has no completed activation; run `{HOST_REPREPARE_COMMAND}` on the host",
            policy.name
        )));
    }
    Ok(WorkspaceEnvironmentIdentity {
        resource_id: format!("env:{}", policy.name),
        generation: record.generation.get().to_string(),
    })
}

/// The pinned inputs a container-mode guest reads locally (§8.2).
///
/// `pins` is the closure the context carries: the root manifest, the source
/// lock, every bundle and catalog source the host composed from, each
/// composition fragment, and the rendered composed manifest. `snapshots` is
/// the rendered document published into the guest store, so a bundle- or
/// catalog-provided task is readable without the guest reading a cache the
/// host did not approve.
pub(super) struct WorkspaceSourceClosure {
    pub(super) pins: Vec<WorkspacePinnedSource>,
    pub(super) snapshots: Vec<(String, Vec<u8>)>,
}

fn workspace_source_closure(
    repo_root: &Path,
    paths: &WorkspacePathMap,
) -> Result<WorkspaceSourceClosure, RunnerError> {
    let host_checkout = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let manifest_path = host_checkout.join(TASK_MANIFEST_FILE);
    let mut pins = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    // Checkout-visible inputs the local task read verifies before parsing.
    for (kind, relative) in [
        ("manifest", TASK_MANIFEST_FILE.to_owned()),
        ("source-lock", ".effigy/sources.lock.json".to_owned()),
    ] {
        let path = host_checkout.join(&relative);
        let Some(bytes) = read_optional_bytes(&path)? else {
            continue;
        };
        seen.insert(relative.clone());
        pins.push(WorkspacePinnedSource {
            kind: kind.to_owned(),
            name: relative.clone(),
            digest: crate::runner::workspace_client::context::sha256_text(&bytes),
            snapshot: Some(guest_checkout_path(paths, &relative)),
        });
    }

    // The composed manifest is the complete task closure: every `[tasks]`
    // entry the bundle, the catalogs and the checkout's own fragments
    // contribute. It is published immutably, so reading it never refreshes a
    // source or writes host state.
    let composed = crate::runner::manifest::load_task_manifest_with_inspection(&manifest_path)?;
    let composed_bytes = composed.effective_manifest.clone().into_bytes();
    let composed_digest = crate::runner::workspace_client::context::sha256_text(&composed_bytes);
    let composed_snapshot = snapshot_path(&composed_digest);
    pins.push(WorkspacePinnedSource {
        kind: COMPOSED_MANIFEST_KIND.to_owned(),
        name: TASK_MANIFEST_FILE.to_owned(),
        digest: composed_digest.clone(),
        snapshot: Some(composed_snapshot.clone()),
    });

    // Every file the composition read, so each pin names the bytes its source
    // contributed and the guest path those bytes are readable at.
    let canonical = |path: PathBuf| path.canonicalize().unwrap_or(path);
    let catalog_roots = effigy_manifest::catalog_source_roots(&manifest_path)
        .map(|roots| {
            let mut declared = roots
                .declared
                .into_iter()
                .map(|(_, path)| canonical(path))
                .collect::<Vec<_>>();
            if let Some(bundle) = roots.bundle {
                declared.push(canonical(bundle));
            }
            declared
        })
        .unwrap_or_default();
    let bundle_root = composed
        .bundle_root
        .as_ref()
        .map(|root| canonical(root.clone()));
    for fragment in &composed.evaluation_order {
        let canonical = fragment
            .canonicalize()
            .unwrap_or_else(|_| fragment.to_path_buf());
        let Ok(relative) = canonical.strip_prefix(&host_checkout) else {
            continue;
        };
        let relative = relative.display().to_string();
        if !seen.insert(relative.clone()) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&canonical) else {
            continue;
        };
        let kind = if bundle_root
            .as_ref()
            .is_some_and(|root| canonical.starts_with(root))
        {
            "bundle"
        } else if catalog_roots.iter().any(|root| canonical.starts_with(root)) {
            "catalog"
        } else {
            "manifest"
        };
        pins.push(WorkspacePinnedSource {
            kind: kind.to_owned(),
            name: relative.clone(),
            digest: crate::runner::workspace_client::context::sha256_text(&bytes),
            snapshot: Some(guest_checkout_path(paths, &relative)),
        });
    }

    // The pinned bundle and catalog identities. Their rendered result is the
    // composed snapshot, which each pin names.
    if let Some(root) = bundle_root.as_ref() {
        if let Some(pin) = source_directory_pin("bundle", root, &host_checkout, &composed_snapshot)
        {
            pins.push(pin);
        }
    }
    for root in &catalog_roots {
        if let Some(pin) = source_directory_pin("catalog", root, &host_checkout, &composed_snapshot)
        {
            pins.push(pin);
        }
    }

    Ok(WorkspaceSourceClosure {
        pins,
        snapshots: vec![(composed_digest, composed_bytes)],
    })
}

/// One pinned bundle or catalog source: the digest of its manifest fragments and
/// the rendered snapshot they produced.
fn source_directory_pin(
    kind: &str,
    root: &Path,
    host_checkout: &Path,
    rendered_snapshot: &str,
) -> Option<WorkspacePinnedSource> {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !canonical.is_dir() {
        return None;
    }
    let name = canonical
        .strip_prefix(host_checkout)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| canonical.display().to_string());
    Some(WorkspacePinnedSource {
        kind: kind.to_owned(),
        name,
        digest: manifest_fragment_digest(&canonical)?,
        snapshot: Some(rendered_snapshot.to_owned()),
    })
}

/// A deterministic digest over one source directory's manifest fragments: its
/// `*.toml` files, sorted by relative path, each contributing its path, length
/// and bytes.
pub(super) fn manifest_fragment_digest(root: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut fragments = Vec::new();
    collect_toml_files(root, root, &mut fragments)?;
    fragments.sort();
    let mut digest = Sha256::new();
    for relative in fragments {
        let path = root.join(&relative);
        let bytes = std::fs::read(&path).ok()?;
        digest.update(relative.display().to_string().as_bytes());
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(&bytes);
    }
    let bytes: [u8; 32] = digest.finalize().into();
    Some(crate::runner::workspace_client::context::render_digest(
        bytes,
    ))
}

fn collect_toml_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Option<()> {
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        let file_type = entry.file_type().ok()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_toml_files(root, &path, out)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("toml") {
            out.push(path.strip_prefix(root).ok()?.to_path_buf());
        }
    }
    Some(())
}

fn read_optional_bytes(path: &Path) -> Result<Option<Vec<u8>>, RunnerError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(RunnerError::task_invocation(format!(
            "cannot read workspace input `{}`: {error}",
            path.display()
        ))),
    }
}

/// Publish the rendered source snapshots the pins name, before the context that
/// names them: one immutable directory per rendered digest.
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

/// The guest path a checkout-relative file is readable at.
fn guest_checkout_path(paths: &WorkspacePathMap, relative: &str) -> String {
    format!(
        "{checkout}/{relative}",
        checkout = paths.guest_checkout.trim_end_matches('/'),
        relative = relative.trim_start_matches('/')
    )
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
    Unbounded {
        target_id: String,
    },
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
    fn unbounded(
        repo_root: &'a Path,
        policy: &'a EffectiveContainerPolicy,
    ) -> Result<Self, RunnerError> {
        let binding = crate::runner::container_runtime_prep::verify_service_binding(
            repo_root,
            policy,
            policy.primary_service.as_str(),
            None,
        )?;
        Ok(Self {
            repo_root,
            policy,
            command: GuestCommand::Unbounded {
                target_id: binding.id().to_owned(),
            },
        })
    }

    fn target_id(&self) -> &str {
        match &self.command {
            GuestCommand::Unbounded { target_id } => target_id,
            GuestCommand::Target { target_id, .. } => target_id,
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
            GuestCommand::Unbounded { target_id } => {
                let compose = effigy_runtime::container_manager::compose_invocation_plan_from_args(
                    self.repo_root,
                    self.policy,
                    compose_args,
                    effigy_containers::ContainerAction::Exec,
                    "workspace guest command",
                )
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
                let args =
                    crate::runner::exec_command::verified_target_exec_args(&compose, target_id)?;
                let plan = effigy_runtime::container_manager::runtime_invocation_plan(
                    self.repo_root,
                    self.policy,
                    "docker",
                    &args,
                    effigy_containers::ContainerAction::Exec,
                    "workspace guest command",
                )
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
                let output = run_command_capture_until(
                    &plan.repo_root,
                    plan.program.as_os_str(),
                    &plan.args,
                    None,
                    None,
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
            GuestCommand::Unbounded { target_id } => copy_file_into_service(
                self.repo_root,
                self.policy,
                target_id,
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

    fn file_sha256(&mut self, path: &str) -> Result<Option<String>, RunnerError> {
        // One digest line, never the file: a bounded caller must not pay for
        // the executable's size in its own capture budget.
        let script = format!(
            "p={path}\n[ -f \"$p\" ] || exit 3\n\
             if command -v sha256sum >/dev/null 2>&1; then sha256sum \"$p\"\n\
             elif command -v shasum >/dev/null 2>&1; then shasum -a 256 \"$p\"\n\
             elif command -v openssl >/dev/null 2>&1; then openssl dgst -sha256 \"$p\"\n\
             else exit 4\nfi",
            path = shell_quote(path)
        );
        let output = self.run_script(&script, false)?;
        match output.status.code() {
            Some(0) => {}
            Some(3) => return Ok(None),
            Some(4) => {
                return Err(RunnerError::task_invocation(format!(
                    "workspace container `{}` has no SHA-256 utility, so the pinned guest \
                     executable cannot be authenticated; run `{HOST_REPREPARE_COMMAND}` with an \
                     image that provides sha256sum, shasum or openssl",
                    self.policy.name
                )))
            }
            _ => {
                return Err(RunnerError::task_invocation(format!(
                    "cannot hash `{path}` in workspace container `{}`: {}",
                    self.policy.name,
                    String::from_utf8_lossy(&output.stderr).trim()
                )))
            }
        }
        let rendered = String::from_utf8_lossy(&output.stdout);
        rendered
            .split_whitespace()
            .find(|token| token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .map(|token| token.to_ascii_lowercase())
            .map(Some)
            .ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "SHA-256 tool in workspace container `{}` returned no digest for `{path}`",
                    self.policy.name
                ))
            })
    }
}

impl GuestWorkspace for ComposeWorkspaceGuest<'_> {
    fn read_link(&mut self, path: &str) -> Result<Option<String>, RunnerError> {
        let script = format!(
            "[ -L {path} ] || exit 3\nreadlink {path}",
            path = shell_quote(path)
        );
        let output = self.run_script(&script, false)?;
        match output.status.code() {
            Some(0) => String::from_utf8(output.stdout)
                .map(|target| Some(target.trim_end_matches('\n').to_owned()))
                .map_err(|error| RunnerError::task_invocation(format!(
                    "cannot decode workspace entry `{path}`: {error}; run `{HOST_REPREPARE_COMMAND}` on the host"
                ))),
            Some(3) => Ok(None),
            _ => Err(RunnerError::task_invocation(format!(
                "cannot read workspace entry `{path}`: {}; run `{HOST_REPREPARE_COMMAND}` on the host",
                String::from_utf8_lossy(&output.stderr).trim()
            ))),
        }
    }

    fn ensure_dir(&mut self, path: &str, mode: u32) -> Result<(), RunnerError> {
        let script = format!(
            "mkdir -p {path} && chmod {mode:04o} {path}",
            path = shell_quote(path)
        );
        self.run_root(&script, "make a guest directory")
    }

    fn write_file(&mut self, path: &str, bytes: &[u8], mode: u32) -> Result<(), RunnerError> {
        let staging = render_workspace_effigy_staging_path();
        let host_file = stage_host_bytes(bytes)?;
        let staged = self.stage(host_file.path(), &staging);
        drop(host_file);
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
        // Placement comes from the host's verified backend binding, and this
        // probe executes against that exact id. Hostnames and cgroup text are
        // guest-controlled claims, never a substitute for the binding.
        let output = self.run_script(
            "printf 'boot=%s\\n' \"$(cat /proc/sys/kernel/random/boot_id)\"\n\
             printf 'platform=%s\\n' \"$(uname -m)\"\n\
             printf 'libc_musl=%s\\n' \"$(ls /lib/ld-musl-*.so.1 2>/dev/null | head -n1)\"\n\
             printf 'libc_glibc=%s\\n' \"$(ls /lib/*-linux-gnu*/libc.so.6 /lib/libc.so.6 /lib64/libc.so.6 2>/dev/null | head -n1)\"",
            false,
        )?;
        if !output.status.success() {
            return Err(RunnerError::task_invocation(format!(
                "cannot read the workspace container identity for `{}`: {}",
                self.policy.name,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        parse_container_identity(&String::from_utf8_lossy(&output.stdout), self.target_id())
            .map_err(|_| {
                RunnerError::task_invocation(format!(
                    "workspace container `{}` did not report its identity; run \
                 `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
                    self.policy.name
                ))
            })
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

/// Combine a probe taken through an exact target exec with the host-verified
/// full backend id it targeted. Neither a hostname nor cgroup text grants
/// placement, even when it looks like a full or short container id.
pub(super) fn parse_container_identity(
    rendered: &str,
    verified_target_id: &str,
) -> Result<WorkspaceContainerIdentity, RunnerError> {
    let field = |name: &str| {
        rendered
            .lines()
            .find_map(|line| line.trim().strip_prefix(name)?.strip_prefix('='))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let container_id = (verified_target_id.len() == 64
        && verified_target_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()))
    .then(|| verified_target_id.to_ascii_lowercase());
    let vm_boot_id = field("boot");
    let platform = field("platform");
    // The loader a guest can see tells the host which C library the image has,
    // and therefore which artifact family can run in it (§8.9). An image with
    // neither marker is unproven, not glibc by default.
    let libc = field("libc").unwrap_or_else(|| {
        if field("libc_musl").is_some() {
            "musl".to_owned()
        } else if field("libc_glibc").is_some() {
            "gnu".to_owned()
        } else {
            String::new()
        }
    });
    match (container_id, vm_boot_id, platform) {
        (Some(container_id), Some(vm_boot_id), Some(platform)) => Ok(WorkspaceContainerIdentity {
            container_id,
            vm_boot_id,
            platform,
            libc,
        }),
        _ => Err(RunnerError::task_invocation(
            "the container identity probe requires a verified full backend id, boot id and platform",
        )),
    }
}

/// Stage bytes for a host → container copy. The file is private to this
/// Prepare run, created exclusively with owner-only permissions in a directory
/// only this run owns, and removed when the guard drops — on every outcome,
/// including an error.
pub(super) fn stage_host_bytes(bytes: &[u8]) -> Result<StagedHostFile, RunnerError> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let directory = std::env::temp_dir().join(format!(
        "effigy-workspace-stage-{pid}",
        pid = std::process::id()
    ));
    match std::fs::create_dir(&directory) {
        Ok(()) => {
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).map_err(
                |error| {
                    RunnerError::task_invocation(format!(
                        "cannot make workspace staging directory `{}` private: {error}",
                        directory.display()
                    ))
                },
            )?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(&directory).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "cannot inspect workspace staging directory `{}`: {error}",
                    directory.display()
                ))
            })?;
            if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
                return Err(RunnerError::task_invocation(format!(
                    "workspace staging directory `{}` is not a private directory this run owns",
                    directory.display()
                )));
            }
        }
        Err(error) => {
            return Err(RunnerError::task_invocation(format!(
                "cannot create workspace staging directory `{}`: {error}",
                directory.display()
            )))
        }
    }
    let path = directory.join(format!(
        "stage-{counter}",
        counter = WORKSPACE_EFFIGY_STAGING_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    // `create_new` refuses a pre-existing path, so a planted symlink can never
    // receive the bytes; 0600 is the only mode the file ever has.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot stage workspace bytes at `{}`: {error}",
                path.display()
            ))
        })?;
    file.write_all(bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot write staged workspace bytes at `{}`: {error}",
            path.display()
        ))
    })?;
    drop(file);
    Ok(StagedHostFile {
        path,
        directory: directory.clone(),
        remove_directory: true,
    })
}

/// A host staging file that removes itself and its run-owned directory.
pub(super) struct StagedHostFile {
    path: PathBuf,
    directory: PathBuf,
    remove_directory: bool,
}

impl StagedHostFile {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StagedHostFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        if self.remove_directory {
            let _ = std::fs::remove_dir(&self.directory);
        }
    }
}
/// Consume the version-locked guest artifact this host may hand to a workspace
/// (§8.2, §8.9).
///
/// Prepare never builds a guest artifact from an Effigy checkout at consumer
/// runtime: it consumes bytes a release download or an explicit `effigy
/// workspace:linux:artifact` already produced, or refuses naming that command.
/// The bytes are authenticated against the producer receipt before publication.
pub(super) fn produced_linux_workspace_effigy_artifact(
    repo_root: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<(PathBuf, Option<String>), RunnerError> {
    let source = configured_linux_workspace_artifact_source()?;
    let release_identity = workspace_effigy_release_install_identity();
    let rebuild = format!(
        "`EFFIGY_LINUX_RELEASE_TRIPLE={triple} effigy workspace:linux:artifact` on the host, or \
         set `{env}=download` to download the published release artifact",
        triple = target.release_triple(),
        env = EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
    );
    if source != LinuxWorkspaceArtifactSource::Download {
        if let Some(effigy_repo_root) = resolve_local_effigy_repo_root(repo_root)? {
            let artifact = effigy_repo_root.join(target.artifact_relative_path());
            if produced_linux_workspace_artifact_is_current(&artifact, target) {
                let host_binary = std::env::current_exe().map_err(RunnerError::Cwd)?;
                let anchor = resolve_local_workspace_effigy_freshness_anchor(
                    &host_binary,
                    &effigy_repo_root,
                );
                return Ok((
                    artifact,
                    Some(workspace_effigy_local_install_identity(&anchor)?),
                ));
            }
            // `local` means this checkout's artifact or nothing: a stale one is
            // never silently replaced by a download.
            if source == LinuxWorkspaceArtifactSource::Local {
                return Err(RunnerError::task_invocation(format!(
                    "no produced linux workspace artifact for `{}` is current at `{}` (a stale \
                     artifact is not consumed); run {rebuild}",
                    target.release_triple(),
                    artifact.display(),
                )));
            }
        }
    }
    // A release download is the other produced artifact: the cache is reused
    // only when its identity sidecar names this release, and fresh bytes must
    // match the checksum the release publishes beside them.
    let cache = linux_workspace_effigy_cache_path(target)?;
    if source == LinuxWorkspaceArtifactSource::Download
        || linux_workspace_release_cache_is_current(&cache, &release_identity)
    {
        let artifact = ensure_downloaded_linux_workspace_effigy_artifact(target)?;
        return Ok((artifact, Some(release_identity)));
    }
    Err(RunnerError::task_invocation(format!(
        "no produced linux workspace artifact for `{}` is available on this host (no local Effigy \
         artifact and no current release download at `{}`); run {rebuild}",
        target.release_triple(),
        cache.display(),
    )))
}

/// Whether a local artifact is already produced for this exact host version and
/// target. The bytes are authenticated by [`verify_guest_artifact`].
pub(super) fn produced_linux_workspace_artifact_is_current(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
) -> bool {
    artifact_path.is_file()
        && read_trimmed_identity_file(&workspace_effigy_active_version_file(artifact_path))
            .as_deref()
            == Some(effigy_core::build_info::active_version().as_str())
        && linux_workspace_effigy_artifact_receipt_matches_target(artifact_path, target)
}

#[cfg(test)]
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
            (true, LinuxWorkspaceTarget::X86_64Musl) => {
                "bounded container command build left no current workspace artifact; rebuild with EFFIGY_LINUX_RELEASE_TRIPLE=x86_64-unknown-linux-musl effigy workspace:linux:artifact"
            }
            (true, LinuxWorkspaceTarget::Aarch64Musl) => {
                "bounded container command build left no current workspace artifact; rebuild with EFFIGY_LINUX_RELEASE_TRIPLE=aarch64-unknown-linux-musl effigy workspace:linux:artifact"
            }
            (false, LinuxWorkspaceTarget::X86_64Gnu) => {
                "bounded container command could not refresh the stale workspace artifact; rebuild with EFFIGY_LINUX_RELEASE_TRIPLE=x86_64-unknown-linux-gnu effigy workspace:linux:artifact"
            }
            (false, LinuxWorkspaceTarget::Aarch64Gnu) => {
                "bounded container command could not refresh the stale workspace artifact; rebuild with EFFIGY_LINUX_RELEASE_TRIPLE=aarch64-unknown-linux-gnu effigy workspace:linux:artifact"
            }
            (false, LinuxWorkspaceTarget::X86_64Musl) => {
                "bounded container command could not refresh the stale workspace artifact; rebuild with EFFIGY_LINUX_RELEASE_TRIPLE=x86_64-unknown-linux-musl effigy workspace:linux:artifact"
            }
            (false, LinuxWorkspaceTarget::Aarch64Musl) => {
                "bounded container command could not refresh the stale workspace artifact; rebuild with EFFIGY_LINUX_RELEASE_TRIPLE=aarch64-unknown-linux-musl effigy workspace:linux:artifact"
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

#[cfg(test)]
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

#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
pub(super) fn linux_workspace_effigy_artifact_is_current(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
    expected_identity: &str,
) -> bool {
    artifact_path.is_file()
        && linux_workspace_effigy_artifact_receipt_matches_target(artifact_path, target)
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
#[cfg(test)]
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
        && linux_workspace_effigy_artifact_receipt_matches_target(artifact_path, target)
}

pub(super) fn read_trimmed_identity_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
}

/// Name what is stale and the supported rebuild. Reports host and artifact
/// versions and identities, with `unknown` where a sidecar is absent.
#[cfg(test)]
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
    let receipt = if linux_workspace_effigy_artifact_receipt_matches_target(artifact_path, target) {
        "matches target"
    } else {
        "missing or names another target"
    };
    format!(
        "linux workspace artifact `{path}` is not current for `{triple}`: host version `{host}`, host identity `{expected_identity}`, artifact version `{artifact_version}`, artifact identity `{artifact_identity}`, artifact receipt {receipt}; rebuild with `EFFIGY_LINUX_RELEASE_TRIPLE={triple} effigy workspace:linux:artifact` from `{root}`",
        path = artifact_path.display(),
        triple = target.release_triple(),
        host = effigy_core::build_info::active_version(),
        root = effigy_repo_root.display(),
    )
}

#[cfg(test)]
pub(super) fn linux_workspace_artifact_offline_hint(
    error: RunnerError,
    refusal: &str,
) -> RunnerError {
    RunnerError::task_invocation(format!(
        "{error}; {refusal}. A rebuild needs Docker Hub for `ubuntu:22.04` / `effigy-linux-release-builder`: pre-load those images for an offline rebuild, run the rebuild command above when Hub is reachable, or set `{EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV}=download` to use a GitHub release binary for this version"
    ))
}

#[cfg(test)]
pub(super) fn linux_workspace_artifact_lock_path(effigy_repo_root: &Path) -> PathBuf {
    effigy_repo_root
        .join(".effigy/locks")
        .join("task-workspace-linux-artifact.lock")
}

#[cfg(test)]
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
    if linux_workspace_release_cache_is_current(&cache_path, &install_identity)
        && verify_guest_artifact(&cache_path, target).is_ok()
    {
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
    let digest = download_linux_workspace_effigy_release(&url, &cache_path)?;
    // The download records the receipt its consumer authenticates against, so
    // a cached artifact is never trusted on a version sidecar alone.
    write_workspace_artifact_receipt(&cache_path, target, &digest, "release-download")?;
    ensure_workspace_effigy_active_version_file(
        &cache_path,
        &effigy_core::build_info::active_version(),
    )?;
    ensure_workspace_effigy_install_identity_file(&cache_path, &install_identity)?;
    Ok(cache_path)
}

/// Write the producer receipt an artifact is authenticated against.
fn write_workspace_artifact_receipt(
    artifact: &Path,
    target: LinuxWorkspaceTarget,
    digest: &str,
    build_kind: &str,
) -> Result<(), RunnerError> {
    let path = workspace_artifact_receipt_path(artifact);
    let hex = digest.strip_prefix("sha256:").unwrap_or(digest);
    let body = format!(
        "release_triple={triple}\nsha256={hex}\nbuild_kind={build_kind}\n",
        triple = target.release_triple(),
    );
    std::fs::write(&path, body).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write linux workspace artifact receipt `{}`: {error}",
            path.display()
        ))
    })
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

/// Download one release artifact and authenticate it against the checksum the
/// release publishes beside it. Returns the `sha256:<hex>` the bytes matched.
pub(super) fn download_linux_workspace_effigy_release(
    url: &str,
    dest: &Path,
) -> Result<String, RunnerError> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to initialize release download client for `{url}`: {error}"
            ))
        })?;
    let bytes = client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.bytes())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to download linux effigy release artifact from `{url}`: {error}"
            ))
        })?;
    let expected = published_release_checksum(&client, url)?;
    let digest = render_digest(sha256(&bytes));
    if !digest_matches(&digest, &expected) {
        return Err(RunnerError::task_invocation(format!(
            "linux effigy release artifact from `{url}` is {digest} but the published checksum is \
             sha256:{expected}; refusing to install bytes the release does not name"
        )));
    }

    let tmp_path = dest.with_extension("tmp");
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
    Ok(digest)
}

/// The checksum a release publishes beside an artifact (`<asset>.sha256`).
///
/// A release without one cannot be authenticated, so the download refuses
/// rather than trusting bytes plus a self-written version sidecar (§8.9).
fn published_release_checksum(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<String, RunnerError> {
    let checksum_url = format!("{url}{RELEASE_CHECKSUM_SUFFIX}");
    let body = client
        .get(&checksum_url)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.text())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "the release publishes no checksum at `{checksum_url}` ({error}); the workspace \
                 guest artifact cannot be authenticated. Rebuild it locally with \
                 `effigy workspace:linux:artifact`, or publish a checksum beside the release \
                 artifact"
            ))
        })?;
    body.split_whitespace()
        .find(|token| token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map(|token| token.to_ascii_lowercase())
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "the published checksum at `{checksum_url}` names no SHA-256 digest"
            ))
        })
}

pub(super) fn linux_workspace_effigy_artifact_receipt_matches_target(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
) -> bool {
    let receipt_path = workspace_artifact_receipt_path(artifact_path);
    let Ok(raw) = std::fs::read_to_string(receipt_path) else {
        return false;
    };
    raw.lines()
        .any(|line| line.trim() == format!("release_triple={}", target.release_triple()))
}

#[cfg(test)]
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

#[cfg(test)]
mod verified_target_tests {
    use super::*;
    use crate::contract_test_support::EnvGuard;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn unbounded_guest_probes_and_copies_keep_the_verified_target() {
        let fixture = effigy_test_support::Fixture::new("workspace-verified-target-").unwrap();
        let bin = fixture.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let capture = fixture.path().join("args");
        let backend = bin.join("docker");
        std::fs::write(
            &backend,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nprintf 'boot=boot-a\\nplatform=x86_64\\nlibc=gnu\\nhostname=spoofed\\n'\n",
                shell_quote(&capture.display().to_string()),
            ),
        ).unwrap();
        std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o755)).unwrap();
        let _env = EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!(
                    "{}:{}",
                    bin.display(),
                    std::env::var("PATH").unwrap()
                )),
            ),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        ]);
        let mut policy = crate::runner::test_support::effective_container_policy(
            "dev",
            "dev",
            "app",
            fixture.path().join("compose.yml"),
        );
        policy.repo_root = fixture.path().to_owned();
        let target = "a1".repeat(32);
        let mut guest = ComposeWorkspaceGuest {
            repo_root: fixture.path(),
            policy: &policy,
            command: GuestCommand::Unbounded {
                target_id: target.clone(),
            },
        };
        assert_eq!(
            guest.probe_container_identity().unwrap().container_id,
            target
        );
        let args = std::fs::read_to_string(&capture).unwrap();
        assert!(args.lines().any(|arg| arg == target));
        assert!(!args.lines().any(|arg| arg == "app"));
        let source = fixture.path().join("artifact");
        std::fs::write(&source, "pinned bytes").unwrap();
        guest.stage(&source, "/run/effigy/staged").unwrap();
        let args = std::fs::read_to_string(&capture).unwrap();
        assert!(args
            .lines()
            .any(|arg| arg == format!("{target}:/run/effigy/staged")));
        assert!(!args.contains("app:/run/effigy/staged"));
    }
}
