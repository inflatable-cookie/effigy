use super::integrations::ContainerUpRuntimeIntegrations;
use super::up::ContainerUpPlan;
use crate::runner::container_command::support::{
    annotate_registered_gateway_routes, annotate_shared_service_notes,
    annotate_tcp_alias_host_notes, annotate_warning_lines,
};
use crate::runner::container_command::{
    gateway_registration, render_container_report, runtime_error_from_runner, support, RunnerError,
};
use crate::runner::environment;
use effigy_containers::environment::{Operation, Retention};
use effigy_containers::{
    annotate_workspace_extra_mount_decisions, up_detached_report,
    workspace_extra_mount_decision_lines, ContainerCommandReport, EffectiveAttachMode,
    EffectiveContainerPolicy,
};
use effigy_runtime::session::run_attached_container_session_with_hook;
use std::path::Path;

#[derive(Default)]
pub(super) struct DetachedOperationState {
    pub(super) active: bool,
    pub(super) stage: Option<&'static str>,
    pub(super) preparation_step: Option<&'static str>,
    /// Workspace preparation diagnostics must not echo backend/secret values.
    pub(super) workspace_mode: bool,
    /// The environment and checkout the activation was asked to bring up.
    pub(super) identity: Option<String>,
}

/// The exact activation identity a detached JSON failure reports, so an
/// operator can place the refusal without reading the human message.
pub(super) fn detached_activation_identity(policy: &EffectiveContainerPolicy) -> String {
    format!(
        "environment `{}` at {}",
        policy.name,
        policy.repo_root.display()
    )
}

/// The underlying refusal, bounded and safe for machine output.
///
/// The detached JSON envelope used to replace the real cause with generic
/// advice, which hid which admission or claim refused. This keeps the cause
/// through the one redactor and the one head-plus-tail bound in
/// `effigy_ui::human`, so a backend's last line survives (effigy#364).
pub(super) fn sanitized_failure_cause(error: &RunnerError) -> String {
    let cause = error
        .to_string()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    effigy_ui::human::bound_text(&cause, MAX_CAUSE_BYTES)
}

/// Bytes of a refusal detail the detached JSON envelope keeps.
const MAX_CAUSE_BYTES: usize = 1200;

