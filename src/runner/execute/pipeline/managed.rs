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
    self, Addressed, Binding, ClientStart, EnvironmentPlan, ExecGroupSpec, Front, HostProcessTab,
    ManagedTab, OwnedRun, TabSource, WorkloadSpec,
};
use crate::runner::task_engine::{
    resolve_lock_wait_ms, RunKey, TaskRun, TaskRunRequest, TaskTarget,
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
    managed_execution_mode, parse_managed_invocation, render_managed_plan, ManagedExecutionMode,
    ManagedInvocation,
};
use effigy_manifest::ManifestTaskSecretsMode;
use effigy_manifest::TaskSelection;
use std::collections::BTreeMap;
use std::time::Duration;

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
    // A managed session in a runtime-scoped checkout resolves the canonical
    // declared backing-service alias in every process it places on the host -
    // including the host entries of a container-bound session, whose own
    // processes keep the declared name on their target's network
    // (effigy#431, contract 005). A plan renders nothing, so it resolves
    // nothing.
    let host_placed = matches!(
        container_binding,
        ContainerExecutionBinding::None | ContainerExecutionBinding::Host
    ) || container_handoff;
    let mut host_aliases = match execution_mode {
        ManagedExecutionMode::RenderPlan => None,
        _ => super::super::host_alias::session_host_alias_resolver(preflight, selection)?,
    };
    let host_env_aliases = match (&host_aliases, execution_mode) {
        (Some(resolver), _) => resolver.hosts(),
        (None, ManagedExecutionMode::RenderPlan) => {
            super::super::host_alias::session_host_plan_env_aliases(preflight, selection)?
        }
        (None, _) => Vec::new(),
    };
    let plan = resolve_managed_task_plan(
        &preflight.selector,
        selection.catalog,
        selection.task,
        runtime_args,
        &preflight.catalogs,
        &selection.catalog.catalog_root,
        &host_env_aliases,
        host_placed,
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
    // A rendered session command that names this checkout's effective alias
    // owes the same published-backing-service proof a plain host run owes, and
    // a refusal here stops the session before its supervisor starts
    // (effigy#431, contract 005). The client proves it; a supervisor continues
    // the run its client already proved.
    if owner.is_none() {
        if let Some(resolver) = host_aliases.as_mut() {
            for process in &plan.processes {
                if !(host_placed || process.run_on_host) {
                    continue;
                }
                resolver.require_published_in(&preflight.selector.task_name, &process.run)?;
                if let Some(setup) = process.setup.as_deref() {
                    resolver.require_published_in(&preflight.selector.task_name, setup)?;
                }
            }
        }
    }
    let mut context = ManagedContext {
        preflight,
        selection,
        binding_resolution: &binding_resolution,
        container_handoff,
        host_aliases: &mut host_aliases,
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
        Some(owner) => own_managed_run(owner, &mut context, plan),
        None => {
            let front = match (execution_mode, detach) {
                (_, true) => Front::Detached,
                (ManagedExecutionMode::Tui, false) => Front::Tui,
                _ => Front::Text,
            };
            start_managed_run(&mut context, plan, front)
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
    let environment_bound = effigy_managed::task_has_environment_binding(
        selection.catalog,
        &preflight.selector.task_name,
        selection.task,
    )?;
    if !effigy_managed::has_managed_profile(selection.task, profile, environment_bound) {
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
struct ManagedContext<'a, 'catalog, 'aliases> {
    preflight: &'a ExecutionPreflight,
    selection: &'a TaskSelection<'catalog>,
    binding_resolution: &'a super::super::api::ExecutionBindingResolution,
    container_handoff: bool,
    /// The scoped alias resolution this host session's environments use.
    host_aliases: &'aliases mut Option<super::super::host_alias::HostAliasResolver>,
}

impl ManagedContext<'_, '_, '_> {
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
    context: &mut ManagedContext<'_, '_, '_>,
    plan: effigy_managed::ManagedTaskPlan,
    front: Front,
) -> Result<Option<String>, RunnerError> {
    // Vault access is resolved once here, on the real terminal, before any
    // front end renders (TUI, headless or detach) and before the supervisor
    // starts: everything the session starts receives it on the internal env
    // route, so no tab, host process or nested Effigy run asks again
    // (Q-007; Tom 2026-10-23).
    resolve_managed_session_secret_access(
        &context.preflight.resolved.resolved_root,
        &context.preflight.selector.task_name,
        &plan.profile,
    )?;
    // Reading the task's secrets here prompts once for the passphrase on
    // this terminal; the supervisor receives it in its spawn environment.
    let secrets = context.task_secrets()?;
    // This checkout's scope resolves the values the session would otherwise
    // inherit unchanged, and refuses before its supervisor starts when one has
    // no published backing service. The supervisor receives the resolved values
    // in its spawn environment (effigy#431 review).
    let resolved_inherited_env = match context.host_aliases.as_mut() {
        Some(resolver) => {
            let mut delivered = context
                .selection
                .task
                .env
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            delivered.extend(secrets.iter().map(|(key, _)| key.clone()));
            resolver.resolved_inherited_env(&delivered, true)?
        }
        None => Vec::new(),
    };
    let target = context.target(&plan.profile);
    let addressed = Addressed {
        target: &target,
        task: &context.preflight.selector.task_name,
        profile: &plan.profile,
    };
    let terminal = (front == Front::Tui)
        .then(crossterm::terminal::size)
        .and_then(Result::ok);
    // The mutating session resolves its environment for the front end before
    // its supervisor starts. Plans and controls never enter this path.
    if plan.owns_environment && !context.container_handoff {
        effigy_core::worktree_scope::prepare_checkout_token(context.repo()).map_err(|error| {
            RunnerError::task_invocation(format!("cannot record the checkout identity: {error}"))
        })?;
    }
    // What each front end shows is resolved before the run starts, so a
    // front-end resolution failure never leaves a supervisor running.
    let tui = match front {
        Front::Tui => Some(tui_tabs(context, &plan)?),
        _ => None,
    };
    let text_hosts = match front {
        Front::Text => environment_host_processes(context, &plan)?,
        _ => Vec::new(),
    };
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
        resolved_env: &resolved_inherited_env,
    })?;
    match front {
        Front::Detached => managed_run::detach(started, &addressed),
        Front::Text => managed_run::attach_text(started, &addressed, text_hosts),
        Front::Tui => {
            let ManagedTabs {
                tabs,
                order,
                shells,
            } = tui.expect("the TUI front end resolves its tabs before the run starts");
            managed_run::attach_tui(
                started,
                &addressed,
                context.repo().to_path_buf(),
                tabs,
                order,
                shells,
            )
        }
    }
}

