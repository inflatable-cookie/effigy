use effigy_cli::{ContainerArgs, ContainerSubcommand, WorkspaceArgs};
use effigy_containers::compose::compose_args;
use effigy_containers::{load_workspace_ownership_targets, EffectiveContainerPolicy};
use effigy_core::shell::shell_quote;
use effigy_manifest::ManifestTask;
use effigy_ui::theme::{is_ci_environment, Theme};
use effigy_ui::{OutputMode, PlainRenderer, Renderer, SpinnerHandle};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::container_command::{run_container, run_container_shell_session};
use crate::runner::exec_command::copy_file_into_service;
use crate::runner::execute::{resolve_container_execution_binding, ContainerExecutionBinding};
use crate::runner::gateway_command::gateway_up_for_managed_task;
use crate::runner::manifest::load_task_manifest;

use super::RunnerError;

const CONTAINER_WORKSPACE_EFFIGY_STAGING_PATH: &str = "/tmp/effigy-host";
const CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH: &str = "/usr/local/bin/effigy";
const EFFIGY_RELEASE_REPO_BASE_URL: &str = "https://github.com/inflatable-cookie/effigy";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinuxWorkspaceTarget {
    X86_64Gnu,
    Aarch64Gnu,
}

impl LinuxWorkspaceTarget {
    fn release_triple(self) -> &'static str {
        match self {
            Self::X86_64Gnu => "x86_64-unknown-linux-gnu",
            Self::Aarch64Gnu => "aarch64-unknown-linux-gnu",
        }
    }

    fn from_machine(machine: &str) -> Option<Self> {
        match machine.trim() {
            "x86_64" | "amd64" => Some(Self::X86_64Gnu),
            "aarch64" | "arm64" => Some(Self::Aarch64Gnu),
            _ => None,
        }
    }

    fn artifact_relative_path(self) -> PathBuf {
        PathBuf::from(".effigy/linux-release/artifacts")
            .join(format!("effigy-{}", self.release_triple()))
    }

    fn cache_relative_dir(self) -> PathBuf {
        PathBuf::from(".effigy/workspace-bin/linux").join(self.release_triple())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkspaceSessionOwnership {
    OwnStartedSystem,
    LeaveSystemRunning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EffigySourceConfig {
    repo_root: String,
}

pub(super) fn run_workspace(args: WorkspaceArgs) -> Result<String, RunnerError> {
    if args.output_json {
        return Err(RunnerError::task_invocation(
            "`effigy workspace` does not support `--json` because it opens an interactive shell",
        ));
    }

    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let container_name = resolve_public_workspace_container(
        &repo_root,
        args.system.as_deref(),
        args.workspace.as_deref(),
        "workspace",
    )?;
    run_workspace_container_session(
        &repo_root,
        container_name.as_deref(),
        args.repo_override,
        None,
        WorkspaceSessionOwnership::OwnStartedSystem,
    )
}

pub(in crate::runner) fn run_workspace_seeded_session(
    repo_root: &Path,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
    initial_command: &str,
) -> Result<String, RunnerError> {
    run_workspace_container_session(
        repo_root,
        container_name,
        repo_override,
        Some(initial_command),
        WorkspaceSessionOwnership::LeaveSystemRunning,
    )
}

pub(super) fn resolve_public_workspace_container(
    repo_root: &Path,
    system: Option<&str>,
    workspace: Option<&str>,
    surface: &str,
) -> Result<Option<String>, RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(effigy_manifest::TASK_MANIFEST_FILE))?;
    let task = ManifestTask {
        system: system.map(str::to_owned),
        workspace: workspace.map(str::to_owned),
        ..Default::default()
    };
    let binding = resolve_container_execution_binding(
        None,
        manifest.systems.as_ref(),
        manifest.containers.as_ref(),
        surface,
        &task,
        &format!("`effigy {surface}`"),
    )?;

    match binding {
        ContainerExecutionBinding::Container { name, .. } => Ok(name),
        ContainerExecutionBinding::Inline { .. } => Err(RunnerError::task_invocation(format!(
            "`effigy {surface}` does not support inline workspace containers yet"
        ))),
        ContainerExecutionBinding::Host | ContainerExecutionBinding::None => {
            Err(RunnerError::task_invocation(format!(
                "`effigy {surface}` requires a workspace-backed system binding"
            )))
        }
    }
}