/// Render one failed detached activation.
///
/// The JSON envelope keeps the underlying refusal as `cause` plus the stage
/// and the environment/checkout identity, so an operator can act on why the
/// activation failed.
pub(super) fn detached_activation_failure(
    operation: &DetachedOperationState,
    output_json: bool,
    error: RunnerError,
) -> RunnerError {
    // Partial exposure is not an unverified activation: the environment
    // settled, and the report names exactly which routes are published and
    // which listener withheld its own. It gets its own envelope so a failing
    // host listener never reads as a whole-activation exposure failure
    // (effigy#455).
    if let RunnerError::ManagedListenerExposure {
        environment,
        exposed,
        failures,
    } = &error
    {
        let withheld = failures
            .iter()
            .map(|failure| {
                serde_json::json!({
                    "kind": "host_listener",
                    "listener": failure.name,
                    "route_domain": failure.route_domain,
                    "declared_route_domain": failure.declared_route_domain,
                    "status": failure.status,
                    "observed_status": failure.observed_status,
                    "detail": failure.detail,
                    "output_tail": failure.output_tail,
                    "log": failure.log,
                    "recovery": format!("effigy container {} restart {}", environment, failure.name),
                })
            })
            .collect::<Vec<_>>();
        let message = format!(
            "{error}\nDetached activation settled the environment with partial exposure: {} route(s) published, {} listener(s) withheld.\nnext: `effigy container up --detach` retries the withheld routes; `effigy container down` stops the scope.",
            exposed.len(),
            failures.len()
        );
        if output_json {
            return RunnerError::CommandJsonFailure {
                rendered: serde_json::json!({
                    "state": "partial",
                    "phase": "listener_exposure",
                    "activation_stage": operation.stage.unwrap_or("runtime_integrations"),
                    "identity": operation.identity,
                    "environment": environment,
                    "failure_kind": "managed_listener_exposure",
                    "readiness": "verified",
                    "exposure": "partial",
                    "exposed_routes": exposed,
                    "withheld_routes": withheld,
                    "recovery": "effigy container up --detach",
                    "stop": "effigy container down",
                    "message": message,
                })
                .to_string(),
            };
        }
        return RunnerError::task_invocation(message);
    }
    let message = match &error {
        RunnerError::ManagedHostReadiness { diagnostic, .. } => {
            format!("Detached activation did not complete: {diagnostic}")
        }
        _ => "Detached activation did not complete: readiness and exposure are unverified. Re-run `effigy container up --detach`; the next activation reconciles from what is running.".to_owned(),
    };
    if output_json {
        let failure_kind = match &error {
            // The store's own typed answer decides the kind; the message is
            // never read for it (effigy#933).
            RunnerError::GatewayEndpointStore {
                kind: effigy_gateway::EndpointStoreRefusal::ObservationUnknown,
                ..
            } => "endpoint_observation_unknown",
            RunnerError::GatewayEndpointStore { .. } => "endpoint_store_refusal",
            RunnerError::TaskInvocation(_) => "invocation_refusal",
            RunnerError::ManagedHostReadiness { .. } => "managed_readiness",
            RunnerError::GatewayRouteTable { .. }
            | RunnerError::GatewayRouteRegistration { .. }
            | RunnerError::GatewayRouteShape { .. }
            | RunnerError::GatewayLoopback { .. }
            | RunnerError::GatewayRuntimeTarget { .. } => "gateway_route_refusal",
            _ => "other",
        };
        let mut rendered = serde_json::json!({
                "state": "unavailable",
                "phase": "detached_activation",
                "activation_stage": operation.stage.unwrap_or("preparation"),
                "preparation_step": operation.preparation_step.filter(|_| operation.stage == Some("preparation")),
                "identity": operation.identity,
                "failure_kind": failure_kind,
                "cause": sanitized_failure_cause(&error),
                "readiness": "unverified",
                "exposure": "unverified",
                "recovery": "effigy container up --detach",
                "stop": "effigy container down",
                "retirement_removes_mutable_data": true,
                "message": message,
        });
        // A wait the bring-up could not finish names its own kind and waits.
        if let Some(details) = wait_refusal_details(&error) {
            rendered["failure_kind"] = details["failure_kind"].clone();
            rendered["waiting_on"] = details["waiting_on"].clone();
        }
        return RunnerError::CommandJsonFailure {
            rendered: rendered.to_string(),
        };
    }
    RunnerError::task_invocation(format!(
        "{error}\ndetached activation unavailable: readiness and exposure are unverified. Inspect with `effigy container status`; retry `effigy container up --detach`. `effigy container down` stops the scope; `effigy container retire --yes` removes owned mutable data."
    ))
}

/// The machine-readable details of a container wait that ended unfinished.
fn wait_refusal_details(error: &RunnerError) -> Option<serde_json::Value> {
    let RunnerError::ContainerWaitRefused { details_json, .. } = error else {
        return None;
    };
    serde_json::from_str::<serde_json::Value>(details_json).ok()
}

pub(super) fn detached_activation_error(
    policy: &EffectiveContainerPolicy,
    error: RunnerError,
) -> RunnerError {
    // Keep the typed, bounded readiness evidence intact across this seam.
    if matches!(&error, RunnerError::ManagedHostReadiness { .. }) {
        return error;
    }
    RunnerError::task_invocation(format!(
        "{error}\ncontainer `{}` stays as Compose left it: gateway exposure is unverified and no teardown ran. Re-run `effigy container up --detach` to reconcile from what is running, or `effigy container down` to stop it.",
        policy.name
    ))
}

/// The routes this activation verified and published, as one line each, for
/// the partial-exposure report.
fn exposed_route_lines(integrations: &ContainerUpRuntimeIntegrations) -> Vec<String> {
    let mut exposed = integrations
        .gateway_routes
        .iter()
        .map(|route| match route.target.as_deref() {
            Some(target) => format!("{} -> {target}", route.domain),
            None => format!("{} (registered)", route.domain),
        })
        .collect::<Vec<_>>();
    for listener in &integrations.managed_host_listeners {
        exposed.push(format!(
            "{} -> {}",
            listener.route_domain, listener.public_url
        ));
    }
    exposed
}

/// A settled environment whose exposure is partial: the container's verified
/// routes and every ready listener are published, and each failed listener
/// kept only its own route withheld.
pub(super) fn listener_exposure_failure(
    policy: &EffectiveContainerPolicy,
    integrations: &ContainerUpRuntimeIntegrations,
) -> RunnerError {
    RunnerError::ManagedListenerExposure {
        environment: policy.name.clone(),
        exposed: exposed_route_lines(integrations),
        failures: integrations.managed_host_listener_failures.clone(),
    }
}

