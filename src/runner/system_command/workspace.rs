use effigy_cli::{ContainerArgs, ContainerSubcommand, WorkspaceArgs};
use effigy_containers::compose::compose_args;
use effigy_containers::{load_workspace_ownership_targets, EffectiveContainerPolicy};
use effigy_core::shell::shell_quote;
use effigy_manifest::ManifestTask;
use effigy_runtime::shell::run_container_shell_session as run_runtime_container_shell_session;
use effigy_ui::style_text;
use effigy_ui::theme::{is_ci_environment, Theme};
use effigy_ui::{OutputMode, PlainRenderer, Renderer, SpinnerHandle};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::container_command::{
    gateway_routes_registered_for_container, register_gateway_routes_for_container, run_container,
    runtime_error_from_runner,
};
use crate::runner::container_runtime_prep::{
    container_policy_uses_gateway_surface, ensure_container_runtime_prepared,
};
use crate::runner::exec_command::copy_file_into_service;
use crate::runner::execute::api::{resolve_container_execution_binding, ContainerExecutionBinding};
use crate::runner::gateway_command::gateway_up_for_managed_task;
use crate::runner::manifest::load_task_manifest;

use super::RunnerError;

const CONTAINER_WORKSPACE_EFFIGY_STAGING_PATH_PREFIX: &str = "/tmp/effigy-host";
const CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH: &str = "/usr/local/bin/effigy";
const EFFIGY_RELEASE_REPO_BASE_URL: &str = "https://github.com/inflatable-cookie/effigy";
const EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV: &str = "EFFIGY_WORKSPACE_EFFIGY_ARTIFACT_SOURCE";
static WORKSPACE_EFFIGY_STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinuxWorkspaceTarget {
    X86_64Gnu,
    Aarch64Gnu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinuxWorkspaceArtifactSource {
    Auto,
    Local,
    Download,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkspaceGatewayState {
    routes_were_ready_before_handoff: bool,
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
    run_workspace_with_repo_root(
        &resolved.resolved_root,
        args.system.as_deref(),
        args.workspace.as_deref(),
        args.repo_override,
        false,
    )
}

/// Workspace shell entrypoint that takes an already-resolved `repo_root`.
///
/// The plain [`run_workspace`] entrypoint reads the process cwd, which is
/// wrong for callers that have already resolved a target repo (e.g. the
/// bootstrap start-task pipeline, which clones into a sibling directory
/// before invoking the workspace task). Pass the resolved root in
/// directly to avoid re-resolving from a parent cwd.
pub(in crate::runner) fn run_workspace_with_repo_root(
    repo_root: &Path,
    system: Option<&str>,
    workspace: Option<&str>,
    repo_override: Option<PathBuf>,
    output_json: bool,
) -> Result<String, RunnerError> {
    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy workspace` does not support `--json` because it opens an interactive shell",
        ));
    }
    let container_name =
        resolve_public_workspace_container(repo_root, system, workspace, "workspace")?;
    run_workspace_container_session(
        repo_root,
        container_name.as_deref(),
        repo_override,
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
    let repo_override = effective_workspace_repo_override(repo_root, repo_override);
    let container_name = container_name.map(str::to_owned);
    let policy = load_workspace_session_policy(repo_root, container_name.as_deref())?;
    let system_was_running = ensure_container_runtime_prepared(
        repo_root,
        &policy,
        container_name.as_deref(),
        repo_override.clone(),
    )?;
    let gateway_state = prepare_workspace_handoff(
        repo_root,
        &policy,
        container_name.as_deref(),
        repo_override.clone(),
        initial_command,
    )?;
    let shell_result =
        run_workspace_handoff_shell(repo_root, container_name.as_deref(), initial_command);
    let cleanup_result = cleanup_workspace_session(
        system_was_running,
        gateway_state,
        ownership,
        shell_result.is_ok(),
        container_name,
        repo_override,
    );

    match (shell_result, cleanup_result) {
        (Ok(output), Ok(())) => Ok(output),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(shell_error), Err(cleanup_error)) => Err(RunnerError::task_invocation(format!(
            "{shell_error}\nworkspace cleanup also failed: {cleanup_error}"
        ))),
    }
}

fn load_workspace_session_policy(
    repo_root: &Path,
    container_name: Option<&str>,
) -> Result<EffectiveContainerPolicy, RunnerError> {
    let policy = super::load_resolved_container_policy(repo_root, container_name)?;
    emit_workspace_info(
        &format!(
            "preparing workspace container `{}` for handoff",
            policy.name
        ),
        false,
    );
    Ok(policy)
}

fn effective_workspace_repo_override(
    repo_root: &Path,
    repo_override: Option<PathBuf>,
) -> Option<PathBuf> {
    repo_override.or_else(|| Some(repo_root.to_path_buf()))
}

fn prepare_workspace_handoff(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
    initial_command: Option<&str>,
) -> Result<WorkspaceGatewayState, RunnerError> {
    let routes_were_ready_before_handoff = gateway_routes_ready_before_handoff(repo_root, policy)?;
    maybe_start_workspace_gateway(policy)?;
    if container_policy_uses_gateway_surface(policy) {
        register_gateway_routes_for_container(repo_root, policy)?;
    }
    let _ = container_name;
    ensure_workspace_effigy_available_for_policy(repo_root, policy, repo_override.clone())?;
    ensure_workspace_permissions_ready(repo_root, policy, container_name, repo_override)?;
    render_workspace_handoff_transition(policy, initial_command)?;
    Ok(WorkspaceGatewayState {
        routes_were_ready_before_handoff,
    })
}

fn render_workspace_handoff_transition(
    policy: &EffectiveContainerPolicy,
    initial_command: Option<&str>,
) -> Result<(), RunnerError> {
    if initial_command.is_some() {
        println!("{}", render_workspace_handoff_notice(policy));
        return Ok(());
    }

    clear_terminal_for_workspace_handoff()
}

fn run_workspace_handoff_shell(
    repo_root: &Path,
    container_name: Option<&str>,
    initial_command: Option<&str>,
) -> Result<String, RunnerError> {
    run_runtime_container_shell_session(
        repo_root,
        container_name,
        None,
        initial_command,
        validate_workspace_runtime_match,
        probe_workspace_shell_capability,
        run_workspace_shell_exec,
    )
    .map_err(Into::into)
}

fn cleanup_workspace_session(
    system_was_running: bool,
    gateway_state: WorkspaceGatewayState,
    ownership: WorkspaceSessionOwnership,
    session_succeeded: bool,
    container_name: Option<String>,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    if !should_shutdown_started_system(
        system_was_running,
        gateway_state,
        ownership,
        session_succeeded,
    ) {
        return Ok(());
    }

    let mut progress = WorkspaceShutdownProgressReporter::new();
    run_container(ContainerArgs {
        subcommand: ContainerSubcommand::Down {
            name: container_name,
            all: false,
        },
        repo_override,
        output_json: false,
    })
    .inspect(|_| progress.finish(true))
    .inspect_err(|_| progress.finish(false))
    .map(|_| ())
}

fn validate_workspace_runtime_match(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), effigy_runtime::EffigyRuntimeError> {
    crate::runner::container_command::support::validate_running_container_runtime_match(
        repo_root, policy,
    )
    .map_err(runtime_error_from_runner)
}

fn probe_workspace_shell_capability(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<String, effigy_runtime::EffigyRuntimeError> {
    crate::runner::exec_command::probe_container_capabilities(repo_root, policy, service)
        .map(|capabilities| capabilities.shell)
        .map_err(runtime_error_from_runner)
}

fn run_workspace_shell_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
) -> Result<std::process::Output, effigy_runtime::EffigyRuntimeError> {
    crate::runner::exec_command::run_compose_exec(repo_root, policy, args, capture, label)
        .map_err(runtime_error_from_runner)
}

fn should_shutdown_started_system(
    system_was_running: bool,
    gateway_state: WorkspaceGatewayState,
    ownership: WorkspaceSessionOwnership,
    session_succeeded: bool,
) -> bool {
    if system_was_running {
        return matches!(ownership, WorkspaceSessionOwnership::OwnStartedSystem)
            && !gateway_state.routes_were_ready_before_handoff;
    }
    match ownership {
        WorkspaceSessionOwnership::OwnStartedSystem => true,
        WorkspaceSessionOwnership::LeaveSystemRunning => session_succeeded,
    }
}

fn gateway_routes_ready_before_handoff(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    if !container_policy_uses_gateway_surface(policy) {
        return Ok(true);
    }
    gateway_routes_registered_for_container(repo_root, policy)
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

pub(crate) fn ensure_workspace_permissions_ready(
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

    let mut progress = WorkspaceTransientProgressReporter::new(
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

fn run_workspace_permission_prep(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    _container_name: Option<&str>,
    command: &str,
    repo_override: Option<&Path>,
) -> Result<String, RunnerError> {
    let service = policy.primary_service.as_str();
    let mut args = compose_args(policy, ["exec", "-T", "-u", "0", service, "sh", "-lc"]);
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

fn render_workspace_permission_command(user: &str, targets: &[String]) -> String {
    let quoted_targets = targets
        .iter()
        .map(|target| shell_quote(target))
        .collect::<Vec<_>>()
        .join(" ");
    // `chown -fR ... || true` is the recursive chown that tolerates per-entry
    // failures. The host-injected gitconfig / SSH known_hosts / agent socket
    // bind mounts under `/home/dev` are read-only by design, so a plain
    // `chown -R` would die on them with "Read-only file system" before
    // touching the rest of the workspace home. The `-f` flag suppresses the
    // per-entry error message, but GNU chown still exits non-zero when any
    // file failed — hence the trailing `|| true` to keep the prep step
    // green. The `mkdir -p` and the loop scaffolding still fail loudly,
    // which is what catches real misconfigurations like a missing target
    // path or an unwritable parent directory.
    format!(
        "user={user}; if id -u \"$user\" >/dev/null 2>&1; then uid=$(id -u \"$user\"); gid=$(id -g \"$user\"); for path in {targets}; do mkdir -p \"$path\" && {{ chown -fR \"$uid:$gid\" \"$path\" || true; }}; done; fi",
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
        style_text(color_enabled, Theme::default().warning, "[next]"),
        policy.name
    )
}

pub(in crate::runner) fn ensure_workspace_effigy_available_for_policy(
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
        &staging_path,
    )
    .inspect_err(|_| progress.finish(false))?;
    run_workspace_effigy_install(
        workspace_repo_root,
        policy,
        &staging_path,
        repo_override.as_deref(),
    )
    .inspect_err(|_| progress.finish(false))?;
    progress.finish(true);
    Ok(())
}

fn run_workspace_effigy_install(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    staging_path: &str,
    repo_override: Option<&Path>,
) -> Result<String, RunnerError> {
    let service = policy.primary_service.as_str();
    let mut args = compose_args(policy, ["exec", "-T", "-u", "0", service, "sh", "-lc"]);
    args.push(OsString::from(render_workspace_effigy_install_command(
        staging_path,
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

fn render_workspace_effigy_install_command(staging_path: &str) -> String {
    format!(
        "install -m 0755 {src} {dest} && rm -f {src}",
        src = shell_quote(staging_path),
        dest = CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH,
    )
}

fn render_workspace_effigy_staging_path() -> String {
    let counter = WORKSPACE_EFFIGY_STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{prefix}-{pid}-{counter}",
        prefix = CONTAINER_WORKSPACE_EFFIGY_STAGING_PATH_PREFIX,
        pid = std::process::id(),
        counter = counter,
    )
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

fn resolve_local_effigy_repo_root(
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

fn resolve_local_effigy_repo_root_from_paths(
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

fn configured_linux_workspace_artifact_source() -> Result<LinuxWorkspaceArtifactSource, RunnerError>
{
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
    let freshness_anchor =
        resolve_local_workspace_effigy_freshness_anchor(host_binary, effigy_repo_root);
    let needs_refresh =
        linux_workspace_effigy_artifact_needs_refresh(&freshness_anchor, &artifact_path, target);

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

fn resolve_local_workspace_effigy_freshness_anchor(
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

fn newest_existing_path(candidates: &[PathBuf]) -> Option<PathBuf> {
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

fn linux_workspace_effigy_artifact_needs_refresh(
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

fn linux_workspace_effigy_rehearsal_receipt_matches_target(
    artifact_path: &Path,
    target: LinuxWorkspaceTarget,
) -> bool {
    let Some(artifacts_dir) = artifact_path.parent() else {
        return false;
    };
    let receipt_path = artifacts_dir.join("rehearsal.txt");
    let Ok(receipt) = std::fs::read_to_string(&receipt_path) else {
        return false;
    };
    receipt
        .lines()
        .any(|line| line.trim() == format!("release_triple={}", target.release_triple()))
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
        style_text(color_enabled, Theme::default().label, "[info]"),
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
#[path = "workspace_tests.rs"]
mod tests;
