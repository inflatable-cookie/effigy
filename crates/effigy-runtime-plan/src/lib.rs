use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeActivationRequest {
    pub repo_root: PathBuf,
    pub policy_name: String,
    pub container_name: Option<String>,
    pub repo_override: Option<PathBuf>,
    pub lease_policy: RuntimeLeasePolicy,
}

impl RuntimeActivationRequest {
    pub fn new(repo_root: PathBuf, policy_name: impl Into<String>) -> Self {
        Self {
            repo_root,
            policy_name: policy_name.into(),
            container_name: None,
            repo_override: None,
            lease_policy: RuntimeLeasePolicy::Skip,
        }
    }

    pub fn container_name(mut self, container_name: impl Into<String>) -> Self {
        self.container_name = Some(container_name.into());
        self
    }

    pub fn repo_override(mut self, repo_override: PathBuf) -> Self {
        self.repo_override = Some(repo_override);
        self
    }

    pub fn lease_policy(mut self, lease_policy: RuntimeLeasePolicy) -> Self {
        self.lease_policy = lease_policy;
        self
    }

    pub fn plan(self) -> RuntimeActivationPlan {
        RuntimeActivationPlan::from_request(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeActivationPlan {
    pub request: RuntimeActivationRequest,
    pub route: RuntimeActivationRoute,
    pub readiness: RuntimeReadinessPlan,
    pub aliases: RuntimeAliasPlan,
    pub lease: RuntimeLeasePlan,
    pub stages: Vec<RuntimeActivationStage>,
}

impl RuntimeActivationPlan {
    pub fn from_request(request: RuntimeActivationRequest) -> Self {
        let lease = RuntimeLeasePlan {
            policy: request.lease_policy,
        };
        let stages = vec![
            RuntimeActivationStage::ValidatePolicy,
            RuntimeActivationStage::ValidateBackend,
            RuntimeActivationStage::CheckRunningState,
            RuntimeActivationStage::EnsureRunning,
            RuntimeActivationStage::PrepareMounts,
            RuntimeActivationStage::ComposeUp,
            RuntimeActivationStage::ExecReadiness,
            RuntimeActivationStage::GatewayReadiness,
            RuntimeActivationStage::AliasReconciliation,
            RuntimeActivationStage::LeaseRefresh,
        ];

        Self {
            request,
            route: RuntimeActivationRoute::Task,
            readiness: RuntimeReadinessPlan {
                probe_primary_service_exec: true,
                restart_on_failed_probe: true,
            },
            aliases: RuntimeAliasPlan {
                reconcile_primary_service_aliases: true,
                register_gateway_routes: true,
            },
            lease,
            stages,
        }
    }

    pub fn report(
        self,
        system_was_running: bool,
        cleanup_result: RuntimeCleanupResult,
    ) -> RuntimeActivationReport {
        RuntimeActivationReport {
            repo_root: self.request.repo_root,
            policy_name: self.request.policy_name,
            container_name: self.request.container_name,
            route: self.route,
            stages: self.stages,
            system_was_running,
            lease_policy: self.lease.policy,
            cleanup_result,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeActivationRoute {
    Task,
    Exec,
    Workspace,
    Bootstrap,
    Rhai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeActivationStage {
    ValidatePolicy,
    ValidateBackend,
    CheckRunningState,
    EnsureRunning,
    PrepareMounts,
    ComposeUp,
    ExecReadiness,
    GatewayReadiness,
    AliasReconciliation,
    LeaseRefresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeReadinessPlan {
    pub probe_primary_service_exec: bool,
    pub restart_on_failed_probe: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeAliasPlan {
    pub reconcile_primary_service_aliases: bool,
    pub register_gateway_routes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeLeasePlan {
    pub policy: RuntimeLeasePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeLeasePolicy {
    Skip,
    RefreshOnActivation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCleanupResult {
    NotRequired,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeActivationReport {
    pub repo_root: PathBuf,
    pub policy_name: String,
    pub container_name: Option<String>,
    pub route: RuntimeActivationRoute,
    pub stages: Vec<RuntimeActivationStage>,
    pub system_was_running: bool,
    pub lease_policy: RuntimeLeasePolicy,
    pub cleanup_result: RuntimeCleanupResult,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_plan_has_stable_stage_order() {
        let plan = RuntimeActivationRequest::new(PathBuf::from("/tmp/repo"), "web")
            .container_name("web")
            .lease_policy(RuntimeLeasePolicy::RefreshOnActivation)
            .plan();

        assert_eq!(
            plan.stages,
            vec![
                RuntimeActivationStage::ValidatePolicy,
                RuntimeActivationStage::ValidateBackend,
                RuntimeActivationStage::CheckRunningState,
                RuntimeActivationStage::EnsureRunning,
                RuntimeActivationStage::PrepareMounts,
                RuntimeActivationStage::ComposeUp,
                RuntimeActivationStage::ExecReadiness,
                RuntimeActivationStage::GatewayReadiness,
                RuntimeActivationStage::AliasReconciliation,
                RuntimeActivationStage::LeaseRefresh,
            ]
        );
        assert_eq!(plan.request.container_name.as_deref(), Some("web"));
        assert_eq!(plan.lease.policy, RuntimeLeasePolicy::RefreshOnActivation);
        assert!(plan.readiness.probe_primary_service_exec);
        assert!(plan.aliases.reconcile_primary_service_aliases);
    }

    #[test]
    fn activation_report_keeps_identity_and_cleanup_result() {
        let report = RuntimeActivationRequest::new(PathBuf::from("/tmp/repo"), "web")
            .container_name("app")
            .lease_policy(RuntimeLeasePolicy::RefreshOnActivation)
            .plan()
            .report(false, RuntimeCleanupResult::Completed);

        assert_eq!(report.repo_root, PathBuf::from("/tmp/repo"));
        assert_eq!(report.policy_name, "web");
        assert_eq!(report.container_name.as_deref(), Some("app"));
        assert_eq!(report.route, RuntimeActivationRoute::Task);
        assert!(!report.system_was_running);
        assert_eq!(report.lease_policy, RuntimeLeasePolicy::RefreshOnActivation);
        assert_eq!(report.cleanup_result, RuntimeCleanupResult::Completed);
    }
}
