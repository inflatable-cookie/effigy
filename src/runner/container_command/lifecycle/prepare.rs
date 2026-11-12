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
    colima_profile_warnings, ensure_runtime_backend_running, inspect_colima_ssh_agent_socket,
    SshAgentSocketHealth,
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
            ports_step.fail(if operation.workspace_mode {
                "workspace ordinary policy preparation failed".to_owned()
            } else {
                error.to_string()
            });
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
    #[cfg(feature = "workspace-private-qualification")]
    let colima_started = if operation.workspace_mode {
        effigy_containers::exec::require_qualification_backend_running(&policy, repo_root)?
    } else {
        ensure_runtime_backend_running(&policy, repo_root)?
    };
    #[cfg(not(feature = "workspace-private-qualification"))]
    let colima_started = ensure_runtime_backend_running(&policy, repo_root)?;
    emit_ssh_agent_socket_warning(&policy, repo_root);
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
fn emit_ssh_agent_socket_warning(policy: &EffectiveContainerPolicy, repo_root: &Path) {
    // The probe runs inside the VM via `colima ssh`; only meaningful for the
    // colima backend.
    if resolve_compose_backend_for_repo(repo_root, policy) != ComposeBackend::ColimaNerdctl {
        return;
    }
    let health = inspect_colima_ssh_agent_socket(policy, repo_root);
    if health.is_broken() {
        let detail = match health {
            SshAgentSocketHealth::Stale => "is stale (the host SSH-agent socket rotated)",
            SshAgentSocketHealth::Absent => "is not set up",
            _ => "is unavailable",
        };
        emit_warning_lines(&[format!(
            "workspace SSH-agent forwarding {detail} on colima profile `{profile}`. The \
             workspace container can fail to start with `mkdir /run/host-services/ssh-auth.sock: \
             file exists`. Fix: run `colima restart {profile}`, then re-run this command.",
            profile = policy.profile,
        )]);
    }
}

/// Only the authenticated private preparation child calls this body. Its
/// stdout/stderr already belong to the parent's qualified Discard custody.
#[cfg(feature = "workspace-private-qualification")]
pub(in crate::runner::container_command) fn qualification_prepare_body(
    root: &Path,
    name: &str,
) -> Result<(), RunnerError> {
    let mut operation = DetachedOperationState {
        workspace_mode: true,
        ..Default::default()
    };
    prepare_container_up_with_loader(
        root,
        Some(name),
        false,
        true,
        &mut operation,
        effigy_containers::load_container_policy_for_qualification_inputs,
    )
    .map(|_| ())
}
