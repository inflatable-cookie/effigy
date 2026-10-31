pub(in crate::runner) mod permissions;
pub(in crate::runner) mod provisioning;
mod session;
pub(in crate::runner) use provisioning::operator_workspace_for_policy;

use effigy_cli::{ContainerArgs, ContainerSubcommand, WorkspaceArgs};
use effigy_containers::EffectiveContainerPolicy;
use effigy_manifest::ManifestTask;
use effigy_runtime::shell::run_container_shell_session as run_runtime_container_shell_session;
use effigy_ui::steps::{self, ProgressSource, Step};
use effigy_ui::style_text;
use effigy_ui::theme::{is_ci_environment, Theme};
use effigy_ui::OutputMode;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use crate::runner::command_context::resolve_active_repo_root;
use crate::runner::container_command::{
    gateway_routes_registered_for_container, maybe_confirm_container_shell_exit_cleanup,
    run_container, runtime_error_from_runner,
};
use crate::runner::container_runtime_prep::container_policy_uses_gateway_surface;
use crate::runner::execute::api::{
    ensure_inline_workspace_supported, resolve_execution_binding_resolution, ExecutionBindingKind,
    InlineWorkspaceCapabilitySurface,
};
use crate::runner::interactive_session::{
    should_cleanup_interactive_session, InteractiveSessionIntent, InteractiveSessionOwnership,
};
use crate::runner::manifest::load_task_manifest;
use crate::runner::runtime_session_context::PublicWorkspaceCleanupOverride;

use super::RunnerError;

/// Make sure the resolved workspace binary is present before the handoff
/// shell opens.
pub(in crate::runner) fn ensure_workspace_effigy_available_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    provisioning::ensure_workspace_effigy_available_for_policy(repo_root, policy, repo_override)
}

/// Publish the guest context and pinned executable for this environment.
///
/// Called by the host's activation paths (`container up`, a task's environment
/// apply, the workspace handoff): container-mode workspaces get a prepared
/// guest, and hybrid mode publishes nothing.
pub(in crate::runner) fn publish_workspace_guest(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    provisioning::publish_workspace_guest(repo_root, policy).map(|_prepared| ())
}

/// Only the host environment activation owns this publication boundary.
pub(in crate::runner) fn publish_workspace_guest_in(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    run: &mut effigy_containers::environment::EnvRun<'_>,
) -> Result<(), RunnerError> {
    provisioning::publish_workspace_guest_in(repo_root, policy, run).map(|_| ())
}

pub(in crate::runner) fn ensure_workspace_effigy_available_for_policy_bounded(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    target_id: &str,
    budget: &mut crate::runner::exec_command::BoundedCommandBudget,
) -> Result<(), RunnerError> {
    provisioning::ensure_workspace_effigy_available_for_policy_bounded(
        repo_root, policy, target_id, budget,
    )
}

pub(in crate::runner) fn ensure_workspace_permissions_ready(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    provisioning::ensure_workspace_permissions_ready(
        repo_root,
        policy,
        container_name,
        repo_override,
    )
}

pub(super) fn run_workspace(args: WorkspaceArgs) -> Result<String, RunnerError> {
    if args.output_json {
        return Err(RunnerError::task_invocation(
            "`effigy workspace` does not support `--json` because it opens an interactive shell",
        ));
    }

    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
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
    run_workspace_with_repo_root_and_cleanup_override(
        repo_root,
        system,
        workspace,
        repo_override,
        output_json,
        None,
    )
}

pub(in crate::runner) fn run_workspace_with_repo_root_and_cleanup_override(
    repo_root: &Path,
    system: Option<&str>,
    workspace: Option<&str>,
    repo_override: Option<PathBuf>,
    output_json: bool,
    cleanup_override: Option<PublicWorkspaceCleanupOverride>,
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
        InteractiveSessionIntent::PublicWorkspace,
        cleanup_override,
    )
}

pub(in crate::runner) fn run_workspace_seeded_session(
    repo_root: &Path,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
    initial_command: &str,
    cleanup_override: Option<PublicWorkspaceCleanupOverride>,
) -> Result<String, RunnerError> {
    run_workspace_container_session(
        repo_root,
        container_name,
        repo_override,
        Some(initial_command),
        InteractiveSessionIntent::SeededTask,
        cleanup_override,
    )
}

