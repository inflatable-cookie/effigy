#[path = "reserved_guest.rs"]
pub(in crate::runner) mod reserved_guest;
#[path = "staging.rs"]
pub(in crate::runner) mod staging;

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
/// One identity probe reads four short `/proc` fields and two `ls` lines.
const WORKSPACE_IDENTITY_STDOUT_BYTES: u64 = 64 * 1024;
const WORKSPACE_IDENTITY_STDERR_BYTES: u64 = 8 * 1024;
const LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(300);
const LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runner) enum LinuxWorkspaceTarget {
    X86_64Gnu,
    Aarch64Gnu,
    X86_64Musl,
    Aarch64Musl,
}

/// Where a host Prepare gets its guest artifact (§8.9). The policy selects a
/// source; the host's provenance decides what `Auto` may use.
///
/// `Auto` builds the artifact in a development host's own checkout, once per
/// stale identity, and downloads the matching release asset on a release host.
/// `Local` names a checkout explicitly and builds there. `Download` takes the
/// release asset, and refuses on a development host, whose release would be a
/// substitute. None of these builds during publication or task entry.
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
    pub(in crate::runner) fn release_triple(self) -> &'static str {
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
    pub(in crate::runner) fn from_image(machine: &str, libc: &str) -> Result<Self, RunnerError> {
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

    /// The image facts this artifact family was built for. The release triple in
    /// the producer's receipt is its own record, so this is the artifact side of
    /// the independent platform comparison, never a read of the image.
    pub(in crate::runner) fn image_platform(
        self,
    ) -> effigy_kernel::workspace_session::ImagePlatform {
        use effigy_kernel::workspace_session::{ImagePlatform, LoaderFamily};
        let (architecture, loader) = match self {
            Self::X86_64Gnu => ("amd64", LoaderFamily::Gnu),
            Self::Aarch64Gnu => ("arm64", LoaderFamily::Gnu),
            Self::X86_64Musl => ("amd64", LoaderFamily::Musl),
            Self::Aarch64Musl => ("arm64", LoaderFamily::Musl),
        };
        ImagePlatform {
            os: "linux".into(),
            architecture: architecture.into(),
            // A release triple names a machine type, never an optional CPU
            // variant, so the artifact claims no variant it was not built for.
            variant: String::new(),
            loader,
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
    if repo_override.is_none()
        && (!std::io::IsTerminal::is_terminal(&std::io::stderr())
            || effigy_ui::theme::is_ci_environment())
    {
        super::emit_workspace_info(&progress_label, false);
    }
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
            session_generation: facts.session_generation,
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
    // The artifact is acquired before the activation run opens: a build or a
    // download is long, and the publication below only consumes what is ready.
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
    check_acquired_artifact(artifact, target)?;
    prepare_workspace_guest(
        &mut guest,
        &artifact.artifact,
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
            // This owner-run fence is the generation activation will publish,
            // never a second read of an already-settled activation.
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

/// Entry path: verify the pinned guest executable and the live identity the
/// published context is bound to.
///
/// This is not a Prepare. It never downloads, builds or reinstalls the guest
/// (§8.9), and it runs no producer; the host Prepare acquired the artifact
/// beforehand. A workspace without a prepared context, or with one that pins
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
pub(in crate::runner) fn verify_guest_artifact(
    artifact: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<GuestBinaryIdentity, RunnerError> {
    verify_guest_artifact_until(artifact, target, None)
}

/// The same authentication, bounded by the publication's one absolute deadline:
/// the digest aborts with a deadline refusal instead of hashing an artifact past
/// the budget it was given.
pub(in crate::runner) fn verify_guest_artifact_until(
    artifact: &Path,
    target: LinuxWorkspaceTarget,
    until: Option<std::time::Instant>,
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
    let digest = linux_workspace_artifact_digest(artifact, || {
        until.is_none_or(|until| std::time::Instant::now() < until)
    })
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

/// Each target's producer receipt lives beside its own binary, so producing
/// another target cannot invalidate a still-current local artifact.
pub(in crate::runner) fn workspace_artifact_receipt_path(artifact: &Path) -> PathBuf {
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

pub(in crate::runner) fn operator_workspace_for_policy(
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

pub(in crate::runner) fn workspace_path_map(
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

pub(in crate::runner) fn host_user() -> String {
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
pub(in crate::runner) struct WorkspaceSourceClosure {
    pub(in crate::runner) pins: Vec<WorkspacePinnedSource>,
    pub(in crate::runner) snapshots: Vec<(String, Vec<u8>)>,
}

/// Why one deadline-bounded publication read produced no value.
#[derive(Debug)]
pub(in crate::runner) enum BoundedReadRefusal {
    /// The publication's one absolute deadline passed before the read finished.
    TimedOut,
    /// The read finished but could not produce a value.
    Unavailable(String),
}

impl std::fmt::Display for BoundedReadRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut => {
                formatter.write_str("the publication deadline expired during the read")
            }
            Self::Unavailable(reason) => formatter.write_str(reason),
        }
    }
}

/// Run one read-only publication step on its own worker and stop waiting at the
/// absolute deadline.
///
/// The steps a publication needs — resolving the operator workspace and loading
/// its manifest, closing the checkout's source closure, finding and
/// authenticating the produced artifact and reading its sidecars — are
/// synchronous file work that cannot be interrupted once started. A clock check
/// before or after such a call cannot enforce a budget while it is blocked, so
/// each step runs here and the caller stops waiting at the deadline, reporting
/// the fact unknown (the publication's pending/unknown path). An abandoned worker
/// only reads: it cannot publish, mutate or hold an authority.
pub(in crate::runner) fn bounded_publication_read<T, F>(
    name: &'static str,
    until: std::time::Instant,
    read: F,
) -> Result<T, BoundedReadRefusal>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, RunnerError> + Send + 'static,
{
    use std::sync::mpsc::{channel, RecvTimeoutError};

    let (send, receive) = channel();
    let worker = std::thread::Builder::new()
        .name(format!("effigy-publication-{name}"))
        .spawn(move || {
            let _ = send.send(read());
        })
        .map_err(|error| {
            BoundedReadRefusal::Unavailable(format!("the {name} worker could not start: {error}"))
        })?;
    let remaining = until.saturating_duration_since(std::time::Instant::now());
    let outcome = if remaining.is_zero() {
        Err(RecvTimeoutError::Timeout)
    } else {
        receive.recv_timeout(remaining)
    };
    match outcome {
        Ok(Ok(value)) => {
            let _ = worker.join();
            Ok(value)
        }
        Ok(Err(error)) => {
            let _ = worker.join();
            Err(BoundedReadRefusal::Unavailable(error.to_string()))
        }
        Err(RecvTimeoutError::Timeout) => Err(BoundedReadRefusal::TimedOut),
        Err(RecvTimeoutError::Disconnected) => {
            let _ = worker.join();
            Err(BoundedReadRefusal::Unavailable(format!(
                "the {name} worker ended without a result"
            )))
        }
    }
}

/// The operator workspace this checkout declares, resolved and read within one
/// absolute deadline.
///
/// Resolving it loads and composes the checkout's manifest, so it is the first
/// blocking read a publication performs; [`bounded_publication_read`] keeps it
/// inside the publication's budget.
pub(in crate::runner) fn operator_workspace_until(
    repo_root: PathBuf,
    policy: EffectiveContainerPolicy,
    until: std::time::Instant,
) -> Result<Option<OperatorWorkspace>, BoundedReadRefusal> {
    bounded_publication_read("operator-workspace", until, move || {
        operator_workspace_for_policy(repo_root.as_path(), &policy)
    })
}

/// The produced artifact this host already holds, found within one absolute
/// deadline. Finding it reads the artifact's version, identity and receipt
/// sidecars, so it is bounded like any other publication read. It never builds
/// or downloads.
pub(in crate::runner) fn held_linux_workspace_effigy_artifact_until(
    repo_root: PathBuf,
    target: LinuxWorkspaceTarget,
    until: std::time::Instant,
) -> Result<PathBuf, BoundedReadRefusal> {
    bounded_publication_read("workspace-artifact", until, move || {
        held_linux_workspace_effigy_artifact(repo_root.as_path(), target)
    })
}

/// The produced artifact's identity, authenticated against its producer receipt
/// within one absolute deadline: the version sidecar, the receipt and the
/// artifact digest are one bounded read.
pub(in crate::runner) fn verify_guest_artifact_bounded(
    artifact: PathBuf,
    target: LinuxWorkspaceTarget,
    until: std::time::Instant,
) -> Result<GuestBinaryIdentity, BoundedReadRefusal> {
    bounded_publication_read("workspace-artifact-receipt", until, move || {
        verify_guest_artifact_until(artifact.as_path(), target, Some(until))
    })
}

pub(in crate::runner) fn workspace_source_closure(
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

pub(in crate::runner) fn workspace_routes(
    policy: &EffectiveContainerPolicy,
) -> Vec<WorkspaceRoute> {
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

// Colima/nerdctl collapses nonzero guest exits into host exit 1. Optional
// reads therefore carry explicit presence framing in a successful guest reply;
// transport failure never supplies absence. The body remains binary-safe.
fn selector_parent(path: &str) -> Result<String, RunnerError> {
    Path::new(path)
        .parent()
        .and_then(Path::to_str)
        .map(shell_quote)
        .ok_or_else(|| RunnerError::task_invocation("guest selector has no parent directory"))
}

fn optional_guest_read(bytes: &[u8]) -> Result<Option<&[u8]>, RunnerError> {
    match bytes {
        b"A\n" => Ok(None),
        [b'P', b'\n', body @ ..] => Ok(Some(body)),
        _ => Err(RunnerError::task_invocation(
            "guest optional read returned malformed presence framing",
        )),
    }
}

#[cfg(test)]
mod guest_read_protocol {
    use super::*;
    #[test]
    fn selector_writes_prepare_missing_parents_through_the_bounded_guest_transport() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::{Duration, Instant};
        let _lock = crate::contract_test_support::lock_test();
        let fixture = effigy_test_support::Fixture::new("guest-selector-parent-").unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let home = root.join("home");
        std::fs::create_dir(&home).unwrap();
        let executable = bin.join("colima");
        std::fs::write(
            &executable,
            r#"#!/bin/sh
set -e
[ "$1" = nerdctl ] && [ "$2" = --profile ] && [ "$3" = fixture ] && [ "$4" = -- ]
shift 4
[ "$1" = exec ]
previous=''
while [ "$#" -gt 0 ] && [ "$1" != sh ]; do previous="$1"; shift; done
[ "$previous" = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ]
exec "$@"
"#,
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let _env = crate::contract_test_support::EnvGuard::set_many(&[
            ("PATH", Some(format!("{}:/usr/bin:/bin", bin.display()))),
            ("HOME", Some(home.display().to_string())),
            ("COLIMA_HOME", Some(root.join("c").display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("colima".into())),
        ]);
        std::fs::write(root.join("effigy.toml"), "[containers.web]\ncompose_file='compose.yml'\nprimary_service='app'\nprofile='fixture'\n").unwrap();
        std::fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: fixture:one\n",
        )
        .unwrap();
        let policy = effigy_containers::load_container_policy(&root, Some("web")).unwrap();
        let now = Instant::now();
        let mut budget = BoundedCommandBudget::new(now, now + Duration::from_secs(10), 4096, 4096);
        let id = "a".repeat(64);
        let mut guest = ComposeWorkspaceGuest::bounded(&root, &policy, &id, &mut budget);
        let pointer = root.join("guest fresh ; literal/run/bin/current");
        guest
            .replace_pointer(pointer.to_str().unwrap(), "42")
            .unwrap();
        assert_eq!(std::fs::read_to_string(&pointer).unwrap(), "42\n");
        // This double executes host utilities. The Linux guest's GNU mv -T
        // branch is exercised here on Linux and by native smoke on macOS.
        #[cfg(target_os = "linux")]
        {
            let link = root.join("another fresh/usr/local/bin/effigy");
            guest
                .replace_link(link.to_str().unwrap(), "/owned/object")
                .unwrap();
            assert_eq!(
                std::fs::read_link(link).unwrap(),
                Path::new("/owned/object")
            );
        }
    }

    #[test]
    fn presence_framing_distinguishes_absence_empty_and_binary_contents() {
        assert_eq!(optional_guest_read(b"A\n").unwrap(), None);
        assert_eq!(optional_guest_read(b"P\n").unwrap(), Some(&b""[..]));
        assert_eq!(
            optional_guest_read(b"P\nA\n\0\xff").unwrap(),
            Some(&b"A\n\0\xff"[..])
        );
        for bad in [&b""[..], &b"A\nextra"[..], &b"diagnostic"[..], &b"P"[..]] {
            assert!(optional_guest_read(bad).is_err());
        }
    }
}

impl GuestReader for ComposeWorkspaceGuest<'_> {
    fn read_file(&mut self, path: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        let script = format!(
            "p={}\nif [ -e \"$p\" ]; then printf 'P\\n'; cat \"$p\"; else printf 'A\\n'; fi",
            shell_quote(path)
        );
        let output = self.run_script(&script, false)?;
        if output.status.success() {
            return optional_guest_read(&output.stdout).map(|value| value.map(<[u8]>::to_vec));
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
            "p={path}\n[ -f \"$p\" ] || {{ printf 'A\\n'; exit 0; }}\nprintf 'P\\n'\n\
             if command -v sha256sum >/dev/null 2>&1; then sha256sum \"$p\"\n\
             elif command -v shasum >/dev/null 2>&1; then shasum -a 256 \"$p\"\n\
             elif command -v openssl >/dev/null 2>&1; then openssl dgst -sha256 \"$p\"\n\
             else exit 4\nfi",
            path = shell_quote(path)
        );
        let output = self.run_script(&script, false)?;
        match output.status.code() {
            Some(0) => {}
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
        let Some(bytes) = optional_guest_read(&output.stdout)? else {
            return Ok(None);
        };
        let rendered = String::from_utf8_lossy(bytes);
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
            "if [ -L {path} ]; then printf 'P\\n'; readlink {path}; else printf 'A\\n'; fi",
            path = shell_quote(path)
        );
        let output = self.run_script(&script, false)?;
        match output.status.code() {
            Some(0) => optional_guest_read(&output.stdout)?
                .map(|bytes| String::from_utf8(bytes.to_vec())
                    .map(|target| target.trim_end_matches('\n').to_owned())
                    .map_err(|_| RunnerError::task_invocation("guest link read returned invalid UTF-8")))
                .transpose(),
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

    fn write_no_replace(&mut self, path: &str, bytes: &[u8], mode: u32) -> Result<(), RunnerError> {
        let staging = render_workspace_effigy_staging_path();
        let host_file = stage_host_bytes(bytes)?;
        self.stage(host_file.path(), &staging)?;
        self.install_immutable(&staging, path, mode)
    }

    fn install_no_replace(&mut self, staging: &str, path: &str) -> Result<(), RunnerError> {
        self.install_immutable(staging, path, 0o555)
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
        let parent = selector_parent(path)?;
        let script = format!(
            "mkdir -p -m 0755 -- {parent} && printf '%s\\n' {target} > {path}.tmp && mv -f {path}.tmp {path}",
            target = shell_quote(target),
            path = shell_quote(path),
        );
        self.run_root(&script, "switch a guest pointer")
    }

    fn replace_link(&mut self, path: &str, target: &str) -> Result<(), RunnerError> {
        let parent = selector_parent(path)?;
        let script = format!(
            "mkdir -p -m 0755 -- {parent} && ln -sfn {target} {path}.tmp && mv -Tf {path}.tmp {path}",
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
    fn install_immutable(
        &mut self,
        staging: &str,
        path: &str,
        mode: u32,
    ) -> Result<(), RunnerError> {
        // The exclusive local staging file shares the destination filesystem.
        // link(2) is no-replace even when an external creator races the gate.
        let script = format!(
            "object=$(mktemp {template}) || exit 1; trap 'rm -f \"$object\" {staging}' EXIT; install -m {mode:04o} {staging} \"$object\" && ln \"$object\" {path}",
            template = shell_quote(&format!("{path}.stage.XXXXXX")),
            staging = shell_quote(staging), path = shell_quote(path),
        );
        self.run_root(&script, "create an immutable guest object")
    }

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

/// Read guest-local identity through the selected backend and exact full ID.
/// This does not re-resolve a Compose service or select a backend from a path.
pub(in crate::runner) fn observed_identity_on_backend(
    root: &effigy_kernel::root::StateRoot,
    selected: &effigy_containers::environment::BackendSelection,
    target_id: &str,
    deadline: std::time::Instant,
) -> Result<WorkspaceContainerIdentity, RunnerError> {
    if target_id.len() != 64
        || !target_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(RunnerError::task_invocation(
            "guest identity needs an exact full container id",
        ));
    }
    let checkout = root.owning_directory();
    let expected = checkout
        .identity()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let check = || -> Result<(), RunnerError> {
        let current = effigy_kernel::fs::DirFd::open(checkout.path())
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        if current
            .identity()
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
            != expected
        {
            return Err(RunnerError::task_invocation(
                "workspace checkout identity changed",
            ));
        }
        Ok(())
    };
    check()?;
    let script = "printf 'boot=%s\\n' \"$(cat /proc/sys/kernel/random/boot_id)\"\n\
                  printf 'platform=%s\\n' \"$(uname -m)\"\n\
                  printf 'libc_musl=%s\\n' \"$(ls /lib/ld-musl-*.so.1 2>/dev/null | head -n1)\"\n\
                  printf 'libc_glibc=%s\\n' \"$(ls /lib/*-linux-gnu*/libc.so.6 /lib/libc.so.6 /lib64/libc.so.6 2>/dev/null | head -n1)\"";
    let args = ["exec", target_id, "sh", "-lc", script].map(OsString::from);
    let (program, args) = selected
        .compose_invocation(&args)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let output = effigy_containers::exec::run_backend_invocation_until(
        checkout.path(),
        &program,
        &args,
        "workspace guest identity",
        &[],
        deadline,
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if output.stdout.len() as u64 > WORKSPACE_IDENTITY_STDOUT_BYTES
        || output.stderr.len() as u64 > WORKSPACE_IDENTITY_STDERR_BYTES
    {
        return Err(RunnerError::task_invocation(
            "workspace guest identity output exceeds its capture budget",
        ));
    }
    check()?;
    if std::time::Instant::now() >= deadline {
        return Err(RunnerError::task_invocation(
            "workspace guest identity deadline expired",
        ));
    }
    let text = std::str::from_utf8(&output.stdout)
        .map_err(|_| RunnerError::task_invocation("workspace guest identity is not UTF-8"))?;
    parse_container_identity(text, target_id)
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

/// Where a Prepare gets its Linux guest artifact (§8.9).
///
/// A checkout-built development host builds its artifact in that checkout, once
/// per stale identity. A published release host downloads the matching GitHub
/// asset. Nothing else selects a source build: a checkout that merely sits nearby
/// is not evidence of development.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ArtifactPlan {
    Build { checkout: PathBuf },
    Download,
}

/// The host facts that decide the [`ArtifactPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum HostProvenance {
    /// Installed by `install:local` or built in a checkout's `target/` tree.
    Checkout(PathBuf),
    /// Stamped as a `+local.` build whose checkout cannot be proven.
    UnprovenLocal,
    /// A published release or a plain install.
    Release,
}

/// The host's guest artifact for one target, ready for publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct HostWorkspaceArtifact {
    pub(in crate::runner) target: LinuxWorkspaceTarget,
    pub(in crate::runner) artifact: PathBuf,
    /// The install identity the bytes are current for: the checkout identity for
    /// a development build, the release identity for a download.
    pub(in crate::runner) install_identity: String,
}

/// The produced artifact this host already holds, found without building or
/// downloading. The reserved publication's deadline cannot bound either, so a
/// host whose artifact is missing or stale refuses here and names the host
/// preparation that produces it.
pub(in crate::runner) fn held_linux_workspace_effigy_artifact(
    repo_root: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<PathBuf, RunnerError> {
    let host_binary = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let cwd = current_working_dir()?;
    let source = configured_linux_workspace_artifact_source()?;
    let (artifact, identity) = match artifact_plan(&host_binary, repo_root, cwd.as_path(), source)?
    {
        ArtifactPlan::Build { checkout } => (
            checkout.join(target.artifact_relative_path()),
            local_artifact_identity(&host_binary, &checkout)?,
        ),
        ArtifactPlan::Download => (
            linux_workspace_effigy_cache_path(target)?,
            workspace_effigy_release_install_identity(),
        ),
    };
    if linux_workspace_effigy_artifact_is_current(&artifact, target, &identity) {
        return Ok(artifact);
    }
    Err(RunnerError::task_invocation(format!(
        "no current produced linux workspace artifact for `{}` is held on this host, and the \
         reserved publication neither builds nor downloads inside its deadline; run `{HOST_REPREPARE_COMMAND}` \
         on the host first",
        target.release_triple(),
    )))
}

/// Acquire the guest artifact a workspace Prepare will publish (§8.9).
///
/// This runs on the host before any activation run opens, so a long producer
/// build or a download never sits inside the publication's reservation. Hybrid
/// mode has no guest and acquires nothing.
pub(in crate::runner) fn prepare_workspace_artifact(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<HostWorkspaceArtifact>, RunnerError> {
    prepare_workspace_artifact_with(repo_root, policy, &ArtifactHost::current()?, || {
        let mut guest = ComposeWorkspaceGuest::unbounded(repo_root, policy)?;
        guest.probe_container_identity()
    })
}

/// The host facts an artifact acquisition reads: the running executable, which
/// selects provenance, the directory discovery starts from, and the source policy.
#[derive(Debug, Clone)]
pub(super) struct ArtifactHost {
    pub(super) binary: PathBuf,
    pub(super) cwd: PathBuf,
    pub(super) source: LinuxWorkspaceArtifactSource,
}

impl ArtifactHost {
    pub(super) fn current() -> Result<Self, RunnerError> {
        Ok(Self {
            binary: std::env::current_exe().map_err(RunnerError::Cwd)?,
            cwd: current_working_dir()?,
            source: configured_linux_workspace_artifact_source()?,
        })
    }
}

/// Host Prepare's artifact step (§8.9). The container-mode boundary comes first,
/// then the probed image picks the target family, then acquisition runs on this
/// host. A hybrid environment never probes or acquires.
pub(super) fn prepare_workspace_artifact_with(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    host: &ArtifactHost,
    probe: impl FnOnce() -> Result<WorkspaceContainerIdentity, RunnerError>,
) -> Result<Option<HostWorkspaceArtifact>, RunnerError> {
    if std::env::var_os("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF").is_some() {
        return Ok(None);
    }
    if operator_workspace_for_policy(repo_root, policy)?.is_none() {
        return Ok(None);
    }
    let image = probe()?;
    let target = LinuxWorkspaceTarget::from_image(&image.platform, &image.libc)?;
    acquire_linux_workspace_effigy_artifact_with(
        &host.binary,
        repo_root,
        &host.cwd,
        host.source,
        target,
    )
    .map(Some)
}

/// Publication consumes an acquired artifact only for the image family it was
/// acquired for, and only while its bytes still carry the identity they were
/// bound to. It never builds or downloads in its place.
pub(super) fn check_acquired_artifact(
    artifact: &HostWorkspaceArtifact,
    target: LinuxWorkspaceTarget,
) -> Result<(), RunnerError> {
    if target != artifact.target {
        return Err(RunnerError::task_invocation(format!(
            "workspace image changed after its guest artifact was prepared (`{}` now, `{}` prepared); run `{HOST_REPREPARE_COMMAND}` on the host",
            target.release_triple(),
            artifact.target.release_triple(),
        )));
    }
    if !linux_workspace_effigy_artifact_is_current(
        &artifact.artifact,
        target,
        &artifact.install_identity,
    ) {
        return Err(RunnerError::task_invocation(format!(
            "linux workspace artifact `{}` changed after it was prepared; run `{HOST_REPREPARE_COMMAND}` on the host",
            artifact.artifact.display(),
        )));
    }
    Ok(())
}

/// The acquisition seam: a development plan builds or reuses the checkout's
/// artifact, a download plan reuses or fetches the matching release asset.
pub(super) fn acquire_linux_workspace_effigy_artifact_with(
    host_binary: &Path,
    workspace_repo_root: &Path,
    cwd: &Path,
    source: LinuxWorkspaceArtifactSource,
    target: LinuxWorkspaceTarget,
) -> Result<HostWorkspaceArtifact, RunnerError> {
    match artifact_plan(host_binary, workspace_repo_root, cwd, source)? {
        ArtifactPlan::Build { checkout } => {
            ensure_local_linux_workspace_effigy_artifact(host_binary, &checkout, target)
        }
        ArtifactPlan::Download => Ok(HostWorkspaceArtifact {
            target,
            artifact: ensure_downloaded_linux_workspace_effigy_artifact(target)?,
            install_identity: workspace_effigy_release_install_identity(),
        }),
    }
}

/// Decide where the artifact comes from. `local` names a checkout explicitly
/// and builds there. `download` and `auto` follow the host's provenance, and a
/// local build never substitutes the published release for itself.
pub(super) fn artifact_plan(
    host_binary: &Path,
    workspace_repo_root: &Path,
    cwd: &Path,
    source: LinuxWorkspaceArtifactSource,
) -> Result<ArtifactPlan, RunnerError> {
    match (source, host_provenance(host_binary)) {
        (LinuxWorkspaceArtifactSource::Local, _) => {
            let checkout = resolve_local_effigy_repo_root_from_paths(
                workspace_repo_root,
                host_binary.parent(),
                Some(cwd),
            )
            .ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "{EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV}=local found no Effigy checkout to \
                     build the workspace artifact from; run from that checkout, or use `auto` or \
                     `download`"
                ))
            })?;
            Ok(ArtifactPlan::Build { checkout })
        }
        (LinuxWorkspaceArtifactSource::Auto, HostProvenance::Checkout(checkout)) => {
            Ok(ArtifactPlan::Build { checkout })
        }
        (
            LinuxWorkspaceArtifactSource::Auto | LinuxWorkspaceArtifactSource::Download,
            HostProvenance::Release,
        ) => Ok(ArtifactPlan::Download),
        (LinuxWorkspaceArtifactSource::Download, HostProvenance::Checkout(_)) => {
            Err(RunnerError::task_invocation(format!(
                "{EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV}=download would substitute the published \
                 release for this local Effigy build, which has no release artifact; use `auto` \
                 to build this checkout's artifact, or `local`"
            )))
        }
        (
            LinuxWorkspaceArtifactSource::Auto | LinuxWorkspaceArtifactSource::Download,
            HostProvenance::UnprovenLocal,
        ) => Err(RunnerError::task_invocation(
            "this local Effigy build has no provable checkout, so no workspace artifact can be \
             built for it, and the published release is not its artifact; run `effigy \
             install:local` from its checkout",
        )),
    }
}

/// The provenance of the running host, from its executable's location and its
/// install stamp. Nearby checkouts are not provenance.
fn host_provenance(host_binary: &Path) -> HostProvenance {
    if let Some(checkout) =
        checkout_of_local_install(host_binary).or_else(|| checkout_of_build_tree(host_binary))
    {
        return HostProvenance::Checkout(checkout);
    }
    let stamped = read_trimmed_identity_file(&workspace_effigy_active_version_file(host_binary));
    if stamped.is_some_and(|version| version.contains("+local.")) {
        return HostProvenance::UnprovenLocal;
    }
    HostProvenance::Release
}

/// `<checkout>/.local-install/bin/effigy`, the path `install:local` activates.
fn checkout_of_local_install(host_binary: &Path) -> Option<PathBuf> {
    let bin = host_binary.parent()?;
    let install = bin.parent()?;
    if !path_named(bin, "bin") || !path_named(install, ".local-install") {
        return None;
    }
    let checkout = install.parent()?;
    looks_like_effigy_repo_root(checkout).then(|| checkout.to_path_buf())
}

/// `<checkout>/target/{debug,release}/effigy`, a checkout's own build output.
fn checkout_of_build_tree(host_binary: &Path) -> Option<PathBuf> {
    let profile = host_binary.parent()?;
    let target = profile.parent()?;
    let checkout = target.parent()?;
    let profile_is_build = path_named(profile, "debug") || path_named(profile, "release");
    if !profile_is_build || !path_named(target, "target") {
        return None;
    }
    looks_like_effigy_repo_root(checkout).then(|| checkout.to_path_buf())
}

fn path_named(path: &Path, name: &str) -> bool {
    path.file_name().is_some_and(|file| file == name)
}

pub(in crate::runner) fn local_artifact_identity(
    host_binary: &Path,
    checkout: &Path,
) -> Result<String, RunnerError> {
    workspace_effigy_local_install_identity(&resolve_local_workspace_effigy_freshness_anchor(
        host_binary,
        checkout,
    ))
}

/// Build or reuse this checkout's artifact for the running host (§8.9).
///
/// Only an artifact bound to the current host identity is handed off. A missing
/// or stale one runs the producer once. Concurrent callers share that run through
/// a per-target lock, and a waiter finds the output current when it gets the lock.
pub(super) fn ensure_local_linux_workspace_effigy_artifact(
    host_binary: &Path,
    checkout: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<HostWorkspaceArtifact, RunnerError> {
    let artifact = checkout.join(target.artifact_relative_path());
    let identity = local_artifact_identity(host_binary, checkout)?;
    if linux_workspace_effigy_artifact_is_current(&artifact, target, &identity) {
        return bind_verified_linux_workspace_effigy_artifact(target, &artifact, &identity);
    }
    let _build = ArtifactFileLock::acquire(
        &linux_workspace_artifact_lock_path(checkout, target),
        &format!(
            "linux workspace artifact build for `{}` in `{}`",
            target.release_triple(),
            checkout.display()
        ),
    )?;
    if linux_workspace_effigy_artifact_is_current(&artifact, target, &identity) {
        return bind_verified_linux_workspace_effigy_artifact(target, &artifact, &identity);
    }
    // Bytes already built for this exact version may come back unchanged from a
    // no-op rebuild; they are this build's output. Touched bytes from another
    // version, or unchanged bytes that were not current, are refused.
    let previous_digest = linux_workspace_artifact_digest(&artifact, || true).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot inspect previous workspace artifact output: {error}"
        ))
    })?;
    let previous_was_current_version =
        read_trimmed_identity_file(&workspace_effigy_active_version_file(&artifact)).as_deref()
            == Some(effigy_core::build_info::active_version().as_str());
    super::emit_workspace_info(
        "building linux effigy artifact for workspace container access",
        false,
    );
    let started = std::time::SystemTime::now();
    if let Err(error) = run_linux_workspace_effigy_artifact_build(host_binary, checkout, target) {
        // A failed producer is named with what is stale, so the refusal says why
        // nothing was handed off and what to run.
        return Err(RunnerError::task_invocation(format!(
            "{error}; {}",
            describe_linux_workspace_artifact_refusal(&artifact, target, &identity, checkout),
        )));
    }
    let produced = linux_workspace_artifact_produced_since(&artifact, target, started)
        && linux_workspace_artifact_digest(&artifact, || true).is_ok_and(|digest| {
            digest.is_some() && (digest != previous_digest || previous_was_current_version)
        });
    if !produced {
        return Err(RunnerError::task_invocation(format!(
            "linux workspace artifact build succeeded but did not produce a current artifact\n{}",
            describe_linux_workspace_artifact_refusal(&artifact, target, &identity, checkout),
        )));
    }
    bind_verified_linux_workspace_effigy_artifact(target, &artifact, &identity)
}

/// Bind the active-version and install-identity sidecars to an artifact that is
/// current, or that a supported build or download just produced. The sidecars
/// are never written for stale bytes.
fn bind_verified_linux_workspace_effigy_artifact(
    target: LinuxWorkspaceTarget,
    artifact_path: &Path,
    install_identity: &str,
) -> Result<HostWorkspaceArtifact, RunnerError> {
    ensure_workspace_effigy_active_version_file(
        artifact_path,
        &effigy_core::build_info::active_version(),
    )?;
    ensure_workspace_effigy_install_identity_file(artifact_path, install_identity)?;
    Ok(HostWorkspaceArtifact {
        target,
        artifact: artifact_path.to_path_buf(),
        install_identity: install_identity.to_owned(),
    })
}

/// The per-target build lock beside a checkout's artifact. Each target has its
/// own lock, so one family's build never waits on another's.
pub(super) fn linux_workspace_artifact_lock_path(
    effigy_repo_root: &Path,
    target: LinuxWorkspaceTarget,
) -> PathBuf {
    effigy_repo_root.join(".effigy/locks").join(format!(
        "workspace-linux-artifact-{}.lock",
        target.release_triple()
    ))
}

/// An exclusive lock on one artifact path. Holding the open file is the lock, so
/// it releases on drop and on process exit; a waiter that gives up never holds it.
struct ArtifactFileLock {
    _held: std::fs::File,
}

impl ArtifactFileLock {
    fn acquire(path: &Path, what: &str) -> Result<Self, RunnerError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "cannot create lock directory `{}` for {what}: {error}",
                    parent.display()
                ))
            })?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "cannot open lock `{}` for {what}: {error}",
                    path.display()
                ))
            })?;
        let deadline = std::time::Instant::now() + artifact_lock_wait_timeout();
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _held: file }),
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(std::fs::TryLockError::Error(error)) => {
                    return Err(RunnerError::task_invocation(format!(
                        "cannot lock `{}` for {what}: {error}",
                        path.display()
                    )))
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(RunnerError::task_invocation(format!(
                    "timed out waiting for another {what} to finish; lock `{}` is still held",
                    path.display()
                )));
            }
            std::thread::sleep(LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_POLL_INTERVAL);
        }
    }
}

