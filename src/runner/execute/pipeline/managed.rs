use super::super::super::container_command::support::validate_running_container_runtime_match;
use super::super::super::gateway_command::gateway_up_for_managed_task;
use super::super::super::managed_shell::{
    managed_readiness_probe_urls, render_inline_compose_command,
    render_inline_managed_exec_command, render_inline_managed_shell_command,
    InlineManagedExecRequest,
};
use super::super::super::runtime_session_context::current_runtime_session_context;
use super::super::super::workspace_command::run_workspace_with_repo_root_and_cleanup_override;
use super::super::api::{
    ensure_inline_workspace_supported, resolve_execution_binding_resolution,
    ContainerExecutionBinding, InlineWorkspaceCapabilitySurface,
};
use super::super::planning::ExecutionPreflight;
use crate::runner::error::RunnerError;
use crate::runner::execute::sequence_run::run_in_process_sequence_steps;
use crate::runner::execute::workspace_seeded::inside_container_handoff;
use crate::runner::managed_run::{
    self, Addressed, Binding, ClientStart, EnvironmentPlan, ExecGroupSpec, Front, ManagedTab,
    OwnedRun, TabSource, WorkloadSpec,
};
use crate::runner::task_engine::{
    refuse_live_legacy_holder, resolve_lock_wait_ms, RunKey, TaskRun, TaskRunRequest, TaskTarget,
};
use effigy_containers::compose::compose_args;
use effigy_containers::session::{
    managed_exec_command, managed_gateway_command, managed_shell_command,
    resolve_effigy_invocation_prefix, ManagedExecRequest,
};
use effigy_containers::EffectiveContainerPolicy;
use effigy_env::secret::SecretString;
use effigy_execution::{ExecutionBindingInput, ExecutionSelectionPlan};
use effigy_managed::command::resolve_managed_task_plan;
use effigy_managed::ManagedProcessRole;
use effigy_managed::{
    managed_execution_mode, parse_managed_invocation, render_managed_plan, wrap_command_with_env,
    ManagedExecutionMode, ManagedInvocation,
};
use effigy_manifest::ManifestTaskSecretsMode;
use effigy_manifest::TaskSelection;
use std::collections::BTreeMap;
use std::time::Duration;

/// `up` of an environment a managed run starts: bounded, even for a cold
/// engine that pulls images and boots a VM inside it.
const ENVIRONMENT_UP_BUDGET: Duration = Duration::from_secs(900);
/// Where an exec workload's remote session leader records itself, under
/// the checkout. Each marker carries its effect's nonce (`EXEC_NONCE`).
const EXEC_MARKER_DIR: &str = ".effigy/runtime/managed-exec";

pub(in crate::runner) fn run_managed_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    selection_plan: &ExecutionSelectionPlan,
    run_key: Option<&RunKey>,
) -> Result<Option<String>, RunnerError> {
    let container_handoff = inside_container_handoff();
    let managed_invocation = (selection.task.mode.as_deref() == Some("tui"))
        .then(|| parse_managed_invocation(&preflight.runtime_args_exec))
        .transpose()?;
    if let Some(output) = run_managed_control(preflight, selection, managed_invocation.as_ref())? {
        return Ok(Some(output));
    }
    // The detached supervisor re-resolves its client's invocation and owns
    // the run it admits (§5.8 step 3).
    let owner = managed_run::take_owner();
    // Retired selectors refuse before any effect, whatever the flags.
    let default_mode = if owner.is_some() {
        ManagedExecutionMode::Headless
    } else {
        managed_execution_mode()?
    };
    let execution_mode = match managed_invocation.as_ref().map(|value| &value.action) {
        Some(ManagedInvocation::Run { headless: true, .. }) => ManagedExecutionMode::Headless,
        _ => default_mode,
    };
    let detach = matches!(
        managed_invocation.as_ref().map(|value| &value.action),
        Some(ManagedInvocation::Run { detach: true, .. })
    );
    let binding_resolution = resolve_execution_binding_resolution(
        selection
            .catalog
            .manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "managed task execution",
    )?;
    let _binding_plan = binding_resolution.plan(ExecutionBindingInput::new(
        selection_plan.clone(),
        "managed task execution",
    ));
    let container_binding = binding_resolution.binding();
    let runtime_args = managed_invocation
        .as_ref()
        .map(|invocation| &invocation.runtime_args)
        .unwrap_or(&preflight.runtime_args_exec);
    let plan = resolve_managed_task_plan(
        &preflight.selector,
        selection.catalog,
        selection.task,
        runtime_args,
        &preflight.catalogs,
        &selection.catalog.catalog_root,
        &effigy_routing::resolve_task_selection,
    )?;
    let Some(plan) = plan else {
        // Not a managed run: leave the environment untouched.
        if should_open_workspace_shell_for_non_managed_task(
            selection.task.run.is_some(),
            container_binding,
            container_handoff,
        ) {
            let target = TaskTarget::for_selection(
                &preflight.resolved.resolved_root,
                &preflight.selector.task_name,
                selection,
            );
            refuse_live_legacy_holder(&target)?;
            let run = TaskRun::begin(TaskRunRequest {
                target: &target,
                lock_wait_ms: resolve_lock_wait_ms(preflight.runtime_args_raw.lock_wait_ms)?,
                custody: false,
                // A workspace shell session is not a task command.
                scratch: false,
                managed_run_token: None,
                args: &preflight.runtime_args_exec.passthrough,
                key: run_key,
                admission: selection.task.admission,
                cwd: Some(&preflight.invocation_cwd),
            })?;
            let result = run_workspace_with_repo_root_and_cleanup_override(
                &preflight.resolved.resolved_root,
                selection.task.system.as_deref(),
                selection.task.workspace.as_deref(),
                preflight.runtime_args_raw.repo_override.clone(),
                preflight.output_json,
                Some(current_runtime_session_context().public_workspace_cleanup),
            );
            close_task_run(run, &target, &result);
            return result.map(Some);
        }
        ensure_inline_workspace_supported(
            container_binding,
            InlineWorkspaceCapabilitySurface::ManagedAttachedSession {
                task_name: &preflight.selector.task_name,
            },
        )?;
        return Ok(None);
    };
    let context = ManagedContext {
        preflight,
        selection,
        binding_resolution: &binding_resolution,
        container_handoff,
    };
    if owner.is_none() && execution_mode == ManagedExecutionMode::RenderPlan {
        let plan = displayed_plan(&context, plan)?;
        return render_managed_plan(
            &preflight.selector.task_name,
            &selection.catalog.catalog_root,
            &selection.catalog.manifest_path,
            plan,
        )
        .map(Some)
        .map_err(Into::into);
    }
    if run_key.is_some() {
        return Err(RunnerError::task_invocation(format!(
            "`--run-id` names one-shot runs; managed task `{}` is addressed by its task and \
             profile (`effigy {} status|logs|stop`)",
            preflight.selector.task_name, preflight.selector.task_name
        )));
    }
    match owner {
        Some(owner) => own_managed_run(owner, &context, plan),
        None => {
            let front = match (execution_mode, detach) {
                (_, true) => Front::Detached,
                (ManagedExecutionMode::Tui, false) => Front::Tui,
                _ => Front::Text,
            };
            start_managed_run(&context, plan, front)
        }
    }
}

