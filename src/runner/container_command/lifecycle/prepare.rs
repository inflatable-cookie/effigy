use super::report::DetachedOperationState;
use super::up::ContainerUpPlan;
use crate::runner::container_command::secret_env::{
    resolve_container_secret_runtime, secrets_required,
};
use crate::runner::container_command::steps::{ProgressSource, TimedStep};
use crate::runner::container_command::support::ensure_shared_services_observed;
use crate::runner::container_command::{secret_env, steps, RunnerError};
use effigy_containers::compose::{resolve_compose_backend_for_repo, ComposeBackend};
use effigy_containers::exec::{
    colima_advice_mode, colima_profile_warnings, ensure_runtime_backend_running_for_bringup,
    inspect_colima_ssh_agent_socket, repair_colima_ssh_agent_symlink, ssh_agent_socket_outcome,
    ContainerExecError, SshAgentSocketOutcome,
};
use effigy_containers::{
    effective_attach_mode, load_container_policy_for_creation_observed,
    validate_compose_backend_runtime, validate_container_policy, ContainerAction,
    EffectiveContainerPolicy, WorkspaceExtraMountDecision,
};
use effigy_runtime::signals::install_stop_requested_flag;
use effigy_ui::steps::ProgressFrame;
use std::path::Path;

pub(super) fn prepare_container_up(
    repo_root: &Path,
    name: Option<&str>,
    attach: bool,
    detach: bool,
    operation: &mut DetachedOperationState,
) -> Result<ContainerUpPlan, RunnerError> {
    prepare_container_up_with_loader(
        repo_root,
        name,
        attach,
        detach,
        operation,
        load_container_policy_for_creation_observed,
    )
}

type CreationPolicyLoader = fn(
    &Path,
    Option<&str>,
    &mut dyn FnMut(effigy_containers::policy::load::CreationPolicyStage),
) -> Result<
    (EffectiveContainerPolicy, Vec<WorkspaceExtraMountDecision>),
    effigy_containers::ContainerPolicyError,
>;

fn prepare_container_up_with_loader(
    repo_root: &Path,
    name: Option<&str>,
    attach: bool,
    detach: bool,
    operation: &mut DetachedOperationState,
    load: CreationPolicyLoader,
) -> Result<ContainerUpPlan, RunnerError> {
    operation.preparation_step = Some("declaration");
    let stop_flag = install_stop_requested_flag()?;
    // Resolve the attach declaration without materializing policy or probing runtime.
    let declared =
        effigy_manifest::load_task_manifest_with_inspection(&repo_root.join("effigy.toml"))
            .map_err(|_| RunnerError::task_invocation("original declaration unavailable"))?;
    let selected = name.or_else(|| {
        declared
            .manifest
            .containers
            .as_ref()
            .and_then(|c| c.default.as_deref())
    });
    let declared_detached = detach
        || (!attach
            && declared
                .manifest
                .containers
                .as_ref()
                .and_then(|c| selected.and_then(|name| c.environments.get(name)))
                .is_some_and(|c| {
                    c.startup == Some(effigy_manifest::ManifestContainerStartup::Detached)
                }));
    operation.active = declared_detached;
    operation.preparation_step = Some("policy");
    // One render per `up`: the creation load records the checkout identity,
    // resolves the declaration and its ports and loopback, prepares render
    // inputs, renders the generation once and prepares every mount source it
    // binds. Each of those can take time, so the step follows the stage the
    // load is in rather than holding one fixed label (effigy#460, Q-017).
    let preparation = ProgressSource::new(
        ProgressFrame::default()
            .label("Preparing config")
            .waiting_for("the container policy, its ports and mount sources"),
    );
    let ports_step = TimedStep::live(preparation.clone());
    let mut render_started = None;
    let mut render_duration = std::time::Duration::ZERO;
    let (policy, workspace_mount_decisions) = match load(repo_root, name, &mut |stage| {
        if let Some(started) = render_started.take() {
            render_duration = std::time::Instant::now().duration_since(started);
        }
        if matches!(stage, effigy_containers::CreationPolicyStage::Render) {
            render_started = Some(std::time::Instant::now());
        }
        preparation
            .update(|_| ProgressFrame::new(stage.description()).label(stage.concise_description()));
    }) {
        Ok(prepared) => {
            ports_step.succeed("Prepared config");
            prepared
        }
        Err(error) => {
            let message = error.to_string();
            ports_step.fail(message.lines().next().unwrap_or_default());
            return Err(error.into());
        }
    };
    operation.preparation_step = Some("rendering");
    // Rendering is immediate, so it is one finished line and never a spinner.
    effigy_ui::steps::finish_line(
        true,
        &format!(
            "Rendered stack ({})",
            effigy_ui::human::human_duration(render_duration)
        ),
    );
    operation.preparation_step = Some("validation");
    validate_container_policy(repo_root, &policy)?;
    crate::runner::host_process::validate_host_process_preflight(repo_root, &policy)?;
    validate_compose_backend_runtime(repo_root, &policy)?;
    let secret_runtime = resolve_container_secret_runtime(repo_root, &policy, secrets_required())?;
    let compose_secret_env = secret_env::compose_env(&secret_runtime);
    let mut warnings = colima_profile_warnings(&policy, repo_root);
    // A declared container key this bring-up could not deliver is reported
    // here, before the slow Compose work, and again in the bring-up's JSON
    // envelope. Required keys already failed resolution above.
    warnings.extend(secret_runtime.warnings.iter().cloned());
    emit_warning_lines(&warnings);
    let attach_mode = effective_attach_mode(&policy, attach, detach);
    operation.preparation_step = Some("runtime");
    let (colima_started, colima_bringup_guard) =
        ensure_runtime_backend_running_for_bringup(&policy, repo_root)?;
    emit_ssh_agent_socket_warning(&policy, repo_root)?;
    operation.preparation_step = Some("mounts");
    effigy_containers::policy::mounts::prepare_container_mount_sources(&policy)?;
    // A shared service is awaited by its own project, so the frame names the
    // exact services it waits on until the ensure reports one of them.
    let shared_names = policy
        .shared_services
        .iter()
        .map(|service| format!("`{}`", service.service_name))
        .collect::<Vec<_>>()
        .join(", ");
    let awaiting = if shared_names.is_empty() {
        "the shared services to be ready".to_owned()
    } else {
        format!("shared services {shared_names} to be ready")
    };
    let shared_progress = ProgressSource::new(
        ProgressFrame::default()
            .label("Starting shared services")
            .waiting_for(awaiting),
    );
    let shared_service_notes =
        steps::during(shared_progress, "Started shared services", |progress| {
            ensure_shared_services_observed(&policy, progress)
        })?;
    operation.preparation_step = Some("dispatch");
    let up_plan = effigy_runtime::container_manager::compose_up_invocation_plan(
        repo_root,
        &policy,
        ContainerAction::Activate,
        "docker compose up",
    )
    .map_err(RunnerError::from)?;
    Ok(ContainerUpPlan {
        stop_flag,
        policy,
        requested_container: name.map(str::to_owned),
        secret_runtime,
        compose_secret_env,
        warnings,
        attach_mode,
        colima_started,
        shared_service_notes,
        handover_notes: Vec::new(),
        workspace_mount_decisions,
        up_plan,
        _colima_bringup_guard: colima_bringup_guard,
    })
}

