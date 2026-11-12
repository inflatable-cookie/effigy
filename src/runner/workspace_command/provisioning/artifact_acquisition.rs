//! Acquire a current guest artifact before activation, serialising local producers.

use super::artifact_plan::{
    artifact_plan, configured_linux_workspace_artifact_source, ArtifactPlan,
    LinuxWorkspaceArtifactSource, LinuxWorkspaceTarget,
};
use super::artifact_receipt::{
    describe_linux_workspace_artifact_refusal, linux_workspace_artifact_digest,
    linux_workspace_artifact_produced_since, linux_workspace_effigy_artifact_is_current,
    read_trimmed_identity_file, workspace_effigy_active_version_file,
    workspace_effigy_local_install_identity, workspace_effigy_release_install_identity,
    write_artifact_identity,
};
use super::artifact_release::{
    ensure_downloaded_linux_workspace_effigy_artifact, linux_workspace_effigy_cache_path,
};
use super::guest_io::ComposeWorkspaceGuest;
use super::sources::operator_workspace_for_policy;
use crate::runner::command_context::current_working_dir;
use crate::runner::workspace_client::context::{
    GuestWorkspace, WorkspaceContainerIdentity, HOST_REPREPARE_COMMAND,
};
use crate::runner::workspace_command::emit_workspace_info;
use crate::runner::RunnerError;
use effigy_containers::EffectiveContainerPolicy;
use std::path::{Path, PathBuf};
const LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(300);
const LINUX_WORKSPACE_ARTIFACT_LOCK_WAIT_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(500);

/// The host's guest artifact for one target, ready for publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct HostWorkspaceArtifact {
    pub(in crate::runner) target: LinuxWorkspaceTarget,
    pub(in crate::runner) artifact: PathBuf,
    /// The install identity the bytes are current for: the checkout identity for
    /// a development build, the release identity for a download.
    pub(in crate::runner) install_identity: String,
}

/// Find an already-produced current artifact. Reserved publication never builds
/// or downloads; a missing or stale artifact requires host preparation.
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

/// Acquire the declared workspace artifact before opening activation (§8.9).
/// Hybrid mode neither probes nor acquires.
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

/// Resolve the declared workspace, probe its image family, then acquire on the host.
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

/// Consume only the prepared image family and current producer identity.
/// Publication never builds or downloads a replacement.
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

/// Acquire from the selected checkout producer or authenticated release cache.
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

pub(in crate::runner) fn local_artifact_identity(
    host_binary: &Path,
    checkout: &Path,
) -> Result<String, RunnerError> {
    workspace_effigy_local_install_identity(&resolve_local_workspace_effigy_freshness_anchor(
        host_binary,
        checkout,
    ))
}

/// Build or reuse the artifact for this host identity (§8.9). A per-target lock
/// shares one producer; waiters reuse its verified output.
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
    emit_workspace_info(
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

/// Bind sidecars only to current bytes or a supported producer's verified output.
fn bind_verified_linux_workspace_effigy_artifact(
    target: LinuxWorkspaceTarget,
    artifact_path: &Path,
    install_identity: &str,
) -> Result<HostWorkspaceArtifact, RunnerError> {
    write_artifact_identity(artifact_path, install_identity)?;
    Ok(HostWorkspaceArtifact {
        target,
        artifact: artifact_path.to_path_buf(),
        install_identity: install_identity.to_owned(),
    })
}

/// Each target has its own build lock beside the checkout artifact.
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
pub(super) struct ArtifactFileLock {
    _held: std::fs::File,
}

impl ArtifactFileLock {
    pub(super) fn acquire(path: &Path, what: &str) -> Result<Self, RunnerError> {
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