/// `status`, `logs` and `stop` of one managed task and profile.
fn run_managed_control(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    invocation: Option<&effigy_managed::ParsedManagedInvocation>,
) -> Result<Option<String>, RunnerError> {
    let Some(invocation) = invocation else {
        return Ok(None);
    };
    let profile = match &invocation.action {
        ManagedInvocation::Status { profile, .. }
        | ManagedInvocation::Logs { profile, .. }
        | ManagedInvocation::Stop { profile } => profile,
        ManagedInvocation::Run { .. } => return Ok(None),
    };
    if !effigy_managed::has_concurrent_profile(selection.task, profile) {
        return Err(effigy_managed::ManagedError::TaskManagedProfileNotFound {
            task: preflight.selector.task_name.clone(),
            profile: profile.to_owned(),
            available: effigy_managed::available_concurrent_profiles(selection.task),
        }
        .into());
    }
    let target = TaskTarget::for_selection(
        &preflight.resolved.resolved_root,
        &preflight.selector.task_name,
        selection,
    )
    .with_profile(profile);
    let addressed = Addressed {
        target: &target,
        task: &preflight.selector.task_name,
        profile,
    };
    let output = match &invocation.action {
        ManagedInvocation::Status { json, .. } => {
            managed_run::status(&addressed, preflight.output_json || *json)
        }
        ManagedInvocation::Logs {
            process, follow, ..
        } => managed_run::logs(&addressed, process.as_deref(), *follow),
        ManagedInvocation::Stop { .. } => managed_run::stop(&addressed),
        ManagedInvocation::Run { .. } => unreachable!("handled above"),
    }?;
    Ok(Some(output))
}

/// What both sides of a managed run resolve from the same invocation.
struct ManagedContext<'a, 'catalog> {
    preflight: &'a ExecutionPreflight,
    selection: &'a TaskSelection<'catalog>,
    binding_resolution: &'a super::super::api::ExecutionBindingResolution,
    container_handoff: bool,
}

impl ManagedContext<'_, '_> {
    fn target(&self, profile: &str) -> TaskTarget {
        let target = TaskTarget::for_selection(
            &self.preflight.resolved.resolved_root,
            &self.preflight.selector.task_name,
            self.selection,
        );
        if self.selection.task.mode.as_deref() == Some("tui") {
            target.with_profile(profile)
        } else {
            target
        }
    }

    fn repo(&self) -> &std::path::Path {
        &self.selection.catalog.catalog_root
    }

    fn secrets_required(&self) -> bool {
        matches!(
            self.selection.task.secrets,
            Some(ManifestTaskSecretsMode::Required)
        )
    }

    fn task_secrets(&self) -> Result<Vec<(String, SecretString)>, RunnerError> {
        if !self.secrets_required() {
            return Ok(Vec::new());
        }
        super::standard::resolve_task_secret_env(
            &self.preflight.resolved.resolved_root,
            &self.preflight.secret_targets,
            self.selection.task,
            true,
            self.preflight.selector.task_name == "dev",
        )
    }
}

/// The client: resolve what only the operator can supply (the vault
/// passphrase), start the run's supervisor and attach the front end.
fn start_managed_run(
    context: &ManagedContext<'_, '_>,
    plan: effigy_managed::ManagedTaskPlan,
    front: Front,
) -> Result<Option<String>, RunnerError> {
    // Reading the task's secrets here prompts once for the passphrase on
    // this terminal; the supervisor receives it in its spawn environment.
    context.task_secrets()?;
    let target = context.target(&plan.profile);
    refuse_live_legacy_holder(&target)?;
    let addressed = Addressed {
        target: &target,
        task: &context.preflight.selector.task_name,
        profile: &plan.profile,
    };
    let terminal = (front == Front::Tui)
        .then(crossterm::terminal::size)
        .and_then(Result::ok);
    let started = managed_run::start(&ClientStart {
        addressed: Addressed {
            target: &target,
            task: &context.preflight.selector.task_name,
            profile: &plan.profile,
        },
        invocation: &context.preflight.invocation,
        cwd: context.preflight.invocation_cwd.clone(),
        front,
        terminal,
    })?;
    match front {
        Front::Detached => managed_run::detach(started, &addressed),
        Front::Text => managed_run::attach_text(started, &addressed),
        Front::Tui => {
            let (tabs, shells) = tui_tabs(context, &plan)?;
            managed_run::attach_tui(
                started,
                &addressed,
                context.repo().to_path_buf(),
                tabs,
                plan.tab_order.clone(),
                shells,
            )
        }
    }
}