/// How long a waiter blocks on another preparation's lock before it gives up.
fn artifact_lock_wait_timeout() -> std::time::Duration {
    #[cfg(test)]
    {
        if let Some(wait) = ARTIFACT_LOCK_WAIT.with(std::cell::Cell::get) {
            return wait;
        }
    }
    LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_TIMEOUT
}

// Tests shorten the lock wait to exercise the give-up path without sleeping.
#[cfg(test)]
thread_local! {
    pub(super) static ARTIFACT_LOCK_WAIT: std::cell::Cell<Option<std::time::Duration>> =
        const { std::cell::Cell::new(None) };
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

/// A prepared artifact is current only when its bytes are bound to the expected
/// host identity by the identity sidecar and its rehearsal receipt names the
/// target. Timestamps and the presence of the file alone do not qualify.
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
    if let Some(cached) = current_release_cache(&cache_path, target, &install_identity)? {
        return Ok(cached);
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
    // One fetch per cache: a concurrent preparation waits here, then reuses the
    // bytes the first one verified and published, instead of racing its rename.
    let _fetch = ArtifactFileLock::acquire(
        &cache_path.with_extension("lock"),
        &format!(
            "linux workspace release download of `{}`",
            target.release_triple()
        ),
    )?;
    if let Some(cached) = current_release_cache(&cache_path, target, &install_identity)? {
        return Ok(cached);
    }

    let url = linux_workspace_effigy_release_url(target);
    super::emit_workspace_info(
        &format!("downloading linux effigy release artifact from `{url}`"),
        false,
    );
    let progress = super::WorkspaceOpProgress::start(false, format!("GET `{url}`"), false);
    progress.update(
        format!("downloading linux effigy `{}`", target.release_triple()),
        None,
        None,
    );
    let result = (|| {
        let digest = download_linux_workspace_effigy_release(&url, &cache_path, &progress)?;
        // The download records the receipt its consumer authenticates against, so
        // a cached artifact is never trusted on a version sidecar alone.
        progress.now_waiting_for(format!(
            "write artifact receipt and identity in `{}`",
            parent.display()
        ));
        progress.update("recording linux effigy artifact identity", None, None);
        write_workspace_artifact_receipt(&cache_path, target, &digest, "release-download")?;
        ensure_workspace_effigy_active_version_file(
            &cache_path,
            &effigy_core::build_info::active_version(),
        )?;
        ensure_workspace_effigy_install_identity_file(&cache_path, &install_identity)?;
        Ok(())
    })();
    match &result {
        Ok(_) => progress.finish_clear(),
        Err(error) => progress.fail_error(error),
    }
    result.map(|()| cache_path)
}

/// The cached release artifact when its identity and receipt are current and its
/// bytes verify. Binds the sidecars to the verified bytes; never stamps stale ones.
fn current_release_cache(
    cache_path: &Path,
    target: LinuxWorkspaceTarget,
    install_identity: &str,
) -> Result<Option<PathBuf>, RunnerError> {
    if !linux_workspace_release_cache_is_current(cache_path, install_identity)
        || verify_guest_artifact(cache_path, target).is_err()
    {
        return Ok(None);
    }
    ensure_workspace_effigy_active_version_file(
        cache_path,
        &effigy_core::build_info::active_version(),
    )?;
    ensure_workspace_effigy_install_identity_file(cache_path, install_identity)?;
    Ok(Some(cache_path.to_path_buf()))
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
    // Written beside and renamed over the receipt, so a reader never sees a
    // half-written receipt that could name the wrong digest.
    let mut staged = path.as_os_str().to_owned();
    staged.push(".new");
    let staged = PathBuf::from(staged);
    std::fs::write(&staged, body)
        .and_then(|()| std::fs::rename(&staged, &path))
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write linux workspace artifact receipt `{}`: {error}",
                path.display()
            ))
        })
}