/// One managed session's vault access, resolved once before the front end
/// renders (contract 032, Q-007 "Secrets before the TUI").
///
/// The order is the existing one: what this invocation already holds, then
/// the sealed command credential `effigy secrets unlock` leaves, then one
/// prompt on the real terminal. A caller-supplied
/// [`crate::runner::secret_session::session_secret_passphrase_env`] answers
/// without a terminal for headless runs.
///
/// The local-dev key deliberately does not answer this: it is the factor a
/// resolved `dev` session uses for its own secrets, and the processes a
/// session starts are not that session. So a `dev` session whose vault has
/// only that key asks once here, before anything renders, rather than inside
/// a tab; the key's reach is unchanged.
pub(in crate::runner) fn resolve_managed_session_secret_access(
    repo_root: &std::path::Path,
    task: &str,
    profile: &str,
) -> Result<(), RunnerError> {
    if crate::runner::secret_session::session_passphrase().is_some() {
        return Ok(());
    }
    let Some(vault_path) = super::standard::managed_session_vault_path(repo_root)? else {
        return Ok(());
    };
    let passphrase_env = crate::runner::secret_session::session_secret_passphrase_env();
    let missing_tty_error = format!(
        "managed task `{task}` (profile `{profile}`) resolves vault access before it renders, \
         and {} is locked: secret input requires an interactive TTY. Unlock it once with \
         `effigy secrets unlock`, or set `{passphrase_env}` for this invocation",
        vault_path.display()
    );
    crate::runner::secret_session::read_vault_passphrase(
        &vault_path,
        false,
        "Vault passphrase: ",
        &missing_tty_error,
    )?;
    Ok(())
}