pub(super) fn prepare_sources_for_container_up(repo_root: &Path) -> Result<(), RunnerError> {
    if effigy_core::container_detection::process_is_inside_effigy_workspace_container() {
        return Ok(());
    }
    crate::runner::bundle_command::prepare_sources_at(repo_root)
}

fn emit_warning_lines(warnings: &[String]) {
    for warning in warnings {
        eprintln!("[warn] {warning}");
    }
}

/// Pre-empt the cryptic nerdctl `mkdir /run/host-services/ssh-auth.sock: file
/// exists` crash. On a long-running colima VM the forwarded host SSH-agent
/// socket can go stale (the host agent socket rotated), leaving a dangling
/// symlink that the workspace container cannot bind-mount. We check the socket
/// once colima is up and, if it is broken, warn with the exact remediation so
/// the operator fixes it instead of decoding a runtime error (g08.017).
fn emit_ssh_agent_socket_warning(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<(), RunnerError> {
    // The probe runs inside the VM via `colima ssh`; only meaningful for the
    // colima backend.
    if resolve_compose_backend_for_repo(repo_root, policy) != ComposeBackend::ColimaNerdctl {
        return Ok(());
    }
    let health = inspect_colima_ssh_agent_socket(policy, repo_root);
    if !health.is_broken() {
        return Ok(());
    }
    // A broken forwarded path is repaired in place when the guest has a live
    // agent socket. The VM is not restarted. The outcome decides whether the
    // bring-up continues, warns, or refuses.
    let repair = repair_colima_ssh_agent_symlink(&policy.profile, repo_root);
    match ssh_agent_socket_outcome(&policy.profile, health, &repair, colima_advice_mode()) {
        SshAgentSocketOutcome::Proceed { relinked } => {
            if let Some(source) = relinked {
                eprintln!(
                    "[info] Colima profile `{}`: repaired the workspace SSH-agent symlink inside the VM to {source}; the VM was not restarted",
                    policy.profile
                );
            }
            Ok(())
        }
        SshAgentSocketOutcome::Warn(message) => {
            emit_warning_lines(&[message]);
            Ok(())
        }
        SshAgentSocketOutcome::Refuse(message) => {
            Err(RunnerError::from(ContainerExecError::Failure {
                command: "container up SSH-agent preflight".to_owned(),
                code: None,
                stdout: String::new(),
                stderr: message,
            }))
        }
    }
}