pub(in crate::runner) fn workspace_effigy_active_version_file(binary: &Path) -> PathBuf {
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
    #[cfg(test)]
    let base = RELEASE_FIXTURE_BASE_URL
        .with(|base| base.borrow().clone())
        .unwrap_or_else(|| EFFIGY_RELEASE_REPO_BASE_URL.to_owned());
    #[cfg(not(test))]
    let base = EFFIGY_RELEASE_REPO_BASE_URL.to_owned();
    format!(
        "{base}/releases/download/v{}/effigy-{}",
        env!("CARGO_PKG_VERSION"),
        target.release_triple()
    )
}

/// Download one release artifact and authenticate it against the checksum the
/// release publishes beside it. Returns the `sha256:<hex>` the bytes matched.
pub(super) fn download_linux_workspace_effigy_release(
    url: &str,
    dest: &Path,
    progress: &super::WorkspaceOpProgress,
) -> Result<String, RunnerError> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to initialize release download client for `{url}`: {error}"
            ))
        })?;
    let bytes = release_get(&client, url).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to download linux effigy release artifact from `{url}`: {error}"
        ))
    })?;
    progress.now_waiting_for(format!("GET `{url}{RELEASE_CHECKSUM_SUFFIX}`"));
    progress.update("fetching linux effigy release checksum", None, None);
    let expected = published_release_checksum(&client, url)?;
    progress.now_waiting_for("SHA-256 artifact digest comparison");
    progress.update("verifying linux effigy release checksum", None, None);
    let digest = render_digest(sha256(&bytes));
    if !digest_matches(&digest, &expected) {
        return Err(RunnerError::task_invocation(format!(
            "linux effigy release artifact from `{url}` is {digest} but the published checksum is \
             sha256:{expected}; refusing to install bytes the release does not name"
        )));
    }

    let tmp_path = dest.with_extension("tmp");
    progress.now_waiting_for(format!("write and rename `{}`", tmp_path.display()));
    progress.update("caching verified linux effigy artifact", None, None);
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