/// The plan as its supervisor would run it: the environment's `up`, each
/// workload's command and each shell tab's attach. A pure read (R2): no
/// secret is resolved, so none can reach the rendered plan.
fn displayed_plan(
    context: &ManagedContext<'_, '_>,
    mut plan: effigy_managed::ManagedTaskPlan,
) -> Result<effigy_managed::ManagedTaskPlan, RunnerError> {
    let environment = environment_plan(context, &plan, "<run token>", &BTreeMap::new())?;
    let container_repo_root = context
        .binding_resolution
        .exec_working_dir(context.repo())?;
    let workloads = managed_workloads(
        context,
        &plan,
        &[],
        "<run token>",
        container_repo_root.as_deref(),
    )?;
    for process in &mut plan.processes {
        match process.role {
            ManagedProcessRole::Lifecycle => {
                if let Some(environment) = &environment {
                    process.run = environment.up.clone();
                }
            }
            ManagedProcessRole::Shell => process.run = shell_command(context, process)?,
            ManagedProcessRole::Standard => {
                if let Some(workload) = workloads.iter().find(|w| w.name == process.name) {
                    process.run = workload.command.clone();
                }
            }
        }
    }
    Ok(plan)
}

/// The TUI's tabs: the lifecycle tab shows the run's own output, workloads
/// show their logs, and shell tabs are local interactive terminals.
fn tui_tabs(
    context: &ManagedContext<'_, '_>,
    plan: &effigy_managed::ManagedTaskPlan,
) -> Result<(Vec<ManagedTab>, Vec<effigy_process::ProcessSpec>), RunnerError> {
    let mut tabs = Vec::new();
    let mut shells = Vec::new();
    for process in &plan.processes {
        let source = match process.role {
            ManagedProcessRole::Lifecycle => TabSource::Output,
            ManagedProcessRole::Standard => TabSource::Workload,
            ManagedProcessRole::Shell => TabSource::Shell,
        };
        if source == TabSource::Shell {
            shells.push(effigy_process::ProcessSpec {
                name: process.name.clone(),
                run: shell_command(context, process)?,
                cwd: process.cwd.clone(),
                start_after_ms: 0,
                shutdown_on_exit: process.shutdown_on_exit,
                pty: true,
                env: BTreeMap::new(),
            });
        }
        tabs.push(ManagedTab {
            name: process.name.clone(),
            pty: process.role != ManagedProcessRole::Lifecycle,
            shutdown_on_exit: process.shutdown_on_exit,
            source,
        });
    }
    Ok((tabs, shells))
}

/// A shell tab's interactive attach, run by the TUI client itself.
fn shell_command(
    context: &ManagedContext<'_, '_>,
    process: &effigy_managed::ManagedProcessSpec,
) -> Result<String, RunnerError> {
    let repo = context.repo();
    if context.container_handoff {
        return Ok(default_handoff_managed_shell_run());
    }
    if let Some(policy) = inline_policy(context)? {
        return Ok(render_inline_managed_shell_command(
            repo,
            &policy,
            process.service.as_deref(),
        ));
    }
    let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    Ok(managed_shell_command(
        repo,
        context.binding_resolution.binding().container_name(),
        process.service.as_deref(),
        &executable,
    ))
}

fn inline_policy(
    context: &ManagedContext<'_, '_>,
) -> Result<Option<EffectiveContainerPolicy>, RunnerError> {
    if context.binding_resolution.is_inline_container() {
        context.binding_resolution.effective_policy(context.repo())
    } else {
        Ok(None)
    }
}

fn named_policy(
    context: &ManagedContext<'_, '_>,
) -> Result<Option<EffectiveContainerPolicy>, RunnerError> {
    if context.binding_resolution.is_inline_container() {
        Ok(None)
    } else {
        context.binding_resolution.effective_policy(context.repo())
    }
}

/// The supervisor: everything the run starts, resolved on the host after
/// admission: setup, gateway, environment and workloads.
fn own_managed_run(
    owner: managed_run::OwnerSession,
    context: &ManagedContext<'_, '_>,
    mut plan: effigy_managed::ManagedTaskPlan,
) -> Result<Option<String>, RunnerError> {
    let preflight = context.preflight;
    let selection = context.selection;
    let token = generate_managed_run_token();
    // Anything that fails before admission is the supervisor's refusal: the
    // client reports it, and nothing ran.
    let inputs = (|| -> Result<_, RunnerError> {
        let secrets = context.task_secrets()?;
        let env_schema_resolved = super::standard::resolve_env_schema_if_present(
            &selection.catalog.catalog_root,
            preflight.runtime_args_raw.env_schema_override.as_deref(),
            selection.catalog.manifest.env_schema.as_ref(),
            None,
        )?;
        let role_schema_env = managed_role_schema_env(
            &preflight.catalogs,
            &selection.catalog.catalog_root,
            preflight.runtime_args_raw.env_schema_override.as_deref(),
        )?;
        let container_repo_root = context
            .binding_resolution
            .exec_working_dir(context.repo())?;
        let environment = environment_plan(context, &plan, &token, &role_schema_env)?;
        let target = context.target(&plan.profile);
        refuse_live_legacy_holder(&target)?;
        let owned = OwnedRun {
            target,
            lock_wait_ms: resolve_lock_wait_ms(preflight.runtime_args_raw.lock_wait_ms)?,
            admission: selection.task.admission,
            cwd: preflight.invocation_cwd.clone(),
            args: preflight.runtime_args_exec.passthrough.clone(),
            token: token.clone(),
            task: preflight.selector.task_name.clone(),
            profile: plan.profile.clone(),
            environment,
            fail_on_non_zero: plan.fail_on_non_zero,
            scratch: managed_task_scratch(selection.task),
            secrets: secrets
                .iter()
                .map(|(_, value)| value.expose().to_owned())
                .collect(),
        };
        Ok((secrets, env_schema_resolved, container_repo_root, owned))
    })();
    let (secrets, env_schema_resolved, container_repo_root, owned) = match inputs {
        Ok(inputs) => inputs,
        Err(error) => {
            owner.session.refused(&error.to_string());
            return Err(error);
        }
    };
    let prepare = |run: &mut TaskRun| -> Result<Vec<WorkloadSpec>, RunnerError> {
        {
            let _parent = run.enter()?;
            run_managed_setup_steps(
                &mut plan,
                run,
                ManagedSetupContext {
                    preflight,
                    selection,
                    env_schema_resolved: &env_schema_resolved,
                    binding_resolution: context.binding_resolution,
                    container_binding: context.binding_resolution.binding(),
                    container_handoff: context.container_handoff,
                    managed_task_secret_pairs: &secrets,
                },
            )?;
        }
        if !context.container_handoff && plan.gateway_auto_start {
            managed_run::record_gateway(run)?;
            maybe_start_managed_gateway(
                context.repo(),
                &preflight.selector.task_name,
                selection,
                true,
            )?;
        }
        managed_workloads(
            context,
            &plan,
            &secrets,
            &token,
            container_repo_root.as_deref(),
        )
    };
    managed_run::own(owner, owned, prepare)
}

