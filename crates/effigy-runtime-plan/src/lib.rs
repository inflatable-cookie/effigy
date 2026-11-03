use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeActivationRequest {
    pub repo_root: PathBuf,
    pub policy_name: String,
    pub container_name: Option<String>,
    pub repo_override: Option<PathBuf>,
    pub route: RuntimeActivationRoute,
    pub lease_policy: RuntimeLeasePolicy,
}

impl RuntimeActivationRequest {
    pub fn new(repo_root: PathBuf, policy_name: impl Into<String>) -> Self {
        Self {
            repo_root,
            policy_name: policy_name.into(),
            container_name: None,
            repo_override: None,
            route: RuntimeActivationRoute::Task,
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

    pub fn route(mut self, route: RuntimeActivationRoute) -> Self {
        self.route = route;
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
        let route = request.route;
        let register_gateway_routes = matches!(
            route,
            RuntimeActivationRoute::Managed
                | RuntimeActivationRoute::Workspace
                | RuntimeActivationRoute::Bootstrap
        );
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
            route,
            readiness: RuntimeReadinessPlan {
                probe_primary_service_exec: true,
                restart_on_failed_probe: true,
            },
            aliases: RuntimeAliasPlan {
                reconcile_primary_service_aliases: true,
                register_gateway_routes,
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
    Managed,
    Exec,
    Workspace,
    Bootstrap,
    Rhai,
    Deferral,
    DataSeed,
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

    /// `activate_container_runtime_for_task` reads exactly these request fields
    /// and the lease policy out of the plan
    /// (src/runner/container_runtime_prep/mod.rs): `request.repo_root` names
    /// the held session root, `request.repo_override` and
    /// `request.container_name` flow into every stage, and the lease policy
    /// selects the activation's task retention. The plan must echo them
    /// unchanged; a request that omits a container name must stay unnamed.
    #[test]
    fn plan_echoes_the_request_identity_the_activation_adapter_reads() {
        let unnamed = RuntimeActivationRequest::new(PathBuf::from("/tmp/repo"), "web")
            .repo_override(PathBuf::from("/tmp/override"))
            .plan();
        assert_eq!(unnamed.request.repo_root, PathBuf::from("/tmp/repo"));
        assert_eq!(
            unnamed.request.repo_override,
            Some(PathBuf::from("/tmp/override"))
        );
        assert_eq!(unnamed.request.container_name, None);

        let named = RuntimeActivationRequest::new(PathBuf::from("/tmp/repo"), "web")
            .container_name("web")
            .lease_policy(RuntimeLeasePolicy::RefreshOnActivation)
            .plan();
        assert_eq!(named.request.container_name.as_deref(), Some("web"));
        assert_eq!(named.lease.policy, RuntimeLeasePolicy::RefreshOnActivation);
    }

    /// `plan.aliases.register_gateway_routes` gates `ensure_gateway_ready` in
    /// the activation adapter, and `plan.route` reaches the stage label and the
    /// held-start decision. Exactly the Managed, Workspace and Bootstrap
    /// routes register gateway routes; every other route skips them.
    #[test]
    fn gateway_route_registration_follows_the_activation_route() {
        for (route, expected) in [
            (RuntimeActivationRoute::Task, false),
            (RuntimeActivationRoute::Exec, false),
            (RuntimeActivationRoute::Rhai, false),
            (RuntimeActivationRoute::Deferral, false),
            (RuntimeActivationRoute::DataSeed, false),
            (RuntimeActivationRoute::Managed, true),
            (RuntimeActivationRoute::Workspace, true),
            (RuntimeActivationRoute::Bootstrap, true),
        ] {
            let plan = RuntimeActivationRequest::new(PathBuf::from("/tmp/repo"), "web")
                .route(route)
                .plan();
            assert_eq!(plan.route, route, "{route:?} must echo into the plan");
            assert_eq!(
                plan.aliases.register_gateway_routes, expected,
                "{route:?} gateway-route registration"
            );
        }
    }
}