pub(in crate::runner) fn resolve_public_workspace_container(
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
    let binding_resolution = resolve_execution_binding_resolution(
        None,
        manifest.systems.as_ref(),
        manifest.containers.as_ref(),
        surface,
        &task,
        &format!("`effigy {surface}`"),
    )?;
    let binding = binding_resolution.binding();
    ensure_inline_workspace_supported(
        binding,
        InlineWorkspaceCapabilitySurface::PublicWorkspaceCommand { surface },
    )?;

    match binding_resolution.kind() {
        ExecutionBindingKind::NamedContainer => Ok(binding_resolution
            .requested_container_name()
            .map(str::to_owned)),
        ExecutionBindingKind::InlineContainer => {
            unreachable!("inline workspace helper should always reject inline bindings")
        }
        ExecutionBindingKind::Host | ExecutionBindingKind::None => {
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
    session_intent: InteractiveSessionIntent,
    cleanup_override: Option<PublicWorkspaceCleanupOverride>,
) -> Result<String, RunnerError> {
    session::run_workspace_container_session(
        repo_root,
        container_name,
        repo_override,
        initial_command,
        session_intent,
        cleanup_override,
    )
}

pub(super) fn load_workspace_session_policy(
    repo_root: &Path,
    container_name: Option<&str>,
) -> Result<EffectiveContainerPolicy, RunnerError> {
    let policy = crate::runner::container_command::policy::load_resolved_container_policy(
        repo_root,
        container_name,
    )?;
    emit_workspace_info(
        &format!(
            "preparing workspace container `{}` for handoff",
            policy.name
        ),
        false,
    );
    Ok(policy)
}

pub(super) fn effective_workspace_repo_override(
    repo_root: &Path,
    repo_override: Option<PathBuf>,
) -> Option<PathBuf> {
    repo_override.or_else(|| Some(repo_root.to_path_buf()))
}

pub(super) fn finish_workspace_handoff_after_activation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
    initial_command: Option<&str>,
    routes_were_ready_before_handoff: bool,
) -> Result<bool, RunnerError> {
    prepare_workspace_handoff_using(
        WorkspaceHandoffRequest {
            repo_root,
            policy,
            container_name,
            repo_override,
            initial_command,
        },
        |_repo_root, _policy| Ok(routes_were_ready_before_handoff),
        |_policy| Ok(()),
        |_repo_root, _policy| Ok(()),
        provisioning::ensure_workspace_provisioning_ready,
        render_workspace_handoff_transition,
    )
}

struct WorkspaceHandoffRequest<'a> {
    repo_root: &'a Path,
    policy: &'a EffectiveContainerPolicy,
    container_name: Option<&'a str>,
    repo_override: Option<PathBuf>,
    initial_command: Option<&'a str>,
}