/// The plan as its supervisor would run it: the environment's `up`, each
/// workload's command and each shell tab's attach. A pure read (R2): no
/// secret is resolved, so none can reach the rendered plan.
fn displayed_plan(
    context: &ManagedContext<'_, '_, '_>,
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
        &[],
        "<run token>",
        container_repo_root.as_deref(),
        false,
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

/// Whether this managed run owns (or borrows) the task's container
/// environment. The plan says whether the session asked for the environment
/// (a `role = "lifecycle"` entry, `container_lifecycle = true`, or a plain
/// bound session with no entries of its own); the runner only checks that the
/// binding resolves to an environment policy. Placement is the environment's
/// property, so both placements own it the same way (effigy#322, Q-007).
fn environment_owned(
    context: &ManagedContext<'_, '_, '_>,
    plan: &effigy_managed::ManagedTaskPlan,
) -> Result<bool, RunnerError> {
    if context.container_handoff || !plan.owns_environment {
        return Ok(false);
    }
    Ok(environment_policy(context)?.is_some())
}

/// The environment policy a session resolves for its tabs and for `up`: the
/// task's inline workspace container first, then its named environment.
fn environment_policy(
    context: &ManagedContext<'_, '_, '_>,
) -> Result<Option<EffectiveContainerPolicy>, RunnerError> {
    match inline_policy(context)? {
        Some(policy) => Ok(Some(policy)),
        None => named_policy(context),
    }
}

/// The environment's host processes as tabs and followed logs. They are the
/// environment's own (`[[containers.<env>.host_processes]]`), started by its
/// `up`, so every placement of the run shows them the same way.
fn environment_host_processes(
    context: &ManagedContext<'_, '_, '_>,
    plan: &effigy_managed::ManagedTaskPlan,
) -> Result<Vec<HostProcessTab>, RunnerError> {
    if !environment_owned(context, plan)? {
        return Ok(Vec::new());
    }
    let Some(policy) = environment_policy(context)? else {
        return Ok(Vec::new());
    };
    Ok(HostProcessTab::for_environment(context.repo(), &policy))
}

/// A managed session's TUI front end: the tabs, the order that names every
/// one of them, and the local shell tabs this client runs itself.
struct ManagedTabs {
    tabs: Vec<ManagedTab>,
    order: Vec<String>,
    shells: Vec<effigy_process::ProcessSpec>,
}

/// The TUI's tabs: the run's own output, one tab per environment host process
/// with its status line and live log, then the task's workloads and shells.
/// The order names every tab, so a session whose first tab is the
/// environment's also finishes its startup projection.
fn tui_tabs(
    context: &ManagedContext<'_, '_, '_>,
    plan: &effigy_managed::ManagedTaskPlan,
) -> Result<ManagedTabs, RunnerError> {
    let owns_environment = environment_owned(context, plan)?;
    let hosts = environment_host_processes(context, plan)?;
    let (tabs, tab_order) = session_tabs(owns_environment, hosts, &plan.processes, &plan.tab_order);
    let mut shells = Vec::new();
    for process in plan
        .processes
        .iter()
        .filter(|process| process.role == ManagedProcessRole::Shell)
    {
        shells.push(effigy_process::ProcessSpec {
            name: process.name.clone(),
            run: shell_command(context, process)?,
            cwd: process.cwd.clone(),
            start_after_ms: 0,
            shutdown_on_exit: process.shutdown_on_exit,
            pty: true,
            // A shell tab is the operator's own terminal, so it is user code:
            // no session access reaches it. A nested `effigy` run there is a
            // top-level invocation and resolves its own access (`dev`'s
            // local-dev rules, or a prompt).
            env: BTreeMap::from([(
                effigy_ui::steps::MANAGED_PANE_ENV.to_owned(),
                "1".to_owned(),
            )]),
        });
    }
    Ok(ManagedTabs {
        tabs,
        order: tab_order,
        shells,
    })
}

/// The tab set and its order, without any shell command to render: the run's
/// own output when this session owns its environment, the environment's host
/// processes in their declaration order, then every declared process. A host
/// process whose name a workload already uses keeps its own tab name.
fn session_tabs(
    owns_environment: bool,
    hosts: Vec<HostProcessTab>,
    processes: &[effigy_managed::ManagedProcessSpec],
    declared_tab_order: &[String],
) -> (Vec<ManagedTab>, Vec<String>) {
    let mut tabs = Vec::new();
    let lifecycle = processes
        .iter()
        .find(|process| process.role == ManagedProcessRole::Lifecycle);
    let lifecycle_name = lifecycle
        .map(|process| process.name.clone())
        .unwrap_or_else(|| super::super::super::managed_run::OUTPUT_LOG.to_owned());
    if owns_environment {
        tabs.push(ManagedTab {
            name: lifecycle_name.clone(),
            pty: false,
            shutdown_on_exit: lifecycle.is_some_and(|process| process.shutdown_on_exit),
            source: TabSource::Output,
        });
    }
    let mut taken = processes
        .iter()
        .map(|process| process.name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    taken.insert(lifecycle_name.clone());
    let mut host_names = Vec::new();
    for host in hosts {
        let mut name = host.name.clone();
        if taken.contains(&name) {
            name = format!("host:{name}");
        }
        taken.insert(name.clone());
        host_names.push(name.clone());
        tabs.push(ManagedTab {
            name,
            pty: false,
            // A host process belongs to the environment, not to a workload's
            // exit policy: its lifecycle is the environment's.
            shutdown_on_exit: false,
            source: TabSource::HostProcess(host),
        });
    }
    for process in processes {
        let source = match process.role {
            ManagedProcessRole::Lifecycle => continue,
            ManagedProcessRole::Standard => TabSource::Workload,
            ManagedProcessRole::Shell => TabSource::Shell,
        };
        tabs.push(ManagedTab {
            name: process.name.clone(),
            pty: true,
            shutdown_on_exit: process.shutdown_on_exit,
            source,
        });
    }
    let mut order = declared_tab_order.to_vec();
    if owns_environment && !order.iter().any(|name| name == &lifecycle_name) {
        order.insert(0, lifecycle_name.clone());
    }
    // The environment's host processes sit next to the run's own output, in
    // their declaration order; `tab` on a workload entry still positions it.
    let anchor = order
        .iter()
        .position(|name| name == &lifecycle_name)
        .map(|index| index + 1)
        .unwrap_or(0);
    for (offset, name) in host_names.iter().enumerate() {
        if !order.contains(name) {
            order.insert(anchor + offset, name.clone());
        }
    }
    for tab in &tabs {
        if !order.contains(&tab.name) {
            order.push(tab.name.clone());
        }
    }
    (tabs, order)
}

/// A shell tab's interactive attach, run by the TUI client itself.
fn shell_command(
    context: &ManagedContext<'_, '_, '_>,
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
    context: &ManagedContext<'_, '_, '_>,
) -> Result<Option<EffectiveContainerPolicy>, RunnerError> {
    if context.binding_resolution.is_inline_container() {
        context.binding_resolution.effective_policy(context.repo())
    } else {
        Ok(None)
    }
}

fn named_policy(
    context: &ManagedContext<'_, '_, '_>,
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
    context: &mut ManagedContext<'_, '_, '_>,
    mut plan: effigy_managed::ManagedTaskPlan,
) -> Result<Option<String>, RunnerError> {
    let pane_feedback = owner.payload.front == Front::Tui;
    let preflight = context.preflight;
    let selection = context.selection;
    let token = generate_managed_run_token();
    // Anything that fails before admission is the supervisor's refusal: the
    // client reports it, and nothing ran.
    let inputs = (|| -> Result<_, RunnerError> {
        let env_schema_resolved = super::standard::resolve_env_schema_if_present(
            &selection.catalog.catalog_root,
            preflight.runtime_args_raw.env_schema_override.as_deref(),
            selection.catalog.manifest.env_schema.as_ref(),
            None,
        )?;
        let mut secrets = context.task_secrets()?;
        // A host session in a runtime-scoped checkout resolves the canonical
        // declared backing-service alias in the secret values it delivers, and
        // refuses before the supervisor starts when this checkout has no
        // published backing service for them (effigy#431, contract 005). A key
        // the session's own rendered environment delivers wins over a secret of
        // the same name, so that secret is neither delivered again nor proved.
        let mut inherited_env = Vec::new();
        if let Some(resolver) = context.host_aliases.as_mut() {
            let mut delivered: std::collections::BTreeSet<String> = selection
                .task
                .env
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            if let Some(resolved) = env_schema_resolved.as_ref() {
                delivered.extend(resolved.plain_env().keys().cloned());
            }
            let pairs = secrets
                .iter()
                .map(|(key, value)| (key.as_str(), value))
                .collect::<Vec<_>>();
            secrets = resolver.resolved_secret_env(&pairs, &delivered, true)?;
            delivered.extend(secrets.iter().map(|(key, _)| key.clone()));
            // A host workload would otherwise inherit this value unchanged; it
            // names the declared alias just as a task's own env does
            // (effigy#431 review).
            inherited_env = resolver.resolved_inherited_env(&delivered, true)?;
        }
        let role_schema_env = managed_role_schema_env(
            &preflight.catalogs,
            &selection.catalog.catalog_root,
            preflight.runtime_args_raw.env_schema_override.as_deref(),
        )?;
        let container_repo_root = context
            .binding_resolution
            .exec_working_dir(context.repo())?;
        // The environment `up` is itself a child Effigy process, and it
        // receives this session's access through the one-shot descriptor
        // handoff its spawn attaches (effigy#338) — never in the rendered
        // command, never in its environment, and never in the plan `--plan`
        // displays.
        let environment = environment_plan(context, &plan, &token, &role_schema_env)?;
        let target = context.target(&plan.profile);
        let owned = OwnedRun {
            target,
            lock_wait_ms: resolve_lock_wait_ms(preflight.runtime_args_raw.lock_wait_ms)?,
            admission: selection.task.admission,
            cwd: preflight.invocation_cwd.clone(),
            args: preflight.runtime_args_exec.passthrough.clone(),
            token: token.clone(),
            task: preflight.selector.task_name.clone(),
            profile: plan.profile.clone(),
            // The catalog whose pins this run's commands resolve against. The
            // supervisor re-resolves it when it starts each process, after the
            // Prepare paths a setup step and the environment `up` run.
            source_manifest: selection.catalog.manifest_path.clone(),
            container_repo_root: container_repo_root.clone(),
            environment,
            fail_on_non_zero: plan.fail_on_non_zero,
            scratch: managed_task_scratch(selection.task),
            secrets: secrets
                .iter()
                .map(|(_, value)| value.expose().to_owned())
                // The session's vault access is a value the run's own output
                // must never show either: a child can echo it, and the
                // supervisor redacts what it knows (K12).
                .chain(
                    crate::runner::secret_session::session_passphrase()
                        .map(|value| value.expose().to_owned()),
                )
                .collect(),
        };
        Ok((
            secrets,
            inherited_env,
            env_schema_resolved,
            container_repo_root,
            owned,
        ))
    })();
    let (secrets, inherited_env, env_schema_resolved, container_repo_root, owned) = match inputs {
        Ok(inputs) => inputs,
        Err(error) => {
            // The failure's own category: an input that could not be read is
            // not a refusal (K4, F16).
            owner
                .session
                .refused_message(error.category(), error.to_string());
            return Err(error);
        }
    };
    let prepare = |run: &mut TaskRun| -> Result<effigy_managed::ManagedTaskPlan, RunnerError> {
        {
            let _parent = run.enter();
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
        Ok(plan)
    };
    // Rendered after the environment's own Prepare published its generation,
    // so a container-routed command never names the one it removed.
    let workloads = |_: &mut TaskRun, plan: effigy_managed::ManagedTaskPlan| {
        managed_workloads(
            context,
            &plan,
            &secrets,
            &inherited_env,
            &token,
            container_repo_root.as_deref(),
            pane_feedback,
        )
    };
    managed_run::own(owner, owned, prepare, workloads)
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
    context: &ManagedContext<'_, '_, '_>,
    plan: &effigy_managed::ManagedTaskPlan,
    token: &str,
    schema_env: &BTreeMap<String, String>,
) -> Result<Option<EnvironmentPlan>, RunnerError> {
    if !environment_owned(context, plan)? {
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
        exec_probe: exec_workloads.then(|| policy.primary_service.clone()),
        route_urls: if plan.readiness.health_wait {
            managed_readiness_probe_urls(Some(policy))
        } else {
            Vec::new()
        },
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
///
/// A workload never receives the session's vault access: not in its
/// environment, not in its rendered command, and not on a descriptor
/// (effigy#338). A nested `effigy` run inside one is a top-level invocation.
fn managed_workloads(
    context: &ManagedContext<'_, '_, '_>,
    plan: &effigy_managed::ManagedTaskPlan,
    secrets: &[(String, SecretString)],
    inherited_env: &[(String, String)],
    token: &str,
    container_repo_root: Option<&std::path::Path>,
    pane_feedback: bool,
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
        let mark = |command: String| {
            if pane_feedback {
                format!("export {}=1; {command}", effigy_ui::steps::MANAGED_PANE_ENV)
            } else {
                command
            }
        };
        let run = mark(process.run.clone());
        let setup = process.setup.clone().map(mark);
        let runs_on_host = process.run_on_host
            || context.container_handoff
            || matches!(
                binding,
                ContainerExecutionBinding::None | ContainerExecutionBinding::Host
            );
        let mut plain =
            BTreeMap::from([(effigy_managed::RUN_TOKEN_ENV.to_owned(), token.to_owned())]);
        let mut secret = BTreeMap::new();
        let (command, setup, exec_group) = if runs_on_host {
            secret.extend(
                secrets
                    .iter()
                    .map(|(key, value)| (key.clone(), value.expose().to_owned())),
            );
            secret.extend(process.secret_env.clone());
            // A host workload's inherited value Effigy resolved is delivered
            // plainly, exactly as the child would have inherited it.
            for (key, value) in inherited_env {
                plain.entry(key.clone()).or_insert_with(|| value.clone());
            }
            let command = if context.container_handoff {
                render_handoff_managed_standard_command(setup.as_deref(), &run)
            } else {
                match setup.as_deref() {
                    Some(setup) => format!("{setup}\n{run}"),
                    None => run,
                }
            };
            (command, None, None)
        } else {
            secret.extend(
                secrets
                    .iter()
                    .map(|(key, value)| (key.clone(), value.expose().to_owned())),
            );
            secret.extend(process.secret_env.clone());
            let (run, setup, names) = if secret.is_empty() {
                // No task values need a private delivery. The settle token is
                // public and retains the existing lightweight wrapper.
                plain.clear();
                let wrap = |command| {
                    effigy_managed::wrap_command_with_env(
                        command,
                        &BTreeMap::from([(
                            effigy_managed::RUN_TOKEN_ENV.to_owned(),
                            token.to_owned(),
                        )]),
                        repo,
                    )
                };
                (wrap(run), setup.map(wrap), Vec::new())
            } else {
                let names = plain
                    .keys()
                    .chain(secret.keys())
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                plain.insert(
                    crate::runner::managed_secrets::NAMES.to_owned(),
                    names.join(" "),
                );
                if inline.is_none() {
                    // `container shell --command` captures output with no input.
                    plain.insert(
                        crate::runner::managed_secrets::STDIN_FREE.to_owned(),
                        "1".to_owned(),
                    );
                }
                (run, setup, names)
            };
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
            // The setup is the route's own exec, run by the supervisor before
            // the process command is resolved: a setup that publishes a source
            // generation must not leave the process naming the one it removed.
            let executable = match &inline {
                Some(_) => None,
                None => Some(resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?),
            };
            let render = |command: &str, exec_group: Option<(&std::path::Path, &str)>| match &inline
            {
                Some(policy) => render_inline_managed_exec_command(InlineManagedExecRequest {
                    repo_root: repo,
                    policy,
                    process_cwd: &process.cwd,
                    container_repo_root,
                    setup_command: None,
                    command,
                    exec_group,
                    env_names: &names,
                }),
                None => managed_exec_command(ManagedExecRequest {
                    repo_root: repo,
                    container_name: selector.as_deref(),
                    process_cwd: &process.cwd,
                    container_repo_root,
                    setup_command: None,
                    executable: executable.as_deref().unwrap_or_default(),
                    command,
                    exec_group,
                }),
            };
            let command = render(&run, Some((&container_marker, managed_run::EXEC_NONCE)));
            let setup = setup.map(|setup| render(&setup, None));
            let binding = match &inline {
                Some(_) => Binding::Inline {
                    task: context.preflight.invocation.name.clone(),
                },
                None => Binding::Named {
                    selector: selector.clone(),
                },
            };
            (
                command,
                setup,
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
            setup,
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
    let output =
        effigy_runtime::signals::run_compose_plan_capture(&plan).map_err(RunnerError::from)?;
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
        managed_task_scratch, render_handoff_managed_standard_command, session_tabs,
        should_open_workspace_shell_for_non_managed_task, ContainerExecutionBinding,
        HostProcessTab, ManagedProcessRole, TabSource,
    };
    use crate::runner::execute::workspace_seeded::render_workspace_seeded_task_command;
    use crate::runner::managed_shell::managed_readiness_probe_urls;
    use effigy_containers::HostProcessRestart;
    use effigy_containers::{
        EffectiveComposeSource, EffectiveContainerPolicy, EffectiveDnsRoute, EffectiveServiceAlias,
    };
    use effigy_managed::ManagedProcessSpec;
    use effigy_manifest::{
        LoadedCatalog, ManifestContainerDriver, ManifestContainerOnTaskExit,
        ManifestContainerShutdownMode, ManifestContainerStartup,
    };
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn managed_placements_keep_task_values_out_of_commands_and_public_plans() {
        use super::*;
        for placement in ["host", "inline", "named"] {
            let fixture =
                effigy_test_support::Fixture::workspace("managed-delivery-plan-").unwrap();
            let root = fixture.path().canonicalize().unwrap();
            let workspace = if placement == "host" {
                "run_in = \"host\"\n"
            } else {
                "workspace = \"app\"\n"
            };
            let container = if placement == "inline" {
                "container = { image = \"alpine\", mount = \"./:/workspace\" }"
            } else {
                "container = \"web\""
            };
            let manifest = format!(
                r#"[tasks.dev]
mode = "tui"
{workspace}concurrent = [{{ name = "api", run = "printf '%s' \"$DATABASE_URL\"", start = 1 }}]
[systems]
default = "dev"
[systems.dev]
default_workspace = "app"
[systems.dev.workspaces.app]
{container}
[containers]
default = "web"
[containers.web]
compose_file = "compose.yaml"
project_name = "private-proof"
primary_service = "app"
working_dir = "/workspace"
"#
            );
            fs::write(root.join("effigy.toml"), manifest).unwrap();
            fs::write(
                root.join("compose.yaml"),
                "services:\n  app:\n    image: alpine\n",
            )
            .unwrap();
            let invocation = effigy_cli::TaskInvocation {
                name: "dev".into(),
                args: vec![],
            };
            let preflight = crate::runner::execute::preflight::build_execution_preflight(
                &invocation,
                root.clone(),
            )
            .unwrap();
            let selection = match crate::runner::execute::selection::resolve_task_selection(
                &invocation,
                &preflight,
            )
            .unwrap()
            {
                crate::runner::execute::selection::SelectionResolution::Selected {
                    selection,
                    ..
                } => selection,
                _ => panic!("expected task selection"),
            };
            let binding = resolve_execution_binding_resolution(
                None,
                selection.catalog.manifest.systems.as_ref(),
                selection.catalog.manifest.containers.as_ref(),
                "dev",
                selection.task,
                "secret proof",
            )
            .unwrap();
            let plan = resolve_managed_task_plan(
                &preflight.selector,
                selection.catalog,
                selection.task,
                &preflight.runtime_args_exec,
                &preflight.catalogs,
                &root,
                &effigy_routing::resolve_task_selection,
            )
            .unwrap()
            .unwrap();
            let context = ManagedContext {
                preflight: &preflight,
                selection: &selection,
                binding_resolution: &binding,
                container_handoff: false,
            };
            let secret = "plan-proof-task-secret";
            let workloads = managed_workloads(
                &context,
                &plan,
                &[("DATABASE_URL".into(), SecretString::new(secret.into()))],
                &[],
                "run-proof",
                Some(Path::new("/workspace")),
                false,
            )
            .unwrap();
            assert_eq!(workloads.len(), 1);
            let workload = &workloads[0];
            assert!(!workload.command.contains(secret));
            assert!(!workload
                .setup
                .as_deref()
                .unwrap_or_default()
                .contains(secret));
            assert!(workload.env.iter().any(|(key, value)| key == "DATABASE_URL" && matches!(value, crate::runner::task_engine::EnvOverride::Secret(value) if value == secret)));
            if placement != "host" {
                assert!(workload
                    .env
                    .iter()
                    .any(|(key, _)| key == crate::runner::managed_secrets::NAMES));
                assert!(
                    workload.exec_group.is_some(),
                    "remote process custody must remain registered"
                );
            }
            // Display resolution reads no secret and serializes no private delivery paths.
            let public = displayed_plan(&context, plan).unwrap();
            let rendered =
                render_managed_plan("dev", &root, &root.join("effigy.toml"), public).unwrap();
            assert!(!rendered.contains(secret));
            assert!(!rendered.contains("managed-secrets"));
        }
    }

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
            runtime_host_key: None,
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

    fn process(name: &str, role: ManagedProcessRole) -> ManagedProcessSpec {
        ManagedProcessSpec {
            name: name.to_owned(),
            role,
            run: format!("run-{name}"),
            secret_env: Default::default(),
            setup: None,
            setup_steps: Vec::new(),
            cwd: std::path::PathBuf::from("/repo"),
            service: None,
            start_after_ms: 0,
            after: Vec::new(),
            shutdown_on_exit: role == ManagedProcessRole::Lifecycle,
            run_on_host: false,
        }
    }

    fn host(name: &str) -> HostProcessTab {
        HostProcessTab {
            name: name.to_owned(),
            log: std::path::PathBuf::from(format!("/repo/.effigy/{name}.log")),
            listener_state: None,
            restart: HostProcessRestart::Never,
            restart_delay_ms: 0,
        }
    }

    /// effigy#322: a session bound to an environment shows the run's own
    /// output, the environment's host processes in declaration order, and its
    /// workloads — in both placements, since the tabs never ask where a
    /// workload runs.
    #[test]
    fn session_tabs_put_the_environments_host_processes_before_its_workloads() {
        let processes = vec![
            process("lifecycle", ManagedProcessRole::Lifecycle),
            process("api", ManagedProcessRole::Standard),
            process("terminal", ManagedProcessRole::Shell),
        ];
        let (tabs, order) = session_tabs(
            true,
            vec![host("tunnel"), host("frontend")],
            &processes,
            &[
                "lifecycle".to_owned(),
                "api".to_owned(),
                "terminal".to_owned(),
            ],
        );
        assert_eq!(
            order,
            vec![
                "lifecycle".to_owned(),
                "tunnel".to_owned(),
                "frontend".to_owned(),
                "api".to_owned(),
                "terminal".to_owned(),
            ],
            "declaration order, next to the run's own output"
        );
        assert_eq!(tabs.len(), 5);
        assert_eq!(tabs[0].source, TabSource::Output);
        assert!(matches!(tabs[1].source, TabSource::HostProcess(_)));
        assert_eq!(tabs[2].name, "frontend");
        assert!(matches!(tabs[3].source, TabSource::Workload));
        assert!(matches!(tabs[4].source, TabSource::Shell));
        assert!(
            !tabs[1].pty,
            "a host process tab renders its log, not a terminal"
        );
    }

    /// An environment-only session still opens the run's own output tab, so
    /// its environment phase is visible.
    #[test]
    fn an_environment_only_session_opens_the_run_output_tab() {
        let (tabs, order) = session_tabs(true, vec![host("api")], &[], &[]);
        assert_eq!(order, vec!["lifecycle".to_owned(), "api".to_owned()]);
        assert_eq!(tabs[0].source, TabSource::Output);
    }

    /// A host process and a workload may share a declared name without
    /// sharing a tab.
    #[test]
    fn a_host_process_tab_keeps_its_name_when_a_workload_uses_it() {
        let processes = vec![process("api", ManagedProcessRole::Standard)];
        let (tabs, order) = session_tabs(true, vec![host("api")], &processes, &["api".to_owned()]);
        assert_eq!(
            order,
            vec![
                "lifecycle".to_owned(),
                "host:api".to_owned(),
                "api".to_owned()
            ]
        );
        assert_eq!(tabs[1].name, "host:api");
        assert_eq!(tabs[2].name, "api");
    }

    /// A run that does not own an environment shows only its workloads: the
    /// environment's host processes belong to whoever runs it.
    #[test]
    fn a_session_without_its_environment_shows_no_host_process_tabs() {
        let processes = vec![process("api", ManagedProcessRole::Standard)];
        let (tabs, order) = session_tabs(false, Vec::new(), &processes, &["api".to_owned()]);
        assert_eq!(order, vec!["api".to_owned()]);
        assert_eq!(tabs.len(), 1);
        assert!(matches!(tabs[0].source, TabSource::Workload));
    }
}
