use super::report::{annotate_managed_host_listeners, detached_activation_error};
use crate::runner::container_command::closeout::finish_container_up_failure;
use crate::runner::container_command::secret_env::{
    materialize_container_secret_runtime, resolve_container_secret_runtime, secrets_required,
    ResolvedContainerSecretRuntime,
};
use crate::runner::container_command::steps::ProgressSource;
use crate::runner::container_command::support::{
    annotate_registered_gateway_routes, annotate_tcp_alias_host_notes, annotate_warning_lines,
    reconcile_primary_service_tcp_alias_hosts, wait_for_container_ready,
};
use crate::runner::container_command::{
    gateway_registration, secret_env, support, workspace_integrations, RunnerError,
};
use effigy_containers::{up_detached_report, EffectiveContainerPolicy};
use effigy_ui::steps::ProgressFrame;
use std::ffi::OsString;
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
    Workspace,
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
            // The journal adapter redacts the diagnostic and retains custody.
            Self::Workspace => error,
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
                match self.outcome {
                    IntegrationOutcomePolicy::Workspace => prepare()?,
                    _ => ensure_up_workspace_permissions(policy, prepare)?,
                }
            }
            Step::Secrets => {
                self.stage(2, "materializing secrets");
                materialize_container_secret_runtime(root, policy, secrets, binding)?;
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
                // A late listener withholds only its own route. A supervisor
                // start failure is fatal in every mode, including non-listeners.
                self.stage(7, "starting host listeners");
                let report = crate::runner::host_process::start_host_processes_in_activation(
                    root,
                    policy,
                    deadline,
                    self.publisher,
                )?;
                state.report.managed_host_listeners = report.ready;
                state.report.managed_host_listener_failures = report.failures;
            }
        }
        Ok(())
    }
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

/// Gate-free effects for the reserved workspace owner. This value owns the
/// frozen policy and all returned exposure handles; it never applies Compose,
/// opens another environment activation or runs lifecycle cleanup itself.
pub(in crate::runner) struct WorkspaceUpEffects {
    target: effigy_kernel::workspace_session::ContainerIdentity,
    policy: EffectiveContainerPolicy,
    secrets: ResolvedContainerSecretRuntime,
    runtime: RuntimeIntegrationState,
    health: Option<&'static str>,
    secrets_attempted: bool,
    routes_attempted: bool,
    waits: Option<crate::runner::managed_wait::ActivationWaits>,
}

/// Secret values remain in this process, resolved once for both apply and
/// runtime-file integration. Binding the observed target adds no new secrets.
pub(in crate::runner) struct WorkspacePreparedEffects {
    policy: EffectiveContainerPolicy,
    secrets: ResolvedContainerSecretRuntime,
}

impl WorkspacePreparedEffects {
    pub(in crate::runner) fn prepare(
        policy: EffectiveContainerPolicy,
    ) -> Result<Self, RunnerError> {
        let secrets =
            resolve_container_secret_runtime(&policy.repo_root, &policy, secrets_required())?;
        Ok(Self { policy, secrets })
    }
    pub(in crate::runner) fn compose_env(&self) -> Vec<(String, OsString)> {
        secret_env::compose_env(&self.secrets)
    }
    pub(in crate::runner) fn bind(
        self,
        target: effigy_kernel::workspace_session::ContainerIdentity,
    ) -> WorkspaceUpEffects {
        WorkspaceUpEffects {
            target,
            policy: self.policy,
            secrets: self.secrets,
            runtime: RuntimeIntegrationState::default(),
            health: None,
            secrets_attempted: false,
            routes_attempted: false,
            waits: None,
        }
    }
}

impl WorkspaceUpEffects {
    pub(in crate::runner) fn startup_report(
        &self,
    ) -> crate::runner::workspace_client::serving_host::ContainerStartupReport {
        let mut report = up_detached_report(&self.policy, false, self.health);
        annotate_registered_gateway_routes(&mut report, &self.runtime.report.gateway_routes);
        annotate_managed_host_listeners(&mut report, &self.runtime.report.managed_host_listeners);
        annotate_tcp_alias_host_notes(&mut report, &self.runtime.report.tcp_alias_host_notes);
        annotate_warning_lines(&mut report, &self.secrets.warnings);
        crate::runner::workspace_client::serving_host::ContainerStartupReport {
            json: report.json,
            text: report.success_text,
        }
    }

    fn binding(
        &self,
    ) -> Result<&effigy_containers::exec::VerifiedComposeContainer, effigy_kernel::run::RunError>
    {
        self.runtime.binding.as_ref().ok_or_else(|| {
            effigy_kernel::run::RunError::refused(
                "workspace integrations have no verified primary binding",
            )
        })
    }
}