fn run_workspace_container_session(
    repo_root: &Path,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
    initial_command: Option<&str>,
    ownership: WorkspaceSessionOwnership,
) -> Result<String, RunnerError> {
    let container_name = container_name.map(str::to_owned);
    let policy = super::load_resolved_container_policy(repo_root, container_name.as_deref())?;
    emit_workspace_info(
        &format!(
            "preparing workspace container `{}` for handoff",
            policy.name
        ),
        false,
    );
    let system_was_running = super::is_primary_service_running(repo_root, &policy)?;

    if !system_was_running {
        run_container(ContainerArgs {
            subcommand: ContainerSubcommand::Up {
                name: container_name.clone(),
                attach: false,
                detach: true,
            },
            repo_override: repo_override.clone(),
            output_json: false,
        })?;
    }

    maybe_start_workspace_gateway(&policy)?;

    ensure_workspace_effigy_available(
        repo_root,
        &policy,
        container_name.as_deref(),
        repo_override.clone(),
    )?;
    ensure_workspace_permissions_ready(
        repo_root,
        &policy,
        container_name.as_deref(),
        repo_override.clone(),
    )?;

    if initial_command.is_some() {
        println!("{}", render_workspace_handoff_notice(&policy));
    }
    if initial_command.is_none() {
        clear_terminal_for_workspace_handoff()?;
    }

    let shell_result =
        run_container_shell_session(repo_root, container_name.as_deref(), None, initial_command);

    let cleanup_result =
        if should_shutdown_started_system(system_was_running, ownership, shell_result.is_ok()) {
            let mut progress = WorkspaceShutdownProgressReporter::new();
            run_container(ContainerArgs {
                subcommand: ContainerSubcommand::Down {
                    name: container_name,
                },
                repo_override,
                output_json: false,
            })
            .inspect(|_| progress.finish(true))
            .inspect_err(|_| progress.finish(false))
            .map(|_| ())
        } else {
            Ok(())
        };

    match (shell_result, cleanup_result) {
        (Ok(output), Ok(())) => Ok(output),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(shell_error), Err(cleanup_error)) => Err(RunnerError::task_invocation(format!(
            "{shell_error}\nworkspace cleanup also failed: {cleanup_error}"
        ))),
    }
}

fn should_shutdown_started_system(
    system_was_running: bool,
    ownership: WorkspaceSessionOwnership,
    session_succeeded: bool,
) -> bool {
    if system_was_running {
        return false;
    }
    match ownership {
        WorkspaceSessionOwnership::OwnStartedSystem => true,
        WorkspaceSessionOwnership::LeaveSystemRunning => session_succeeded,
    }
}

fn maybe_start_workspace_gateway(policy: &EffectiveContainerPolicy) -> Result<(), RunnerError> {
    if policy.dns_routes.is_empty() {
        return Ok(());
    }
    let executable =
        effigy_containers::session::resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    let command = effigy_containers::session::managed_gateway_command(&executable);
    gateway_up_for_managed_task(&command)
}

fn ensure_workspace_permissions_ready(
    _workspace_repo_root: &Path,
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

    let mut progress = WorkspaceTransientProgressReporter::new(
        repo_override.is_some(),
        "preparing workspace permissions",
        false,
    );
    run_container(ContainerArgs {
        subcommand: ContainerSubcommand::Shell {
            name: container_name.map(str::to_owned),
            service: Some(policy.primary_service.clone()),
            command: Some(render_workspace_permission_command(user, &targets)),
        },
        repo_override,
        output_json: false,
    })
    .inspect_err(|_| progress.finish(false))?;
    progress.finish(true);
    Ok(())
}

fn render_workspace_permission_command(user: &str, targets: &[String]) -> String {
    let quoted_targets = targets
        .iter()
        .map(|target| shell_quote(target))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "user={user}; if id -u \"$user\" >/dev/null 2>&1; then uid=$(id -u \"$user\"); gid=$(id -g \"$user\"); for path in {targets}; do mkdir -p \"$path\" && chown -R \"$uid:$gid\" \"$path\"; done; fi",
        user = shell_quote(user),
        targets = quoted_targets,
    )
}