/// One GET of a published release asset: its body, or the transport's refusal.
#[cfg(not(test))]
fn release_get(client: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>, String> {
    client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.bytes())
        .map(|bytes| bytes.to_vec())
        .map_err(|error| error.to_string())
}

// Tests serve release bodies from this table, so no download reaches the
// network. A URL the table does not name refuses like a failed transport.
#[cfg(test)]
thread_local! {
    pub(super) static RELEASE_FIXTURE_BASE_URL: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
    pub(super) static RELEASE_FIXTURES: std::cell::RefCell<BTreeMap<String, Vec<u8>>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
}

#[cfg(test)]
fn release_get(client: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>, String> {
    // The checksum sidecar is a second GET of the same download, not a fetch.
    if !url.ends_with(RELEASE_CHECKSUM_SUFFIX) {
        RELEASE_FETCHES.with(|fetches| fetches.set(fetches.get() + 1));
    }
    if url.starts_with("http://127.0.0.1:") {
        return client
            .get(url)
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.bytes())
            .map(|bytes| bytes.to_vec())
            .map_err(|error| error.to_string());
    }
    let delay = RELEASE_FETCH_DELAY.with(std::cell::Cell::get);
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
    RELEASE_FIXTURES.with(|fixtures| {
        fixtures
            .borrow()
            .get(url)
            .cloned()
            .ok_or_else(|| format!("no test release fixture serves `{url}`"))
    })
}

