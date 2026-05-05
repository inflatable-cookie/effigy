use effigy_containers::{load_workspace_ownership_targets, EffectiveContainerPolicy};
use effigy_core::shell::shell_quote;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runner::command_context::current_working_dir;
use crate::runner::exec_command::copy_file_into_service;

use super::{workspace, RunnerError};

const CONTAINER_WORKSPACE_EFFIGY_STAGING_PATH_PREFIX: &str = "/tmp/effigy-host";
const CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH: &str = "/usr/local/bin/effigy";
const CONTAINER_WORKSPACE_EFFIGY_ACTIVE_VERSION_PATH: &str = "/usr/local/bin/effigy.active-version";
const EFFIGY_RELEASE_REPO_BASE_URL: &str = "https://github.com/inflatable-cookie/effigy";
pub(super) const EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV: &str =
    "EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE";
static WORKSPACE_EFFIGY_STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

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

pub(super) fn ensure_workspace_provisioning_ready(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    ensure_workspace_effigy_available_for_policy(
        workspace_repo_root,
        policy,
        repo_override.clone(),
    )?;
    ensure_workspace_permissions_ready(workspace_repo_root, policy, container_name, repo_override)
}

pub(super) fn ensure_workspace_permissions_ready(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    let Some(user) = policy.workspace_user.as_deref() else {
        return Ok(());
    };
    let targets = load_workspace_ownership_targets(policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if targets.is_empty() {
        return Ok(());
    }

    let mut progress = workspace::WorkspaceTransientProgressReporter::new(
        repo_override.is_some(),
        "preparing workspace permissions",
        false,
    );
    run_workspace_permission_prep(
        workspace_repo_root,
        policy,
        container_name,
        &render_workspace_permission_command(user, &targets),
        repo_override.as_deref(),
    )
    .inspect_err(|_| progress.finish(false))?;
    progress.finish(true);
    Ok(())
}

pub(super) fn run_workspace_permission_prep(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    _container_name: Option<&str>,
    command: &str,
    repo_override: Option<&Path>,
) -> Result<String, RunnerError> {
    let service = policy.primary_service.as_str();
    let mut args = effigy_containers::compose::compose_args(
        policy,
        ["exec", "-T", "-u", "0", service, "sh", "-lc"],
    );
    args.push(OsString::from(command));
    let output = crate::runner::exec_command::run_compose_exec(
        repo_root,
        policy,
        &args,
        false,
        "docker compose exec",
    )?;
    if output.status.success() {
        return Ok(String::new());
    }

    Err(RunnerError::task_invocation(match repo_override {
        Some(repo_override) => format!(
            "failed to prepare workspace permissions in service `{}` with repo root `{}`",
            service,
            repo_override.display()
        ),
        None => format!("failed to prepare workspace permissions in service `{service}`"),
    }))
}

pub(super) fn render_workspace_permission_command(user: &str, targets: &[String]) -> String {
    let quoted_targets = targets
        .iter()
        .map(|target| shell_quote(target))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "user={user}; if id -u \"$user\" >/dev/null 2>&1; then uid=$(id -u \"$user\"); gid=$(id -g \"$user\"); for path in {targets}; do mkdir -p \"$path\" && {{ chown -fR \"$uid:$gid\" \"$path\" || true; }}; done; fi",
        user = shell_quote(user),
        targets = quoted_targets,
    )
}

pub(super) fn ensure_workspace_effigy_available_for_policy(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    if std::env::var_os("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF").is_some() {
        return Ok(());
    }
    let target = probe_workspace_linux_target(workspace_repo_root, policy)?;
    let artifact = ensure_linux_workspace_effigy_artifact(workspace_repo_root, target)?;
    let staging_path = render_workspace_effigy_staging_path();
    let active_version_source = workspace_effigy_active_version_file(&artifact);
    let active_version_staging_path = format!("{staging_path}.active-version");
    let mut progress = workspace::WorkspaceTransientProgressReporter::new(
        repo_override.is_some(),
        "installing linux effigy into workspace container",
        false,
    );
    copy_file_into_service(
        workspace_repo_root,
        policy,
        policy.primary_service.as_str(),
        &artifact,
        &staging_path,
    )
    .inspect_err(|_| progress.finish(false))?;
    if active_version_source.is_file() {
        copy_file_into_service(
            workspace_repo_root,
            policy,
            policy.primary_service.as_str(),
            &active_version_source,
            &active_version_staging_path,
        )
        .inspect_err(|_| progress.finish(false))?;
    }
    run_workspace_effigy_install(
        workspace_repo_root,
        policy,
        &staging_path,
        active_version_source
            .is_file()
            .then_some(active_version_staging_path.as_str()),
        repo_override.as_deref(),
    )
    .inspect_err(|_| progress.finish(false))?;
    progress.finish(true);
    Ok(())
}

pub(super) fn run_workspace_effigy_install(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    staging_path: &str,
    active_version_staging_path: Option<&str>,
    repo_override: Option<&Path>,
) -> Result<String, RunnerError> {
    let service = policy.primary_service.as_str();
    let mut args = effigy_containers::compose::compose_args(
        policy,
        ["exec", "-T", "-u", "0", service, "sh", "-lc"],
    );
    args.push(OsString::from(render_workspace_effigy_install_command(
        staging_path,
        active_version_staging_path,
    )));
    let output = crate::runner::exec_command::run_compose_exec(
        repo_root,
        policy,
        &args,
        false,
        "docker compose exec",
    )?;
    if output.status.success() {
        return Ok(String::new());
    }

    Err(RunnerError::task_invocation(match repo_override {
        Some(repo_override) => format!(
            "failed to install effigy into workspace service `{}` with repo root `{}`",
            service,
            repo_override.display()
        ),
        None => format!("failed to install effigy into workspace service `{service}`"),
    }))
}