pub(super) fn render_container_up_result(
    repo_root: &Path,
    plan: &ContainerUpPlan,
    health: Option<&'static str>,
    integrations: ContainerUpRuntimeIntegrations,
    creation: Option<&effigy_containers::environment::CreationOutcome>,
    output_json: bool,
) -> Result<String, RunnerError> {
    if plan.attach_mode == EffectiveAttachMode::Detached {
        let mut report = detached_container_up_report(plan, health, integrations);
        annotate_environment_record(&mut report, &plan.policy, creation);
        return Ok(render_container_report(report, output_json));
    }
    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container up --json` is only supported for detached bring-up; attached sessions stream live output instead",
        ));
    }
    let mut session = run_attached_container_session_with_hook(
        repo_root,
        &plan.policy,
        plan.colima_started,
        health,
        None,
        |policy| {
            gateway_registration::deregister_gateway_routes_for_container(policy)
                .map_err(runtime_error_from_runner)
        },
        // The session's configured shutdown is one stop transition: it
        // refuses while other runs hold the environment.
        environment::stop_hook(Operation::Down),
    )
    .map_err(RunnerError::from)?;
    for line in workspace_extra_mount_decision_lines(&plan.workspace_mount_decisions) {
        session.push('\n');
        session.push_str(&line);
    }
    Ok(session)
}

fn detached_container_up_report(
    plan: &ContainerUpPlan,
    health: Option<&'static str>,
    integrations: ContainerUpRuntimeIntegrations,
) -> ContainerCommandReport {
    let mut report = up_detached_report(&plan.policy, plan.colima_started, health);
    annotate_shared_service_notes(&mut report, &plan.shared_service_notes);
    support::annotate_data_handover_notes(&mut report, &plan.handover_notes);
    annotate_registered_gateway_routes(&mut report, &integrations.gateway_routes);
    annotate_managed_host_listeners(&mut report, &integrations.managed_host_listeners);
    annotate_tcp_alias_host_notes(&mut report, &integrations.tcp_alias_host_notes);
    annotate_warning_lines(&mut report, &integrations.warnings);
    annotate_workspace_extra_mount_decisions(&mut report, &plan.workspace_mount_decisions);
    support::annotate_source_update_notes(&mut report);
    report
}

/// The environment record's view of this bring-up, in text and JSON.
fn annotate_environment_record(
    report: &mut ContainerCommandReport,
    policy: &EffectiveContainerPolicy,
    creation: Option<&effigy_containers::environment::CreationOutcome>,
) {
    let record = environment::record(policy);
    let retention = record
        .as_ref()
        .and_then(|record| record.engine().map(|engine| engine.retention));
    let backend = environment::backend(&policy.repo_root, policy);
    if let Some(json) = report.json.as_object_mut() {
        let mut environment = serde_json::json!({
            "retention": retention.map(|retention| match retention {
                Retention::Persistent => "persistent",
                Retention::Held => "held",
            }),
            "backend": backend.label(),
            "backend_reason": backend.reason.describe(),
        });
        if let Some(creation) = creation {
            let object = environment.as_object_mut().expect("object");
            match creation.unproven() {
                // Only a complete gate-scoped inference reports created IDs.
                // Ordinary unsealed applies and uncertain observations say why
                // they provide no workspace attribution (effigy#510).
                Some(reason) => {
                    object.insert(
                        "creation_unproven".to_owned(),
                        serde_json::Value::String(reason.to_owned()),
                    );
                }
                None => {
                    if let Some(receipt) = creation.receipt() {
                        object.insert(
                            "creation_image_identities".to_owned(),
                            serde_json::json!(receipt.image_identities()),
                        );
                    }
                    object.insert(
                        "created_container_ids".to_owned(),
                        serde_json::Value::Array(
                            creation
                                .created_ids()
                                .into_iter()
                                .map(serde_json::Value::String)
                                .collect(),
                        ),
                    );
                }
            }
        }
        json.insert("environment".to_owned(), environment);
    }
}

pub(super) fn annotate_managed_host_listeners(
    report: &mut ContainerCommandReport,
    listeners: &[crate::runner::host_process::ManagedHostListenerResult],
) {
    if listeners.is_empty() {
        return;
    }
    if let Some(json) = report.json.as_object_mut() {
        json.insert(
            "managed_host_listeners".to_owned(),
            serde_json::to_value(listeners).expect("managed listener result serializes"),
        );
    }
    for listener in listeners {
        report.success_text.push('\n');
        report.success_text.push_str(&format!(
            "[host listener] {} ready at {} -> {}",
            listener.name, listener.address, listener.public_url
        ));
    }
}