// Test-only transport accounting: how many asset GETs this thread issued, and how long
// each takes, so a concurrency test can observe duplicate fetches.
#[cfg(test)]
thread_local! {
    pub(super) static RELEASE_FETCHES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static RELEASE_FETCH_DELAY: std::cell::Cell<std::time::Duration> =
        const { std::cell::Cell::new(std::time::Duration::ZERO) };
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
    let body = release_get(client, &checksum_url)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "the release publishes no checksum at `{checksum_url}` ({error}); the workspace \
                 guest artifact cannot be authenticated. Rebuild it locally with \
                 `effigy workspace:linux:artifact`, or publish a checksum beside the release \
                 artifact"
            ))
        })
        .and_then(|bytes| {
            String::from_utf8(bytes).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "the published checksum at `{checksum_url}` is not text: {error}"
                ))
            })
        })?;
    let asset = url.rsplit('/').next().unwrap_or_default();
    let line = body.strip_suffix('\n').ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "the published checksum at `{checksum_url}` is not one newline-terminated asset line"
        ))
    })?;
    let (digest, named_asset) = line.split_once("  ").ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "the published checksum at `{checksum_url}` does not use the release sidecar format"
        ))
    })?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || named_asset != asset
    {
        return Err(RunnerError::task_invocation(format!(
            "the published checksum at `{checksum_url}` does not name `{asset}` with a lowercase SHA-256 digest"
        )));
    }
    Ok(digest.to_owned())
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