fn prepare_workspace_handoff_using(
    request: WorkspaceHandoffRequest<'_>,
    gateway_ready_before_handoff: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
    ) -> Result<bool, RunnerError>,
    start_gateway: impl FnOnce(&EffectiveContainerPolicy) -> Result<(), RunnerError>,
    register_routes: impl FnOnce(&Path, &EffectiveContainerPolicy) -> Result<(), RunnerError>,
    ensure_provisioning_ready: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        Option<&str>,
        Option<PathBuf>,
    ) -> Result<(), RunnerError>,
    render_transition: impl FnOnce(&EffectiveContainerPolicy, Option<&str>) -> Result<(), RunnerError>,
) -> Result<bool, RunnerError> {
    let WorkspaceHandoffRequest {
        repo_root,
        policy,
        container_name,
        repo_override,
        initial_command,
    } = request;
    let routes_were_ready_before_handoff = gateway_ready_before_handoff(repo_root, policy)?;
    start_gateway(policy)?;
    if container_policy_uses_gateway_surface(policy) {
        register_routes(repo_root, policy)?;
    }
    ensure_provisioning_ready(repo_root, policy, container_name, repo_override)?;
    render_transition(policy, initial_command)?;
    Ok(routes_were_ready_before_handoff)
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

pub(super) fn run_workspace_handoff_shell(
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

pub(super) fn cleanup_workspace_session(
    ownership: InteractiveSessionOwnership,
    session_succeeded: bool,
    container_label: &str,
    container_name: Option<String>,
    repo_override: Option<PathBuf>,
) -> Result<(), RunnerError> {
    if !resolve_workspace_shell_exit_cleanup(
        ownership,
        session_succeeded,
        maybe_confirm_container_shell_exit_cleanup(container_label)?,
    ) {
        return Ok(());
    }

    let progress = WorkspaceShutdownProgressReporter::new(container_label);
    let result = run_container(ContainerArgs {
        subcommand: ContainerSubcommand::Down {
            name: container_name,
            system: None,
            global: false,
        },
        repo_override,
        output_json: false,
    });
    match &result {
        Ok(_) => progress.succeed(),
        Err(error) => progress.fail(error),
    }
    result.map(|_| ())
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
    policy: &EffectiveContainerPolicy,
    plan: &effigy_containers::ContainerComposeInvocationPlan,
    capture: bool,
) -> Result<std::process::Output, effigy_runtime::EffigyRuntimeError> {
    crate::runner::exec_command::run_compose_exec_plan_with_options(policy, plan, capture, None)
        .map_err(runtime_error_from_runner)
}

#[cfg(test)]
fn should_shutdown_started_system(
    ownership: InteractiveSessionOwnership,
    session_succeeded: bool,
) -> bool {
    should_cleanup_interactive_session(ownership, session_succeeded)
}

fn resolve_workspace_shell_exit_cleanup(
    ownership: InteractiveSessionOwnership,
    session_succeeded: bool,
    prompt_decision: Option<bool>,
) -> bool {
    if session_succeeded {
        if let Some(prompt_decision) = prompt_decision {
            return prompt_decision;
        }
    }
    should_cleanup_interactive_session(ownership, session_succeeded)
}

pub(super) fn gateway_routes_ready_before_handoff(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    if !container_policy_uses_gateway_surface(policy) {
        return Ok(true);
    }
    gateway_routes_registered_for_container(repo_root, policy)
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

pub(super) fn emit_workspace_info(message: &str, suppress: bool) {
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
    progress: WorkspaceOpProgress,
    system: String,
}

impl WorkspaceShutdownProgressReporter {
    fn new(system: &str) -> Self {
        let progress = WorkspaceOpProgress::start(false, PROBE_CONTAINER_DOWN, true);
        if !std::io::stderr().is_terminal() || is_ci_environment() {
            emit_workspace_info("waiting for workspace system shutdown", false);
        }
        progress.update(format!("stopping workspace system `{system}`"), None, None);
        Self {
            progress,
            system: system.to_owned(),
        }
    }

    fn succeed(self) {
        self.progress
            .succeed(&format!("workspace system {} stopped", self.system));
    }

    /// Off a terminal there is no spinner to end; the error the command returns
    /// carries the same cause.
    fn fail(self, error: &RunnerError) {
        self.progress
            .fail(&workspace_shutdown_failure_line(&self.system, error));
    }
}

/// The one terminal line for a failed shutdown: the system, then the first
/// line of what the stop error says failed. The full error and its next action
/// still print after the spinner.
fn workspace_shutdown_failure_line(system: &str, error: &RunnerError) -> String {
    let cause = error
        .human_error()
        .what_failed()
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    format!("workspace system {system} shutdown failed: {cause}")
}

pub(super) const PROBE_CONTAINER_DOWN: &str = "container down";
pub(super) const PROBE_CONTAINER_IDENTITY: &str = "container identity probe";
pub(super) const PROBE_GUEST_PUBLICATION: &str = "workspace guest publication";
pub(super) const PROBE_PERMISSION_PREP: &str = "workspace path inspect";

/// Live progress for workspace context, provisioning and shutdown. Labels
/// name the current object or sub-step, known count, elapsed wait and last
/// event or the exact probe. JSON, piped and quiet output stay silent.
pub(super) struct WorkspaceOpProgress {
    source: Option<ProgressSource>,
    step: Option<Step>,
}

impl WorkspaceOpProgress {
    pub(super) fn start(
        suppress: bool,
        probe: impl Into<String>,
        leading_blank_lines: bool,
    ) -> Self {
        if suppress {
            return Self {
                source: None,
                step: None,
            };
        }
        if leading_blank_lines {
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(stderr);
            let _ = writeln!(stderr);
        }
        let source = ProgressSource::waiting_for(probe);
        let step = steps::live_step(source.clone());
        Self {
            source: Some(source),
            step: Some(step),
        }
    }

    pub(super) fn now_waiting_for(&self, probe: impl Into<String>) {
        if let Some(source) = &self.source {
            source.now_waiting_for(probe);
        }
    }

    pub(super) fn update(
        &self,
        sub_step: impl Into<String>,
        count: Option<(u64, u64)>,
        event: Option<String>,
    ) {
        if let Some(source) = &self.source {
            source.update(sub_step, count, event);
        }
    }

    pub(super) fn succeed(self, summary: &str) {
        if let Some(step) = self.step {
            step.succeed(summary);
        }
    }

    pub(super) fn fail(self, summary: &str) {
        if let Some(step) = self.step {
            step.fail(summary);
        }
    }

    pub(super) fn finish_clear(self) {
        if let Some(step) = self.step {
            step.finish_clear();
        }
    }

    pub(super) fn fail_error(self, error: &RunnerError) {
        let cause = error
            .human_error()
            .what_failed()
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned();
        self.fail(&cause);
    }

    #[cfg(test)]
    pub(super) fn frame_at(&self, elapsed: std::time::Duration) -> String {
        self.source
            .as_ref()
            .map(|source| source.frame_at(elapsed))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