fn render_workspace_handoff_notice(policy: &EffectiveContainerPolicy) -> String {
    let color_enabled = effigy_ui::theme::resolve_color_enabled(
        OutputMode::from_env(),
        std::io::stdout().is_terminal(),
    );
    format!(
        "{} switching into workspace container `{}`",
        crate::runner::tasks_view::style_text(color_enabled, Theme::default().warning, "[next]"),
        policy.name
    )
}

fn ensure_workspace_effigy_available(
    workspace_repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    let target = probe_workspace_linux_target(workspace_repo_root, policy)?;
    let artifact = ensure_linux_workspace_effigy_artifact(workspace_repo_root, target)?;
    let mut progress = WorkspaceTransientProgressReporter::new(
        repo_override.is_some(),
        "installing linux effigy into workspace container",
        false,
    );
    copy_file_into_service(
        workspace_repo_root,
        policy,
        policy.primary_service.as_str(),
        &artifact,
        CONTAINER_WORKSPACE_EFFIGY_STAGING_PATH,
    )
    .inspect_err(|_| progress.finish(false))?;
    run_container(ContainerArgs {
        subcommand: ContainerSubcommand::Shell {
            name: container_name.map(str::to_owned),
            service: Some(policy.primary_service.clone()),
            command: Some(format!(
                "install -m 0755 {src} {dest} && rm -f {src}",
                src = CONTAINER_WORKSPACE_EFFIGY_STAGING_PATH,
                dest = CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH,
            )),
        },
        repo_override,
        output_json: false,
    })
    .inspect_err(|_| progress.finish(false))?;
    progress.finish(true);
    Ok(())
}