pub(super) fn render_workspace_effigy_install_command(
    staging_path: &str,
    active_version_staging_path: Option<&str>,
) -> String {
    let mut command = format!(
        "install -m 0755 {src} {dest}",
        src = shell_quote(staging_path),
        dest = CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH,
    );
    if let Some(active_version_staging_path) = active_version_staging_path {
        command.push_str(&format!(
            " && install -m 0644 {src} {dest}",
            src = shell_quote(active_version_staging_path),
            dest = CONTAINER_WORKSPACE_EFFIGY_ACTIVE_VERSION_PATH,
        ));
    }
    command.push_str(&format!(" && rm -f {}", shell_quote(staging_path)));
    if let Some(active_version_staging_path) = active_version_staging_path {
        command.push_str(&format!(" {}", shell_quote(active_version_staging_path)));
    }
    command
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

pub(super) fn probe_workspace_linux_target(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<LinuxWorkspaceTarget, RunnerError> {
    let mut args = effigy_containers::compose::compose_args(
        policy,
        ["exec", "-T", policy.primary_service.as_str(), "sh", "-lc"],
    );
    args.push(OsString::from("uname -m"));
    let output = crate::runner::exec_command::run_compose_exec(
        repo_root,
        policy,
        &args,
        true,
        "docker compose exec architecture probe",
    )?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "workspace architecture probe failed for container `{}` service `{}` with status {}",
            policy.name, policy.primary_service, output.status
        )));
    }
    let machine = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    LinuxWorkspaceTarget::from_machine(&machine).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "unsupported workspace container architecture `{machine}` for container `{}` service `{}`",
            policy.name, policy.primary_service
        ))
    })
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
    path.join("effigy.toml").is_file()
        && path.join("tasks/effigy.tasks.toml").is_file()
        && path.join("containers/effigy.containers.toml").is_file()
}

pub(super) fn ensure_local_linux_workspace_effigy_artifact(
    host_binary: &Path,
    effigy_repo_root: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<PathBuf, RunnerError> {
    let artifact_path = effigy_repo_root.join(target.artifact_relative_path());
    let freshness_anchor =
        resolve_local_workspace_effigy_freshness_anchor(host_binary, effigy_repo_root);
    let needs_refresh =
        linux_workspace_effigy_artifact_needs_refresh(&freshness_anchor, &artifact_path, target);

    if needs_refresh {
        workspace::emit_workspace_info(
            "building linux effigy artifact for workspace container access",
            false,
        );
        run_linux_workspace_effigy_rehearsal(host_binary, effigy_repo_root, target)?;
    }

    if !artifact_path.is_file() {
        return Err(RunnerError::task_invocation(format!(
            "expected linux effigy artifact at `{}` after preparation",
            artifact_path.display()
        )));
    }
    ensure_workspace_effigy_active_version_file(&artifact_path)?;
    Ok(artifact_path)
}

pub(super) fn resolve_local_workspace_effigy_freshness_anchor(
    host_binary: &Path,
    effigy_repo_root: &Path,
) -> PathBuf {
    let candidates = [
        effigy_repo_root.join(".local-install/bin/effigy"),
        effigy_repo_root.join("target/bootstrap-local/debug/effigy"),
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
    if cache_path.is_file() {
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
    workspace::emit_workspace_info(
        &format!("downloading linux effigy release artifact from `{url}`"),
        false,
    );
    download_linux_workspace_effigy_release(&url, &cache_path)?;
    ensure_workspace_effigy_active_version_file(&cache_path)?;
    Ok(cache_path)
}

pub(super) fn workspace_effigy_active_version_file(binary: &Path) -> PathBuf {
    binary.with_extension("active-version")
}

fn ensure_workspace_effigy_active_version_file(binary: &Path) -> Result<(), RunnerError> {
    let version = effigy_core::build_info::active_version();
    let version_file = workspace_effigy_active_version_file(binary);
    std::fs::write(&version_file, format!("{version}\n")).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write workspace effigy active version file `{}`: {error}",
            version_file.display()
        ))
    })
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

pub(super) fn linux_workspace_effigy_artifact_needs_refresh(
    host_binary: &Path,
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
) -> bool {
    if !artifact_path.is_file() {
        return true;
    }
    if !linux_workspace_effigy_rehearsal_receipt_matches_target(artifact_path, target) {
        return true;
    }

    let Ok(host_meta) = std::fs::metadata(host_binary) else {
        return false;
    };
    let Ok(artifact_meta) = std::fs::metadata(artifact_path) else {
        return true;
    };
    match (host_meta.modified(), artifact_meta.modified()) {
        (Ok(host_modified), Ok(artifact_modified)) => artifact_modified < host_modified,
        _ => false,
    }
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

pub(super) fn run_linux_workspace_effigy_rehearsal(
    host_binary: &Path,
    effigy_repo_root: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<(), RunnerError> {
    let status = std::process::Command::new(host_binary)
        .arg("release:linux:rehearse")
        .env("EFFIGY_LINUX_RELEASE_TRIPLE", target.release_triple())
        .current_dir(effigy_repo_root)
        .status()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to launch linux workspace rehearsal in `{}`: {error}",
                effigy_repo_root.display()
            ))
        })?;
    if status.success() {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "linux workspace rehearsal failed in `{}` with status {}",
        effigy_repo_root.display(),
        status
    )))
}