/// A managed session's host processes use the configured scratch directory
/// unless the task declares `scratch = false`.
fn managed_task_scratch(task: &effigy_manifest::ManifestTask) -> bool {
    task.uses_scratch()
}

fn managed_workload<T>(
    target: &TaskTarget,
    result: &Result<T, RunnerError>,
) -> effigy_kernel::outcome::WorkloadOutcome {
    match result {
        Ok(_) => effigy_kernel::outcome::WorkloadOutcome::Succeeded,
        Err(error) => {
            let code = error.task_exit_status().unwrap_or(1);
            if code == 124 {
                effigy_kernel::outcome::WorkloadOutcome::TimedOut {
                    phase: target.label.clone(),
                }
            } else {
                crate::runner::task_engine::workload_for_exit(code)
            }
        }
    }
}

/// Records the managed run's outcome and releases its gate. Journal
/// failures are warnings; they never change the run's result (K10).
fn close_task_run<T>(run: TaskRun, target: &TaskTarget, result: &Result<T, RunnerError>) {
    let workload = managed_workload(target, result);
    for warning in run.close(workload) {
        eprintln!("warning: {}: {warning}", target.label);
    }
}

struct ManagedSetupContext<'a, 'catalog> {
    preflight: &'a ExecutionPreflight,
    selection: &'a TaskSelection<'catalog>,
    env_schema_resolved: &'a Option<effigy_env::resolver::ResolvedEnv>,
    binding_resolution: &'a super::super::api::ExecutionBindingResolution,
    container_binding: &'a ContainerExecutionBinding,
    container_handoff: bool,
    managed_task_secret_pairs: &'a [(String, SecretString)],
}

fn run_managed_setup_steps(
    plan: &mut effigy_managed::ManagedTaskPlan,
    run: &TaskRun,
    context: ManagedSetupContext<'_, '_>,
) -> Result<(), RunnerError> {
    let ManagedSetupContext {
        preflight,
        selection,
        env_schema_resolved,
        binding_resolution,
        container_binding,
        container_handoff,
        managed_task_secret_pairs,
    } = context;
    let plan_has_lifecycle = plan
        .processes
        .iter()
        .any(|process| process.role == ManagedProcessRole::Lifecycle);
    let local_secret_refs = managed_task_secret_pairs
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect::<Vec<_>>();
    let local_secret_ref = (!local_secret_refs.is_empty()).then_some(local_secret_refs.as_slice());
    for process in &mut plan.processes {
        if process.setup_steps.is_empty() {
            continue;
        }
        if managed_setup_steps_can_run_locally(process, container_binding, container_handoff) {
            let mut local_task = selection.task.clone();
            local_task.run_in = Some(effigy_manifest::ManifestTaskRunIn::Host);
            let local_selection = TaskSelection {
                task: &local_task,
                catalog: selection.catalog,
                mode: selection.mode,
                evidence: selection.evidence.clone(),
                surface: selection.surface,
            };
            run_in_process_sequence_steps(
                preflight,
                &local_selection,
                &process.setup_steps,
                env_schema_resolved,
                local_secret_ref,
                (&process.name, &[]),
                run,
            )?;
            process.setup = None;
            continue;
        }
        if matches!(
            binding_resolution.kind(),
            super::super::api::ExecutionBindingKind::NamedContainer
        ) && !plan_has_lifecycle
        {
            // The typed engine, as for host setup: each step is a child run
            // and container workloads run under its remote custody.
            run_in_process_sequence_steps(
                preflight,
                selection,
                &process.setup_steps,
                env_schema_resolved,
                local_secret_ref,
                (&process.name, &[]),
                run,
            )?;
            process.setup = None;
        }
    }
    Ok(())
}

fn managed_setup_steps_can_run_locally(
    process: &effigy_managed::ManagedProcessSpec,
    container_binding: &ContainerExecutionBinding,
    container_handoff: bool,
) -> bool {
    // Container-routed managed setup cannot move to the host fast path without
    // changing Rhai script semantics for `fs::*`, `process::*`, and relative
    // cwd handling. Those scripts still need a true local route.
    if process.run_on_host || container_handoff {
        return true;
    }

    matches!(
        container_binding,
        ContainerExecutionBinding::None | ContainerExecutionBinding::Host
    )
}

fn should_open_workspace_shell_for_non_managed_task(
    has_run_command: bool,
    container_binding: &ContainerExecutionBinding,
    container_handoff: bool,
) -> bool {
    if container_handoff || has_run_command {
        return false;
    }

    matches!(
        container_binding,
        ContainerExecutionBinding::Container { .. }
    )
}

fn build_managed_gateway_command(
    repo_root: &std::path::Path,
    task_name: &str,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
    let binding_resolution = resolve_execution_binding_resolution(
        selection
            .catalog
            .manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        task_name,
        selection.task,
        "managed gateway startup",
    )?;
    let requested_container_name = match binding_resolution.kind() {
        super::super::api::ExecutionBindingKind::NamedContainer => {
            binding_resolution.requested_container_name()
        }
        super::super::api::ExecutionBindingKind::InlineContainer => {
            return Err(RunnerError::task_invocation(
                "`gateway = true` requires a workspace-backed container binding on the task",
            ));
        }
        super::super::api::ExecutionBindingKind::Host
        | super::super::api::ExecutionBindingKind::None => {
            return Err(RunnerError::task_invocation(
                "`gateway = true` requires a workspace-backed container binding on the task",
            ));
        }
    };
    let policy = effigy_containers::load_container_policy(repo_root, requested_container_name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if policy.dns_routes.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "task `{}` sets `gateway = true`, but container session `{}` does not declare any `[containers.{}.dns].routes`",
            task_name,
            requested_container_name.unwrap_or("default"),
            policy.name
        )));
    }
    let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    Ok(managed_gateway_command(&executable))
}