impl crate::runner::workspace_client::container_up::integrations::IntegrationEffects
    for WorkspaceUpEffects
{
    fn reconcile(
        &mut self,
        step: workspace_integrations::WorkspaceIntegrationStep,
        deadline: std::time::Instant,
    ) -> Result<(), effigy_kernel::run::RunError> {
        use effigy_kernel::run::RunError;
        use workspace_integrations::WorkspaceIntegrationStep as Step;
        let failed = |error: RunnerError| {
            #[cfg(feature = "workspace-serving-fixture")]
            crate::runner::workspace_client::container_up::fixture_diagnostic(
                "integration effect failed",
            );
            let _ = error;
            RunError::unavailable("workspace integration effect failed")
        };
        if std::time::Instant::now() >= deadline {
            return Err(RunError::unavailable("integration budget exhausted"));
        }
        if step != Step::Readiness {
            self.binding()?
                .reverify(&self.policy.repo_root, &self.policy, deadline)
                .map_err(|_| RunError::unavailable("workspace integration target changed"))?;
        }
        let root = &self.policy.repo_root;
        if step == Step::Readiness {
            let waits = crate::runner::managed_wait::ActivationWaits::begin(
                root,
                &self.policy.name,
                crate::runner::environment::requested_holder(),
            )
            .map_err(failed)?;
            let mut graph = crate::runner::host_process::HostWaitGraph::for_environment_phase(
                &self.policy,
                waits.publisher(),
                std::time::Instant::now(),
            );
            let mut observe_phase = |state: &effigy_core::managed_waits::EntryState| {
                graph.observe_environment_phase(state, std::time::Instant::now());
            };
            self.health = wait_for_container_ready(
                &self.policy,
                None,
                crate::runner::container_command::readiness::ObservationBudget::Until(deadline),
                Some(&mut observe_phase),
            )
            .map_err(failed)?;
            graph.environment_ready(std::time::Instant::now());
            self.waits = Some(waits);
            if self.health == Some("interrupted") {
                return Err(RunError::refused("workspace readiness interrupted"));
            }
        }
        if step == Step::Secrets {
            self.secrets_attempted = !self.secrets.env.is_empty()
                && self.secrets.delivery
                    == effigy_manifest::ManifestContainerSecretDelivery::RuntimeFiles;
        }
        if step == Step::Routes {
            self.routes_attempted =
                crate::runner::container_runtime_prep::container_policy_uses_gateway_surface(
                    &self.policy,
                );
        }
        let waits = self.waits.as_mut().ok_or_else(|| {
            RunError::unavailable("workspace activation observations are unavailable")
        })?;
        RuntimeIntegrationContext {
            root,
            policy: &self.policy,
            secrets: &self.secrets,
            container_name: Some(&self.policy.name),
            outcome: IntegrationOutcomePolicy::Workspace,
            progress: None,
            publisher: waits.publisher(),
        }
        .reconcile(&mut self.runtime, step, Some(deadline))
        .map_err(failed)?;
        if step == Step::HostIntegrations {
            let complete = self
                .runtime
                .report
                .managed_host_listener_failures
                .is_empty();
            waits.finish(complete).map_err(failed)?;
            if !complete {
                return Err(RunError::unavailable(
                    "workspace host listener exposure is incomplete",
                ));
            }
        }
        if step == Step::Readiness && self.binding()?.id() != self.target.container_id {
            return Err(RunError::refused(
                "workspace integration primary target changed",
            ));
        }

        Ok(())
    }

    fn unwind(
        &mut self,
        step: workspace_integrations::WorkspaceIntegrationStep,
        cleanup_deadline: std::time::Instant,
    ) -> Result<(), effigy_kernel::run::RunError> {
        use effigy_kernel::run::RunError;
        use workspace_integrations::WorkspaceIntegrationStep as Step;
        if std::time::Instant::now() >= cleanup_deadline {
            return Err(RunError::unavailable(
                "workspace integration cleanup budget exhausted",
            ));
        }
        // Readiness and undeclared permission/host integrations change nothing.
        // A journalled step alone does not prove its helper mutated anything.
        // For attempted mutations the ordinary helpers do not yet
        // supply exact-effect absence evidence: retain handles/journal instead
        // of erasing another activation's route, process or secret files.
        let absent = match step {
            Step::Readiness => true,
            Step::Permissions => self.policy.workspace_user.is_none(),
            Step::Secrets => !self.secrets_attempted,
            Step::Routes => !self.routes_attempted,
            Step::HostIntegrations => {
                self.policy.host_processes.is_empty()
                    && !crate::runner::container_runtime_prep::container_policy_uses_gateway_surface(
                        &self.policy,
                    )
            }
        };
        absent
            .then_some(())
            .ok_or_else(|| RunError::unavailable("integration effects unsettled; re-run up"))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{shared_service, test_policy};
    use super::*;
    #[test]
    fn workspace_integration_cleanup_distinguishes_undeclared_effects_from_mutations() {
        use crate::runner::container_command::secret_env::ResolvedContainerSecretRuntime;
        use crate::runner::container_command::workspace_integrations::WorkspaceIntegrationStep as Step;
        use crate::runner::workspace_client::container_up::integrations::IntegrationEffects;
        use std::time::{Duration, Instant};
        let mut effects = WorkspacePreparedEffects {
            policy: test_policy(Vec::new()),
            secrets: ResolvedContainerSecretRuntime {
                delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
                env: Vec::new(),
                warnings: Vec::new(),
            },
        }
        .bind(effigy_kernel::workspace_session::ContainerIdentity {
            container_id: "a".repeat(64),
            vm_boot_id: "boot".into(),
            backend: "docker-compose".into(),
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        for step in [
            Step::Readiness,
            Step::Permissions,
            Step::Secrets,
            Step::Routes,
            Step::HostIntegrations,
        ] {
            effects
                .unwind(step, deadline)
                .expect("undeclared effects have no mutation to withdraw");
            assert!(effects.unwind(step, Instant::now()).is_err());
        }
        effects.policy.workspace_user = Some("nobody".into());
        assert!(effects.unwind(Step::Permissions, deadline).is_err());
        effects
            .policy
            .shared_services
            .push(shared_service("db", "postgres", 5432));
        assert!(effects.unwind(Step::HostIntegrations, deadline).is_err());
        effects.secrets_attempted = true;
        effects.routes_attempted = true;
        assert!(effects.unwind(Step::Secrets, deadline).is_err());
        assert!(effects.unwind(Step::Routes, deadline).is_err());
    }
}
