use super::report::detached_activation_error;
use crate::runner::container_command::closeout::finish_container_up_failure;
use crate::runner::container_command::secret_env::{
    materialize_container_secret_runtime, ResolvedContainerSecretRuntime,
};
use crate::runner::container_command::steps::ProgressSource;
use crate::runner::container_command::support::reconcile_primary_service_tcp_alias_hosts;
use crate::runner::container_command::{
    gateway_registration, support, workspace_integrations, RunnerError,
};
use effigy_containers::{EffectiveAttachMode, EffectiveContainerPolicy};
use effigy_ui::steps::ProgressFrame;
use std::path::Path;

#[derive(Default)]
pub(super) struct ContainerUpRuntimeIntegrations {
    pub(super) gateway_routes: Vec<gateway_registration::RegisteredGatewayRoute>,
    pub(super) managed_host_listeners: Vec<crate::runner::host_process::ManagedHostListenerResult>,
    /// Listeners that kept their own route withheld. Every other verified
    /// route stays published, so this list is what exposure reports (effigy#455).
    pub(super) managed_host_listener_failures:
        Vec<crate::runner::host_process::ManagedHostListenerFailure>,
    pub(super) tcp_alias_host_notes: Vec<String>,
    pub(super) warnings: Vec<String>,
}

/// The integration stages `container up` runs after readiness, in order. The
/// frame counts the stages finished before the one now running.
const INTEGRATION_STAGES: u64 = 8;

pub(super) fn integration_stage(progress: &ProgressSource, finished: u64, stage: &str) {
    // A new stage is a new wait: an earlier stage's blocker must not stay on
    // screen for work it no longer describes (effigy#460, Q-017).
    if effigy_ui::steps::in_managed_pane() {
        eprintln!("Started: {stage} ({finished}/{INTEGRATION_STAGES})");
    }
    progress.update(|_| ProgressFrame::new(stage).count(Some((finished, INTEGRATION_STAGES))));
}

/// The route stage while the declared gateway service ports are polled: the
/// runtime read in flight, then the blocker that read reported — naming the
/// service and domain when the detail knows them — with the stage count and
/// elapsed time (effigy#460, Q-017).
pub(super) fn gateway_port_wait_stage(
    progress: &ProgressSource,
    wait: gateway_registration::GatewayPortWait<'_>,
) {
    use gateway_registration::GatewayPortWait;
    let label = match &wait {
        GatewayPortWait::Reading { .. } => "Checking gateway ports",
        GatewayPortWait::Blocked { .. } => "Waiting for gateway ports",
        GatewayPortWait::ConfirmingOwnership => "Checking container ownership",
        GatewayPortWait::ResolvingRoutes => "Resolving gateway routes",
        GatewayPortWait::ReconcilingRoutes => "Saving gateway routes",
        GatewayPortWait::ReadingExitedContainers => "Checking exited containers",
    };
    let count = Some((5, INTEGRATION_STAGES));
    progress.update(|previous| {
    let frame = match wait {
        // The next poll keeps the last blocker as its event: that is what the
        // previous read observed about this same wait.
        GatewayPortWait::Reading { attempt } => ProgressFrame::new(format!(
                "reading the runtime's containers for the declared gateway service ports (attempt {attempt})"
            )).count(count).observations_from(previous),
        GatewayPortWait::Blocked { attempt, detail } => ProgressFrame::new(format!("waiting for the declared gateway service ports (attempt {attempt})")).count(count).event(Some(detail.to_owned())),
        // The work after the poll is a different wait, so the poll's blocker is
        // cleared rather than shown against it.
        GatewayPortWait::ConfirmingOwnership => {
            ProgressFrame::new("confirming the primary container still owns its binding").count(count)
        }
        GatewayPortWait::ResolvingRoutes => {
            ProgressFrame::new("resolving the declared gateway routes against the published ports").count(count)
        }
        GatewayPortWait::ReconcilingRoutes => {
            ProgressFrame::new("writing the gateway route table").count(count)
        }
        GatewayPortWait::ReadingExitedContainers => {
            ProgressFrame::new("reading the exited gateway service containers for the timeout diagnostic").count(count)
        }
    };
    frame.label(label)
    });
}

/// The effects are shared; their owner decides what a failure leaves behind.
#[derive(Clone, Copy)]
pub(super) enum IntegrationOutcomePolicy {
    Attached,
    Detached,
}

impl IntegrationOutcomePolicy {
    pub(super) fn failure(
        self,
        policy: &EffectiveContainerPolicy,
        error: RunnerError,
        cleanup: impl FnOnce() -> Result<(), RunnerError>,
    ) -> RunnerError {
        match self {
            Self::Attached => finish_container_up_failure(error, cleanup()),
            Self::Detached => detached_activation_error(policy, error),
        }
    }
}

#[derive(Default)]
pub(super) struct RuntimeIntegrationState {
    pub(super) binding: Option<effigy_containers::exec::VerifiedComposeContainer>,
    pub(super) report: ContainerUpRuntimeIntegrations,
}