fn maybe_start_managed_gateway(
    repo_root: &std::path::Path,
    task_name: &str,
    selection: &TaskSelection<'_>,
    should_start: bool,
) -> Result<(), RunnerError> {
    if !should_start {
        return Ok(());
    }
    let gateway_command = build_managed_gateway_command(repo_root, task_name, selection)?;
    gateway_up_for_managed_task(&gateway_command)
}

fn normalize_managed_lifecycle_container_ref(container_name: &str) -> Option<&str> {
    match container_name.trim() {
        "" | "default" => None,
        other => Some(other),
    }
}

/// The environment a profile with a lifecycle entry brings up, or borrows.
/// Inside a container handoff the workspace container is already the
/// environment: there is nothing to bring up.
fn environment_plan(
    context: &ManagedContext<'_, '_>,
    plan: &effigy_managed::ManagedTaskPlan,
    token: &str,
    schema_env: &BTreeMap<String, String>,
) -> Result<Option<EnvironmentPlan>, RunnerError> {
    let owns_environment = plan
        .processes
        .iter()
        .any(|process| process.role == ManagedProcessRole::Lifecycle);
    if !owns_environment || context.container_handoff {
        return Ok(None);
    }
    let repo = context.repo().to_path_buf();
    let inline = inline_policy(context)?;
    let named = named_policy(context)?;
    let Some(policy) = inline.as_ref().or(named.as_ref()) else {
        return Ok(None);
    };
    if inline.is_none() {
        validate_running_container_runtime_match(&repo, policy)?;
    }
    let (binding, up) = match &inline {
        Some(policy) => (
            Binding::Inline {
                task: context.preflight.invocation.name.clone(),
            },
            render_inline_compose_command(&repo, policy, &compose_args(policy, ["up", "-d"])),
        ),
        None => {
            let selector = context
                .binding_resolution
                .binding()
                .container_name()
                .and_then(normalize_managed_lifecycle_container_ref)
                .map(str::to_owned);
            let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
            (
                Binding::Named {
                    selector: selector.clone(),
                },
                managed_environment_up_command(&repo, selector.as_deref(), &executable),
            )
        }
    };
    let mut up_env = schema_env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    up_env.push((effigy_managed::RUN_TOKEN_ENV.to_owned(), token.to_owned()));
    if context.secrets_required() {
        up_env.push(("EFFIGY_SECRETS_REQUIRED".to_owned(), "1".to_owned()));
    }
    let exec_workloads = plan.processes.iter().any(|process| {
        process.role == ManagedProcessRole::Standard
            && !process.run_on_host
            && !matches!(
                context.binding_resolution.binding(),
                ContainerExecutionBinding::None | ContainerExecutionBinding::Host
            )
    });
    let working_dir = context
        .binding_resolution
        .exec_working_dir(&repo)?
        .unwrap_or_else(|| repo.clone());
    let mut ready_lines = Vec::new();
    if let Some(message) = managed_ready_message(
        context.selection.task.ready_message.as_deref(),
        Some(policy),
    ) {
        ready_lines.push(format!("ready: {message}"));
    }
    for line in managed_dns_route_lines(Some(policy)) {
        ready_lines.push(format!("route: {line}"));
    }
    Ok(Some(EnvironmentPlan {
        name: policy.name.clone(),
        repo: repo.clone(),
        binding,
        up,
        up_env,
        borrowed: inline.is_none() && environment_borrowed(&repo, policy),
        exec_probe: exec_workloads.then(|| (policy.primary_service.clone(), working_dir)),
        route_urls: if plan.readiness.health_wait {
            managed_readiness_probe_urls(Some(policy))
        } else {
            Vec::new()
        },
        up_budget: ENVIRONMENT_UP_BUDGET,
        readiness_budget: Duration::from_secs(plan.readiness.timeout_secs),
        ready_lines,
    }))
}

/// A stack an explicit `effigy container up` keeps running (its record is
/// active and persistent) is borrowed: the run leaves it running (Tom's
/// ruling 5; contract 053 §5). Only an inventory that proves it is not
/// running makes it this run's to stop; an unreadable one keeps it borrowed,
/// since an unknown never grants teardown (R3).
fn environment_borrowed(repo: &std::path::Path, policy: &EffectiveContainerPolicy) -> bool {
    use effigy_containers::environment::{EnvState, Retention};
    use effigy_kernel::observe::Observed;
    let persistent = crate::runner::environment::record(policy).is_some_and(|record| {
        record.state == EnvState::Active
            && record
                .engine()
                .is_some_and(|engine| engine.retention == Retention::Persistent)
    });
    persistent
        && !matches!(
            crate::runner::environment::running(repo, policy),
            Ok(Observed::Verified(false, _))
        )
}

fn managed_environment_up_command(
    repo: &std::path::Path,
    selector: Option<&str>,
    executable: &str,
) -> String {
    let mut command = format!("{executable} container");
    if let Some(selector) = selector {
        command.push(' ');
        command.push_str(&effigy_core::shell::shell_quote(selector));
    }
    command.push_str(&format!(
        " up --detach --repo {}",
        effigy_core::shell::shell_quote(&repo.display().to_string())
    ));
    command
}