fn probe_workspace_linux_target(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<LinuxWorkspaceTarget, RunnerError> {
    let mut args = compose_args(
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

fn ensure_linux_workspace_effigy_artifact(
    workspace_repo_root: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<PathBuf, RunnerError> {
    let host_binary = std::env::current_exe().map_err(RunnerError::Cwd)?;
    if let Some(effigy_repo_root) = configured_effigy_repo_root() {
        persist_effigy_source_repo_root(&effigy_repo_root)?;
        return ensure_local_linux_workspace_effigy_artifact(
            &host_binary,
            &effigy_repo_root,
            target,
        );
    }
    if let Some(effigy_repo_root) = sibling_effigy_repo_root(workspace_repo_root) {
        persist_effigy_source_repo_root(&effigy_repo_root)?;
        return ensure_local_linux_workspace_effigy_artifact(
            &host_binary,
            &effigy_repo_root,
            target,
        );
    }
    let current_exe = std::env::current_exe().map_err(RunnerError::Cwd)?;
    if let Some(effigy_repo_root) = discover_effigy_repo_root(current_exe.parent()) {
        persist_effigy_source_repo_root(&effigy_repo_root)?;
        return ensure_local_linux_workspace_effigy_artifact(
            &host_binary,
            &effigy_repo_root,
            target,
        );
    }

    let cwd = current_working_dir()?;
    if let Some(effigy_repo_root) = discover_effigy_repo_root(Some(cwd.as_path())) {
        persist_effigy_source_repo_root(&effigy_repo_root)?;
        return ensure_local_linux_workspace_effigy_artifact(
            &host_binary,
            &effigy_repo_root,
            target,
        );
    }

    ensure_downloaded_linux_workspace_effigy_artifact(target)
}

fn sibling_effigy_repo_root(workspace_repo_root: &Path) -> Option<PathBuf> {
    for ancestor in workspace_repo_root.ancestors().skip(1) {
        for candidate in nearby_effigy_repo_candidates(ancestor) {
            if looks_like_effigy_repo_root(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn nearby_effigy_repo_candidates(base: &Path) -> [PathBuf; 2] {
    [base.join("effigy"), base.join("projects").join("effigy")]
}

fn configured_effigy_repo_root() -> Option<PathBuf> {
    let path = effigy_source_config_path().ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let config = toml::from_str::<EffigySourceConfig>(&raw).ok()?;
    let repo_root = PathBuf::from(config.repo_root);
    looks_like_effigy_repo_root(&repo_root).then_some(repo_root)
}

fn persist_effigy_source_repo_root(repo_root: &Path) -> Result<(), RunnerError> {
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

fn effigy_source_config_path() -> Result<PathBuf, RunnerError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        RunnerError::task_invocation("HOME is not set; cannot resolve effigy source config path")
    })?;
    Ok(PathBuf::from(home).join(".effigy").join("source.toml"))
}

fn discover_effigy_repo_root(start: Option<&Path>) -> Option<PathBuf> {
    let mut current = start?;
    loop {
        if looks_like_effigy_repo_root(current) {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

fn looks_like_effigy_repo_root(path: &Path) -> bool {
    path.join("effigy.toml").is_file()
        && path.join("tasks/effigy.tasks.toml").is_file()
        && path.join("containers/effigy.containers.toml").is_file()
}

fn ensure_local_linux_workspace_effigy_artifact(
    host_binary: &Path,
    effigy_repo_root: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<PathBuf, RunnerError> {
    let artifact_path = effigy_repo_root.join(target.artifact_relative_path());
    let needs_refresh = linux_workspace_effigy_artifact_needs_refresh(host_binary, &artifact_path);

    if needs_refresh {
        emit_workspace_info(
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
    Ok(artifact_path)
}

fn ensure_downloaded_linux_workspace_effigy_artifact(
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
    emit_workspace_info(
        &format!("downloading linux effigy release artifact from `{url}`"),
        false,
    );
    download_linux_workspace_effigy_release(&url, &cache_path)?;
    Ok(cache_path)
}

fn linux_workspace_effigy_cache_path(target: LinuxWorkspaceTarget) -> Result<PathBuf, RunnerError> {
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

fn linux_workspace_effigy_release_url(target: LinuxWorkspaceTarget) -> String {
    format!(
        "{}/releases/download/v{}/effigy-{}",
        EFFIGY_RELEASE_REPO_BASE_URL,
        env!("CARGO_PKG_VERSION"),
        target.release_triple()
    )
}

fn download_linux_workspace_effigy_release(url: &str, dest: &Path) -> Result<(), RunnerError> {
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

fn linux_workspace_effigy_artifact_needs_refresh(host_binary: &Path, artifact_path: &Path) -> bool {
    if !artifact_path.is_file() {
        return true;
    }
    let Ok(host_meta) = std::fs::metadata(host_binary) else {
        return true;
    };
    let Ok(artifact_meta) = std::fs::metadata(artifact_path) else {
        return true;
    };
    match (host_meta.modified(), artifact_meta.modified()) {
        (Ok(host_modified), Ok(artifact_modified)) => artifact_modified < host_modified,
        _ => true,
    }
}

fn run_linux_workspace_effigy_rehearsal(
    host_binary: &Path,
    effigy_repo_root: &Path,
    target: LinuxWorkspaceTarget,
) -> Result<(), RunnerError> {
    let output = ProcessCommand::new(host_binary)
        .arg("release:linux:rehearse")
        .arg("--repo")
        .arg(effigy_repo_root)
        .env("EFFIGY_INTERNAL_SUPPRESS_HEADER", "1")
        .env("EFFIGY_LINUX_RELEASE_TRIPLE", target.release_triple())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} release:linux:rehearse --repo {}",
                host_binary.display(),
                effigy_repo_root.display()
            ),
            error,
        })?;

    if !output.success() {
        return Err(RunnerError::task_invocation(format!(
            "linux effigy rehearsal failed with status {output}"
        )));
    }
    Ok(())
}

fn emit_workspace_info(message: &str, suppress: bool) {
    if suppress {
        return;
    }
    eprintln!("{}", render_workspace_info(message));
}

fn render_workspace_info(message: &str) -> String {
    let color_enabled = effigy_ui::theme::resolve_color_enabled(
        OutputMode::from_env(),
        std::io::stderr().is_terminal(),
    );
    format!(
        "{} {}",
        crate::runner::tasks_view::style_text(color_enabled, Theme::default().label, "[info]"),
        message
    )
}

fn clear_terminal_for_workspace_handoff() -> Result<(), RunnerError> {
    if !std::io::stdout().is_terminal() {
        return Ok(());
    }
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(workspace_handoff_terminal_reset_sequence().as_bytes())
        .and_then(|_| stdout.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to prepare terminal for workspace handoff: {error}"
            ))
        })
}

fn workspace_handoff_terminal_reset_sequence() -> &'static str {
    "\x1b[2J\x1b[H\x1b[3J"
}

struct WorkspaceShutdownProgressReporter {
    spinner: Option<Box<dyn SpinnerHandle>>,
}

impl WorkspaceShutdownProgressReporter {
    fn new() -> Self {
        let transient = WorkspaceTransientProgressReporter::new(
            false,
            "waiting for workspace system shutdown",
            true,
        );
        Self {
            spinner: transient.spinner,
        }
    }

    fn finish(&mut self, success: bool) {
        let Some(spinner) = self.spinner.take() else {
            return;
        };
        if success {
            spinner.finish_clear();
        } else {
            spinner.finish_error("workspace system shutdown failed");
        }
    }
}

struct WorkspaceTransientProgressReporter {
    spinner: Option<Box<dyn SpinnerHandle>>,
}

impl WorkspaceTransientProgressReporter {
    fn new(suppress: bool, label: &str, leading_blank_lines: bool) -> Self {
        use std::io::Write;

        if suppress {
            return Self { spinner: None };
        }

        if leading_blank_lines {
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr);
            let _ = writeln!(stderr);
        }

        if !std::io::stderr().is_terminal() || is_ci_environment() {
            emit_workspace_info(label, false);
            return Self { spinner: None };
        }

        let mut renderer = PlainRenderer::stderr(OutputMode::from_env());
        let spinner = renderer.spinner(label).ok();
        Self { spinner }
    }

    fn finish(&mut self, success: bool) {
        let Some(spinner) = self.spinner.take() else {
            return;
        };
        if success {
            spinner.finish_clear();
        } else {
            spinner.finish_error("workspace operation failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_repo(manifest: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "effigy-workspace-system-tests-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(root.join("infra/dev")).expect("mkdir repo");
        std::fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
        std::fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n")
            .expect("write compose");
        root
    }

    #[test]
    fn workspace_handoff_notice_mentions_container() {
        let rendered = render_workspace_handoff_notice(&EffectiveContainerPolicy {
            name: "stack".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: effigy_containers::EffectiveComposeSource::Direct,
            compose_files: vec![std::path::PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo-stack".to_owned(),
            primary_service: "workspace".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
            shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
        });

        assert!(rendered.contains("[next]"));
        assert!(rendered.contains("switching into workspace container `stack`"));
    }

    #[test]
    fn workspace_info_render_mentions_info_label_and_message() {
        let rendered = render_workspace_info("building linux effigy artifact");

        assert!(rendered.contains("[info]"));
        assert!(rendered.contains("building linux effigy artifact"));
    }

    #[test]
    fn workspace_handoff_reset_sequence_clears_screen_and_scrollback() {
        assert_eq!(
            workspace_handoff_terminal_reset_sequence(),
            "\x1b[2J\x1b[H\x1b[3J"
        );
    }

    #[test]
    fn resolve_public_workspace_container_uses_implied_default_workspace() {
        let root = temp_repo(
            r#"
[catalog]
alias = "probe"

[containers.app]
context = "dev"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"

[systems.dev]
"#,
        );

        let container = resolve_public_workspace_container(&root, None, None, "workspace")
            .expect("resolve workspace container");

        assert_eq!(container.as_deref(), Some("app"));
    }

    #[test]
    fn linux_artifact_refreshes_when_missing_or_older_than_host_binary() {
        let root = std::env::temp_dir().join(format!(
            "effigy-linux-artifact-refresh-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("mkdir root");
        let host = root.join("effigy-host");
        let artifact = root.join("effigy-linux");
        std::fs::write(&host, "host").expect("write host");

        assert!(linux_workspace_effigy_artifact_needs_refresh(
            &host, &artifact
        ));

        std::fs::write(&artifact, "artifact").expect("write artifact");
        assert!(!linux_workspace_effigy_artifact_needs_refresh(
            &host, &artifact
        ));

        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&host, "host-new").expect("rewrite host");
        assert!(linux_workspace_effigy_artifact_needs_refresh(
            &host, &artifact
        ));
    }

    #[test]
    fn discover_effigy_repo_root_walks_up_to_repo_markers() {
        let root = std::env::temp_dir().join(format!(
            "effigy-workspace-root-discovery-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("tasks")).expect("mkdir tasks");
        std::fs::create_dir_all(root.join("containers")).expect("mkdir containers");
        std::fs::create_dir_all(root.join("target/debug")).expect("mkdir debug");
        std::fs::write(root.join("effigy.toml"), "").expect("write manifest");
        std::fs::write(root.join("tasks/effigy.tasks.toml"), "").expect("write tasks");
        std::fs::write(root.join("containers/effigy.containers.toml"), "")
            .expect("write containers");

        let discovered = discover_effigy_repo_root(Some(root.join("target/debug").as_path()))
            .expect("discover repo root");
        assert_eq!(discovered, root);
    }

    #[test]
    fn linux_workspace_release_url_matches_published_artifact_shape() {
        assert_eq!(
            linux_workspace_effigy_release_url(LinuxWorkspaceTarget::X86_64Gnu),
            format!(
                "https://github.com/inflatable-cookie/effigy/releases/download/v{}/effigy-x86_64-unknown-linux-gnu",
                env!("CARGO_PKG_VERSION")
            )
        );
        assert_eq!(
            linux_workspace_effigy_release_url(LinuxWorkspaceTarget::Aarch64Gnu),
            format!(
                "https://github.com/inflatable-cookie/effigy/releases/download/v{}/effigy-aarch64-unknown-linux-gnu",
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[test]
    fn workspace_linux_cache_path_is_versioned() {
        let original_home = std::env::var_os("HOME");
        let temp_home = std::env::temp_dir().join(format!(
            "effigy-workspace-cache-home-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_home).expect("mkdir temp home");
        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        let cache_path =
            linux_workspace_effigy_cache_path(LinuxWorkspaceTarget::X86_64Gnu).expect("cache path");

        if let Some(value) = original_home {
            unsafe {
                std::env::set_var("HOME", value);
            }
        } else {
            unsafe {
                std::env::remove_var("HOME");
            }
        }

        assert!(cache_path.starts_with(&temp_home));
        assert!(cache_path
            .display()
            .to_string()
            .contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
        assert!(cache_path
            .display()
            .to_string()
            .ends_with("effigy-x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn linux_workspace_target_maps_machine_names() {
        assert_eq!(
            LinuxWorkspaceTarget::from_machine("x86_64"),
            Some(LinuxWorkspaceTarget::X86_64Gnu)
        );
        assert_eq!(
            LinuxWorkspaceTarget::from_machine("amd64"),
            Some(LinuxWorkspaceTarget::X86_64Gnu)
        );
        assert_eq!(
            LinuxWorkspaceTarget::from_machine("aarch64"),
            Some(LinuxWorkspaceTarget::Aarch64Gnu)
        );
        assert_eq!(
            LinuxWorkspaceTarget::from_machine("arm64"),
            Some(LinuxWorkspaceTarget::Aarch64Gnu)
        );
        assert_eq!(LinuxWorkspaceTarget::from_machine("ppc64le"), None);
    }

    #[test]
    fn sibling_effigy_repo_root_prefers_adjacent_effigy_checkout() {
        let parent = std::env::temp_dir().join(format!(
            "effigy-sibling-root-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let workspace = parent.join("underlay-reference");
        let effigy = parent.join("effigy");
        std::fs::create_dir_all(&workspace).expect("mkdir workspace");
        std::fs::create_dir_all(effigy.join("tasks")).expect("mkdir tasks");
        std::fs::create_dir_all(effigy.join("containers")).expect("mkdir containers");
        std::fs::write(effigy.join("effigy.toml"), "").expect("write manifest");
        std::fs::write(effigy.join("tasks/effigy.tasks.toml"), "").expect("write tasks");
        std::fs::write(effigy.join("containers/effigy.containers.toml"), "")
            .expect("write containers");

        let discovered = sibling_effigy_repo_root(&workspace).expect("discover sibling effigy");
        assert_eq!(discovered, effigy);
    }

    #[test]
    fn sibling_effigy_repo_root_discovers_projects_effigy_from_ancestor() {
        let root = std::env::temp_dir().join(format!(
            "effigy-projects-sibling-root-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let workspace = root.join("legacy/sites/contactpatch");
        let effigy = root.join("projects/effigy");
        std::fs::create_dir_all(&workspace).expect("mkdir workspace");
        std::fs::create_dir_all(effigy.join("tasks")).expect("mkdir tasks");
        std::fs::create_dir_all(effigy.join("containers")).expect("mkdir containers");
        std::fs::write(effigy.join("effigy.toml"), "").expect("write manifest");
        std::fs::write(effigy.join("tasks/effigy.tasks.toml"), "").expect("write tasks");
        std::fs::write(effigy.join("containers/effigy.containers.toml"), "")
            .expect("write containers");

        let discovered = sibling_effigy_repo_root(&workspace).expect("discover sibling effigy");
        assert_eq!(discovered, effigy);
    }

    #[test]
    fn configured_effigy_repo_root_reads_host_pointer_file() {
        let original_home = std::env::var_os("HOME");
        let temp_home = std::env::temp_dir().join(format!(
            "effigy-configured-source-home-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let effigy = temp_home.join("projects/effigy");
        std::fs::create_dir_all(effigy.join("tasks")).expect("mkdir tasks");
        std::fs::create_dir_all(effigy.join("containers")).expect("mkdir containers");
        std::fs::write(effigy.join("effigy.toml"), "").expect("write manifest");
        std::fs::write(effigy.join("tasks/effigy.tasks.toml"), "").expect("write tasks");
        std::fs::write(effigy.join("containers/effigy.containers.toml"), "")
            .expect("write containers");
        std::fs::create_dir_all(temp_home.join(".effigy")).expect("mkdir .effigy");
        std::fs::write(
            temp_home.join(".effigy/source.toml"),
            format!("repo_root = \"{}\"\n", effigy.display()),
        )
        .expect("write source config");

        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        let discovered = configured_effigy_repo_root().expect("configured repo root");

        if let Some(value) = original_home {
            unsafe {
                std::env::set_var("HOME", value);
            }
        } else {
            unsafe {
                std::env::remove_var("HOME");
            }
        }

        assert_eq!(discovered, effigy);
    }

    #[test]
    fn persist_effigy_source_repo_root_writes_host_pointer_file() {
        let original_home = std::env::var_os("HOME");
        let temp_home = std::env::temp_dir().join(format!(
            "effigy-persisted-source-home-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let effigy = temp_home.join("projects/effigy");
        std::fs::create_dir_all(&effigy).expect("mkdir effigy");

        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        persist_effigy_source_repo_root(&effigy).expect("persist source config");
        let raw = std::fs::read_to_string(temp_home.join(".effigy/source.toml"))
            .expect("read source config");

        if let Some(value) = original_home {
            unsafe {
                std::env::set_var("HOME", value);
            }
        } else {
            unsafe {
                std::env::remove_var("HOME");
            }
        }

        assert!(raw.contains(&format!("repo_root = \"{}\"", effigy.display())));
    }

    #[test]
    fn plain_workspace_session_shuts_down_only_if_it_started_the_system() {
        assert!(should_shutdown_started_system(
            false,
            WorkspaceSessionOwnership::OwnStartedSystem,
            true,
        ));
        assert!(!should_shutdown_started_system(
            true,
            WorkspaceSessionOwnership::OwnStartedSystem,
            true,
        ));
    }

    #[test]
    fn seeded_workspace_session_shuts_down_after_successful_handoff() {
        assert!(should_shutdown_started_system(
            false,
            WorkspaceSessionOwnership::LeaveSystemRunning,
            true,
        ));
    }

    #[test]
    fn seeded_workspace_session_leaves_started_system_running_after_failed_handoff() {
        assert!(!should_shutdown_started_system(
            false,
            WorkspaceSessionOwnership::LeaveSystemRunning,
            false,
        ));
        assert!(!should_shutdown_started_system(
            true,
            WorkspaceSessionOwnership::LeaveSystemRunning,
            true,
        ));
    }
}