pub(super) struct RuntimeIntegrationContext<'a> {
    pub(super) root: &'a Path,
    pub(super) policy: &'a EffectiveContainerPolicy,
    pub(super) secrets: &'a ResolvedContainerSecretRuntime,
    pub(super) container_name: Option<&'a str>,
    pub(super) outcome: IntegrationOutcomePolicy,
    pub(super) progress: Option<&'a ProgressSource>,
    pub(super) publisher: &'a crate::runner::managed_wait::WaitPublisher,
}

impl RuntimeIntegrationContext<'_> {
    fn stage(&self, finished: u64, label: &str) {
        if let Some(progress) = self.progress {
            integration_stage(progress, finished, label);
        }
    }

    pub(super) fn reconcile(
        &self,
        state: &mut RuntimeIntegrationState,
        step: workspace_integrations::WorkspaceIntegrationStep,
        deadline: Option<std::time::Instant>,
    ) -> Result<(), RunnerError> {
        use workspace_integrations::WorkspaceIntegrationStep as Step;
        let Self {
            root,
            policy,
            secrets,
            ..
        } = *self;
        if step == Step::Readiness {
            // Readiness has already decided Running and health. Binding shares
            // its observation budget rather than asking a fresh question.
            self.stage(0, "reading the primary service binding");
            state.binding = Some(
                crate::runner::container_runtime_prep::ensure_primary_service_binding(
                    root, policy, deadline,
                )?,
            );
            return Ok(());
        }
        let binding = state.binding.as_ref().ok_or_else(|| {
            RunnerError::task_invocation("runtime integrations have no verified primary binding")
        })?;
        match step {
            Step::Readiness => unreachable!("binding was handled above"),
            Step::Permissions => {
                // Repair disposable cache/build volume ownership before secrets
                // or exposure, on first creation, reuse and handover alike.
                self.stage(1, "preparing workspace permissions");
                let prepare = || {
                    crate::runner::workspace_command::ensure_workspace_permissions_ready(
                        root,
                        policy,
                        self.container_name,
                        None,
                    )
                };
                ensure_up_workspace_permissions(policy, prepare)?;
            }
            Step::Secrets => {
                self.stage(2, "materializing secrets");
                let delivered =
                    materialize_container_secret_runtime(root, policy, secrets, binding)?;
                // A service this bring-up started read nothing yet: the
                // delivery names the keys it would have to restart for.
                for warning in delivered {
                    eprintln!("[warn] {warning}");
                }
            }
            Step::Routes => {
                self.stage(3, "checking pinned host ports");
                support::ensure_pinned_host_ports_available(root, policy, deadline)?;
                self.stage(4, "starting the gateway");
                gateway_registration::ensure_gateway_running_for_container(policy)?;
                self.stage(5, "registering gateway routes");
                let mut observe = |wait: gateway_registration::GatewayPortWait<'_>| {
                    if let Some(progress) = self.progress {
                        gateway_port_wait_stage(progress, wait);
                    }
                };
                state.report.gateway_routes =
                    gateway_registration::register_gateway_routes_for_verified_container_observed(
                        root,
                        policy,
                        Some(binding),
                        &mut observe,
                    )?;
            }
            Step::HostIntegrations => {
                self.stage(6, "reconciling tcp alias hosts");
                state.report.tcp_alias_host_notes =
                    reconcile_primary_service_tcp_alias_hosts(root, policy, binding, deadline)?;
                // Host startup has its own lifecycle: a live child may still
                // be building when the container observation window ends.
                // Keep custody until verified readiness or actual failure;
                // the supervisor starts its timeout when the port first opens.
                // A failed listener still settles before reporting its route
                // withheld. Supervisor start failures remain fatal in all modes.
                self.stage(7, "starting host listeners");
                let report = crate::runner::host_process::start_host_processes_in_activation(
                    root,
                    policy,
                    None,
                    self.publisher,
                )?;
                state.report.managed_host_listeners = report.ready;
                state.report.managed_host_listener_failures = report.failures;
            }
        }
        Ok(())
    }
}

/// The bring-up's last integration, once it has succeeded: the stack-owned
/// file-event relay matches the environment now running. Only a detached
/// `fs_events = "on"` environment starts one; an unstartable relay is a
/// warning, never a failed bring-up.
pub(super) fn reconcile_stack_fs_relay(
    policy: &EffectiveContainerPolicy,
    root: &Path,
    attach_mode: EffectiveAttachMode,
    report: &mut ContainerUpRuntimeIntegrations,
) {
    report
        .warnings
        .extend(crate::runner::fs_relay::reconcile_stack_relay(
            root,
            policy,
            attach_mode,
        ));
}

/// Run workspace permission preparation as a `container up` integration
/// stage, after exec readiness is verified and before secrets, gateway
/// exposure or readiness reporting.
///
/// The injectable `prepare` keeps the failure mapping unit-testable without
/// a runtime; production passes the same workspace permission backend task
/// activation uses, so owned-disposable cache volumes are writable by the
/// workspace user on first creation, reuse and handover alike.
pub(super) fn ensure_up_workspace_permissions(
    policy: &EffectiveContainerPolicy,
    prepare: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    prepare().map_err(|error| {
        RunnerError::task_invocation(format!(
            "container `{}` workspace permission preparation failed: {error}",
            policy.name
        ))
    })
}