/// Every standard entry as a workload of the run. Host entries get the
/// task's secrets through their spawn environment (K12); exec entries attach
/// through the environment's exec surface once it is ready, and record the
/// remote group their leader starts.
fn managed_workloads(
    context: &ManagedContext<'_, '_>,
    plan: &effigy_managed::ManagedTaskPlan,
    secrets: &[(String, SecretString)],
    token: &str,
    container_repo_root: Option<&std::path::Path>,
) -> Result<Vec<WorkloadSpec>, RunnerError> {
    let repo = context.repo();
    let binding = context.binding_resolution.binding();
    let inline = inline_policy(context)?;
    let selector = binding
        .container_name()
        .and_then(normalize_managed_lifecycle_container_ref)
        .map(str::to_owned);
    let mut workloads = Vec::new();
    for process in plan
        .processes
        .iter()
        .filter(|process| process.role == ManagedProcessRole::Standard)
    {
        let run = crate::runner::secret_session::inject_secret_passphrase_into_internal_command(
            process.run.clone(),
        );
        let setup = process.setup.as_ref().map(|setup| {
            crate::runner::secret_session::inject_secret_passphrase_into_internal_command(
                setup.clone(),
            )
        });
        let runs_on_host = process.run_on_host
            || context.container_handoff
            || matches!(
                binding,
                ContainerExecutionBinding::None | ContainerExecutionBinding::Host
            );
        let mut plain =
            BTreeMap::from([(effigy_managed::RUN_TOKEN_ENV.to_owned(), token.to_owned())]);
        let mut secret = BTreeMap::new();
        let (command, exec_group) = if runs_on_host {
            secret.extend(
                secrets
                    .iter()
                    .map(|(key, value)| (key.clone(), value.expose().to_owned())),
            );
            secret.extend(process.secret_env.clone());
            let command = if context.container_handoff {
                render_handoff_managed_standard_command(setup.as_deref(), &run)
            } else {
                match setup.as_deref() {
                    Some(setup) => format!("{setup}\n{run}"),
                    None => run,
                }
            };
            (command, None)
        } else {
            plain.clear();
            let wrapped = |command: String| {
                wrap_with_managed_secret_env(
                    wrap_command_with_run_token(command, Some(token), repo),
                    secrets,
                    repo,
                )
            };
            let run = wrapped(run);
            let setup = setup.map(wrapped);
            let relative = std::path::Path::new(EXEC_MARKER_DIR).join(format!(
                "{}+{}",
                marker_component(&context.preflight.selector.task_name),
                marker_component(&plan.profile)
            ));
            let file = format!("{}.pgid", marker_component(&process.name));
            let host_marker = repo.join(&relative).join(&file);
            let container_marker = container_repo_root
                .unwrap_or(repo)
                .join(&relative)
                .join(&file);
            let (command, binding) = match &inline {
                Some(policy) => (
                    render_inline_managed_exec_command(InlineManagedExecRequest {
                        repo_root: repo,
                        policy,
                        process_cwd: &process.cwd,
                        container_repo_root,
                        setup_command: setup.as_deref(),
                        command: &run,
                        exec_group: Some((&container_marker, managed_run::EXEC_NONCE)),
                    }),
                    Binding::Inline {
                        task: context.preflight.invocation.name.clone(),
                    },
                ),
                None => {
                    let executable =
                        resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
                    (
                        managed_exec_command(ManagedExecRequest {
                            repo_root: repo,
                            container_name: selector.as_deref(),
                            process_cwd: &process.cwd,
                            container_repo_root,
                            setup_command: setup.as_deref(),
                            executable: &executable,
                            command: &run,
                            exec_group: Some((&container_marker, managed_run::EXEC_NONCE)),
                        }),
                        Binding::Named {
                            selector: selector.clone(),
                        },
                    )
                }
            };
            (
                command,
                Some(ExecGroupSpec {
                    repo: repo.to_path_buf(),
                    binding,
                    marker: host_marker,
                }),
            )
        };
        workloads.push(WorkloadSpec {
            name: process.name.clone(),
            command,
            cwd: process.cwd.clone(),
            env: managed_run::workload_env(&plain, &secret),
            pty: true,
            start_after_ms: process.start_after_ms,
            shutdown_on_exit: process.shutdown_on_exit,
            exec_group,
        });
    }
    Ok(workloads)
}

/// One path component per name, injective (K1): `+` separates components
/// and is never left bare, and every byte outside `[A-Za-z0-9_-]` is
/// escaped as `%XX`, so distinct names never share a marker.
fn marker_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// A settle's release of an inline environment the run started: the task's
/// inline policy, re-resolved from the repository, brought down by Compose.
pub(in crate::runner) fn release_inline_managed_environment(
    repo: &std::path::Path,
    task: &str,
) -> Result<String, RunnerError> {
    let policy = inline_managed_policy(repo, task)?.ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "task `{task}` no longer resolves to an inline workspace container"
        ))
    })?;
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo,
        &policy,
        compose_args(&policy, ["down", "--remove-orphans"]),
        effigy_containers::ContainerAction::Shutdown,
        "inline managed environment down",
    )
    .map_err(RunnerError::from)?;
    let output = effigy_runtime::signals::run_compose_plan_capture(&policy, &plan)
        .map_err(RunnerError::from)?;
    if output.status.success() {
        Ok(format!("inline environment `{}` is down", policy.name))
    } else {
        Err(RunnerError::task_invocation(format!(
            "inline environment `{}` down exited {:?}: {}",
            policy.name,
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// The inline workspace container policy of `task` under `repo`.
pub(in crate::runner) fn inline_managed_policy(
    repo: &std::path::Path,
    task: &str,
) -> Result<Option<EffectiveContainerPolicy>, RunnerError> {
    let invocation = effigy_cli::TaskInvocation {
        name: task.to_owned(),
        args: Vec::new(),
    };
    let preflight =
        super::super::preflight::build_execution_preflight(&invocation, repo.to_path_buf())?;
    let selection = match super::super::selection::resolve_task_selection(&invocation, &preflight)?
    {
        super::super::selection::SelectionResolution::Selected { selection, .. } => selection,
        super::super::selection::SelectionResolution::Output(_) => return Ok(None),
    };
    let resolution = resolve_execution_binding_resolution(
        selection
            .catalog
            .manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "managed environment settle",
    )?;
    if !resolution.is_inline_container() {
        return Ok(None);
    }
    resolution.effective_policy(&selection.catalog.catalog_root)
}

/// Carry the managed run's settle token into a command that runs on the far
/// side of the exec surface, where the host environment cannot follow.
/// Host children inherit the variable from the run itself instead.
fn wrap_command_with_run_token(
    command: String,
    managed_run_token: Option<&str>,
    repo_root: &std::path::Path,
) -> String {
    let Some(token) = managed_run_token else {
        return command;
    };
    effigy_managed::wrap_command_with_env(
        command,
        &BTreeMap::from([(effigy_managed::RUN_TOKEN_ENV.to_owned(), token.to_owned())]),
        repo_root,
    )
}

/// One unique token identifying this managed run's settle.
fn generate_managed_run_token() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "managedrun-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

fn wrap_with_managed_secret_env(
    command: String,
    managed_task_secret_pairs: &[(String, SecretString)],
    repo_root: &std::path::Path,
) -> String {
    if managed_task_secret_pairs.is_empty() {
        return command;
    }
    let env = managed_task_secret_pairs
        .iter()
        .map(|(key, value)| (key.clone(), value.expose().to_owned()))
        .collect::<BTreeMap<String, String>>();
    wrap_command_with_env(command, &env, repo_root)
}

/// Resolves the catalog env schema plain env for shell/lifecycle role
/// processes, inheriting the nearest ancestor catalog's `[env_schema]` when
/// the selection catalog declares none (same rule as task rendering).
fn managed_role_schema_env(
    catalogs: &[effigy_manifest::LoadedCatalog],
    catalog_root: &std::path::Path,
    runtime_env_schema_override: Option<&std::path::Path>,
) -> Result<BTreeMap<String, String>, RunnerError> {
    let declaring = effigy_manifest::env_schema_declaring_catalog(catalogs, catalog_root);
    let resolved = super::standard::resolve_env_schema_if_present(
        declaring.map_or(catalog_root, |catalog| catalog.catalog_root.as_path()),
        runtime_env_schema_override,
        declaring.and_then(|catalog| catalog.manifest.env_schema.as_ref()),
        None,
    )?;
    Ok(resolved
        .map(|resolved| resolved.plain_env())
        .unwrap_or_default())
}

fn managed_ready_message(
    explicit_ready_message: Option<&str>,
    policy: Option<&EffectiveContainerPolicy>,
) -> Option<String> {
    if let Some(ready_message) = explicit_ready_message
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(ready_message.to_owned());
    }

    let policy = policy?;
    let mut seen = std::collections::HashSet::new();
    let urls = policy
        .dns_routes
        .iter()
        .filter_map(|route| {
            let scheme = if route.tls { "https" } else { "http" };
            let url = format!("{scheme}://{}", route.domain);
            seen.insert(url.clone()).then_some(url)
        })
        .collect::<Vec<_>>();

    if urls.is_empty() {
        None
    } else {
        Some(format!("routes: {}", urls.join(" | ")))
    }
}

fn managed_dns_route_lines(policy: Option<&EffectiveContainerPolicy>) -> Vec<String> {
    let Some(policy) = policy else {
        return Vec::new();
    };

    let mut seen = std::collections::HashSet::new();
    let mut routes = Vec::new();
    for route in &policy.dns_routes {
        let domain = route.domain.trim();
        if domain.is_empty() {
            continue;
        }
        let scheme = if route.tls { "https" } else { "http" };
        let service = route.service.as_deref().unwrap_or(&policy.primary_service);
        let mut line = format!("{scheme}://{domain} -> {service}");
        if let Some(port) = route.port {
            line.push_str(&format!(":{port}"));
        }
        if seen.insert(line.clone()) {
            routes.push(line);
        }
    }

    let Some(base_domain) = policy.dns_effective_base_domain() else {
        return routes;
    };

    let explicit_domains = policy
        .dns_routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<std::collections::HashSet<_>>();
    for alias in &policy.service_aliases {
        let domain = format!("{}.{}", alias.domain_label, base_domain);
        if explicit_domains.contains(domain.as_str()) {
            continue;
        }
        let line = format!("{domain}:{} -> {}", alias.container_port, alias.service);
        if seen.insert(line.clone()) {
            routes.push(line);
        }
    }

    routes
}

fn render_handoff_managed_standard_command(setup_command: Option<&str>, run: &str) -> String {
    let Some(setup_command) = setup_command
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return run.to_owned();
    };
    format!(
        "if ! {setup_command}; then printf '%s\\n' 'managed setup failed before process launch' 1>&2; exit 1; fi; {run}"
    )
}

fn default_handoff_managed_shell_run() -> String {
    "if [ -n \"${SHELL:-}\" ] && [ -x \"${SHELL}\" ]; then exec \"${SHELL}\" -i; fi; if command -v bash >/dev/null 2>&1; then exec \"$(command -v bash)\" -i; fi; if command -v sh >/dev/null 2>&1; then exec \"$(command -v sh)\" -i; fi; exec /bin/sh -i".to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        default_handoff_managed_shell_run, managed_dns_route_lines, managed_role_schema_env,
        managed_task_scratch, render_handoff_managed_standard_command,
        should_open_workspace_shell_for_non_managed_task, ContainerExecutionBinding,
    };
    use crate::runner::execute::workspace_seeded::render_workspace_seeded_task_command;
    use crate::runner::managed_shell::managed_readiness_probe_urls;
    use effigy_containers::{
        EffectiveComposeSource, EffectiveContainerPolicy, EffectiveDnsRoute, EffectiveServiceAlias,
    };
    use effigy_manifest::{
        LoadedCatalog, ManifestContainerDriver, ManifestContainerOnTaskExit,
        ManifestContainerShutdownMode, ManifestContainerStartup,
    };
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn a_managed_task_honours_its_scratch_opt_out() {
        let uses: effigy_manifest::ManifestTask =
            toml::from_str("mode = \"tui\"\nrun = [\"bun run dev\"]").expect("parse task");
        assert!(
            managed_task_scratch(&uses),
            "a managed session uses the configured scratch by default"
        );

        let opted_out: effigy_manifest::ManifestTask =
            toml::from_str("mode = \"tui\"\nscratch = false\nrun = [\"bun run dev\"]")
                .expect("parse task");
        assert!(
            !managed_task_scratch(&opted_out),
            "a managed task that opts out plans no host scratch"
        );
    }

    #[test]
    fn handoff_shell_default_avoids_hardcoded_zsh_requirement() {
        let rendered = default_handoff_managed_shell_run();

        assert!(rendered.contains("exec \"${SHELL}\" -i"), "got: {rendered}");
        assert!(rendered.contains("command -v bash"), "got: {rendered}");
        assert!(rendered.contains("command -v sh"), "got: {rendered}");
        assert!(rendered.contains("exec /bin/sh -i"), "got: {rendered}");
        assert!(!rendered.contains("/bin/zsh"), "got: {rendered}");
    }

    #[test]
    fn workspace_seeded_task_command_preserves_passthrough_args() {
        let rendered = render_workspace_seeded_task_command(
            "dev",
            &[
                "front".to_owned(),
                "--".to_owned(),
                "--host".to_owned(),
                "0.0.0.0".to_owned(),
            ],
        );

        assert!(
            rendered.ends_with("effigy 'dev' 'front' '--' '--host' '0.0.0.0'"),
            "got: {rendered}"
        );
    }

    #[test]
    fn non_managed_container_task_with_run_skips_workspace_shell_handoff() {
        assert!(!should_open_workspace_shell_for_non_managed_task(
            true,
            &ContainerExecutionBinding::Container {
                name: Some("web".to_owned()),
                workspace: None,
            },
            false,
        ));
    }

    #[test]
    fn non_managed_container_task_without_run_opens_workspace_shell() {
        assert!(should_open_workspace_shell_for_non_managed_task(
            false,
            &ContainerExecutionBinding::Container {
                name: Some("web".to_owned()),
                workspace: None,
            },
            false,
        ));
    }

    #[test]
    fn handoff_managed_standard_command_runs_setup_before_process() {
        let rendered =
            render_handoff_managed_standard_command(Some("printf setup-ok"), "bun run dev");

        let setup_index = rendered.find("printf setup-ok").expect("setup command");
        let run_index = rendered.find("bun run dev").expect("run command");
        assert!(setup_index < run_index, "got: {rendered}");
        assert!(
            rendered.contains("managed setup failed before process launch"),
            "got: {rendered}"
        );
    }

    #[test]
    fn managed_dns_route_lines_include_http_routes_and_service_aliases() {
        let mut policy = test_policy();
        policy.primary_service = "app".to_owned();
        policy.dns_routes = vec![
            EffectiveDnsRoute {
                domain: "project.test".to_owned(),
                declared_domain: "project.test".to_owned(),
                tls: false,
                port: None,
                service: None,
                target_host: None,
            },
            EffectiveDnsRoute {
                domain: "admin.project.test".to_owned(),
                declared_domain: "admin.project.test".to_owned(),
                tls: true,
                port: Some(41002),
                service: Some("admin".to_owned()),
                target_host: None,
            },
        ];
        policy.service_aliases = vec![EffectiveServiceAlias {
            service: "postgres".to_owned(),
            domain_label: "postgres".to_owned(),
            container_port: 5432,
        }];

        let routes = managed_dns_route_lines(Some(&policy));

        assert_eq!(
            routes,
            vec![
                "http://project.test -> app".to_owned(),
                "https://admin.project.test -> admin:41002".to_owned(),
                "postgres.project.test:5432 -> postgres".to_owned(),
            ]
        );
    }

    #[test]
    fn managed_readiness_probe_urls_skip_routes_owned_by_external_processes() {
        let mut policy = test_policy();
        policy.dns_routes = vec![
            EffectiveDnsRoute {
                domain: "project.test".to_owned(),
                declared_domain: "project.test".to_owned(),
                tls: false,
                port: None,
                service: None,
                target_host: None,
            },
            EffectiveDnsRoute {
                domain: "admin.project.test".to_owned(),
                declared_domain: "admin.project.test".to_owned(),
                tls: true,
                port: Some(41002),
                service: Some("admin".to_owned()),
                target_host: None,
            },
            EffectiveDnsRoute {
                domain: "api.project.test".to_owned(),
                declared_domain: "api.project.test".to_owned(),
                tls: true,
                port: None,
                service: None,
                target_host: Some("host.docker.internal:41001".to_owned()),
            },
        ];

        let urls = managed_readiness_probe_urls(Some(&policy));

        assert_eq!(
            urls,
            vec![
                "http://project.test".to_owned(),
                "https://admin.project.test".to_owned(),
            ]
        );
    }

    fn managed_role_schema_test_repo(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "effigy-managed-role-schema-{name}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("mkdir temp repo");
        root
    }

    fn load_schema_test_catalog(catalog_root: &Path, alias: &str, depth: usize) -> LoadedCatalog {
        let manifest = effigy_manifest::load_task_manifest(&catalog_root.join("effigy.toml"))
            .expect("manifest");
        LoadedCatalog::for_test(alias, catalog_root, manifest, depth)
    }

    #[test]
    fn managed_role_schema_env_inherits_ancestor_catalog_env_schema() {
        let root = managed_role_schema_test_repo("ancestor");
        let child = root.join("cp-api");
        fs::create_dir_all(&child).expect("mkdir child catalog");
        fs::write(
            root.join("effigy.toml"),
            "[env_schema]\nschema = \"dev.env.schema\"\n",
        )
        .expect("root manifest");
        fs::write(root.join("dev.env.schema"), "ENVIRONMENT=effigy\n").expect("write schema");
        fs::write(
            child.join("effigy.toml"),
            "[tasks.dev]\nrun = \"printf ok\"\n",
        )
        .expect("child manifest");
        let catalogs = vec![
            load_schema_test_catalog(&root, "root", 0),
            load_schema_test_catalog(&child, "cp-api", 1),
        ];

        let env = managed_role_schema_env(&catalogs, &child, None).expect("resolve schema env");

        assert_eq!(env.get("ENVIRONMENT").map(String::as_str), Some("effigy"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn managed_role_schema_env_is_empty_without_any_schema() {
        let root = managed_role_schema_test_repo("none");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.dev]\nrun = \"printf ok\"\n",
        )
        .expect("manifest");
        let catalogs = vec![load_schema_test_catalog(&root, "root", 0)];

        let env = managed_role_schema_env(&catalogs, &root, None).expect("resolve schema env");

        assert!(env.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            render: None,
            repo_root: std::path::PathBuf::from("/tmp"),
            name: "stack".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Attached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![std::path::PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            borrowed_volumes: Vec::new(),
            data_peers: Vec::new(),
            data_from: None,
            shared_services: vec![],
            project_name: "stack".to_owned(),
            primary_service: "workspace".to_owned(),
            dns_domain: None,
            dns_base_domain: None,
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
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }
}
