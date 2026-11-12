//! The one activation path: build the runtime activation plan for a route,
//! take the environment hold, prepare the stack, prove the workspace and the
//! gateway, and hand the caller the published snapshot.

use std::path::{Path, PathBuf};

use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::id::RunId;
use effigy_runtime_plan::{
    RuntimeActivationPlan, RuntimeActivationRequest, RuntimeActivationRoute, RuntimeLeasePolicy,
};
#[cfg(test)]
use effigy_runtime_plan::{RuntimeActivationReport, RuntimeCleanupResult};

use effigy_containers::environment::Retention;

use crate::runner::environment::Held;
use crate::runner::error::RunnerError;
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, RuntimeSessionContext};

use super::exec_prep::prepare_container_exec_runtime;
use super::gateway::ensure_runtime_gateway_readiness_stage;
use super::running::{check_runtime_running_state_stage, ensure_runtime_running_stage};
use super::validation::validate_policy_runtime;

#[derive(Debug)]
pub(in crate::runner) struct ContainerTaskActivation {
    /// The immutable snapshot used by every activation stage and its exec caller.
    pub(in crate::runner) policy: EffectiveContainerPolicy,
    pub(in crate::runner) system_was_running: bool,
    /// This activation started a held environment: it stops when the last
    /// run using it ends.
    pub(in crate::runner) started_held: bool,
    /// The command's hold on its environment, released when the activation
    /// drops at the end of the command, or earlier by a command whose own
    /// closeout stops the environment.
    pub(in crate::runner) hold: Option<Held>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct ActivationRequest<'a> {
    pub(in crate::runner) container_name: Option<&'a str>,
    pub(in crate::runner) repo_override: Option<PathBuf>,
    pub(in crate::runner) route: RuntimeActivationRoute,
    pub(in crate::runner) session_context: RuntimeSessionContext,
    /// Whether this activation may reconcile the environment's shared services.
    /// A data command that proves its own target sets this false, so its
    /// activation never publishes, starts or replaces a shared service it does
    /// not target.
    pub(in crate::runner) reconcile_shared_services: bool,
}

pub(in crate::runner) fn ensure_container_runtime_prepared(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    route: RuntimeActivationRoute,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
    start: StartPolicy<'_>,
) -> Result<bool, RunnerError> {
    validate_policy_runtime(repo_root, policy)?;
    if route == RuntimeActivationRoute::Exec {
        return ensure_exec_container_running_with(policy, container_name, || {
            check_runtime_running_state_stage(repo_root, policy)
        });
    }
    let system_was_running = check_runtime_running_state_stage(repo_root, policy)?;
    if !system_was_running && !start.reconcile_shared_services {
        // Bringing the environment up runs the ordinary apply, which reconciles
        // every shared service. A command that must not touch them asks the
        // user for the environment command instead.
        return Err(RunnerError::task_invocation(format!(
            "container `{}` is not running, and this command must not start it because bringing it up would reconcile its shared services. Run `effigy container {} up` first, then re-run.",
            container_name.unwrap_or(policy.name.as_str()),
            container_name.unwrap_or(policy.name.as_str()),
        )));
    }
    ensure_runtime_running_stage(
        system_was_running,
        container_name.map(str::to_owned),
        repo_override,
        |name, repo_override| {
            crate::runner::container_command::run_container_up_held(
                repo_override,
                name,
                start.retention,
                start.holders,
            )
        },
    )?;

    prepare_container_exec_runtime(
        repo_root,
        policy,
        container_name.or(Some(policy.name.as_str())),
        start,
    )?;
    Ok(system_was_running)
}

/// How an activation starts or reconciles a stack: the retention a stack it
/// starts gets, and the runs the activation attaches.
#[derive(Debug, Clone, Copy)]
pub(in crate::runner) struct StartPolicy<'a> {
    pub(in crate::runner) retention: Retention,
    pub(in crate::runner) holders: &'a [RunId],
    /// Whether this apply reconciles the environment's shared services.
    pub(in crate::runner) reconcile_shared_services: bool,
}

pub(super) fn ensure_exec_container_running_with(
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    is_running: impl FnOnce() -> Result<bool, RunnerError>,
) -> Result<bool, RunnerError> {
    if is_running()? {
        return Ok(true);
    }
    Err(RunnerError::container_surface_not_running(
        container_name.unwrap_or(policy.name.as_str()),
    ))
}

pub(in crate::runner) fn activate_container_runtime_for_task(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    request: ActivationRequest<'_>,
) -> Result<ContainerTaskActivation, RunnerError> {
    let prepared = effigy_containers::policy::render::prepare_resolved(policy)?;
    let policy = &prepared;
    let reconcile_shared_services = request.reconcile_shared_services;
    let plan = runtime_activation_plan_from_request(repo_root, policy, request);
    activate_container_runtime_plan_for_task_using(
        &plan,
        policy,
        reconcile_shared_services,
        ensure_container_runtime_prepared,
        crate::runner::workspace_command::ensure_workspace_permissions_ready,
        ensure_runtime_gateway_readiness_stage,
    )
}

/// Activate the named container of the manifest at `repo_root` for a routed
/// task or suite execution.
///
/// This is the one activation path shared by the standard task pipeline and
/// container-routed builtin test suites: the policy loads from the manifest
/// that owns the declared runtime target.
/// Whether an activation of `policy` may reconcile that environment's shared
/// services.
///
/// A data seed's session leaves them alone, but the decision belongs to the
/// policy actually activated: a catalog-routed step into another catalog's
/// container is judged by that catalog's declaration, so a seed that started in
/// an environment without shared services still cannot start, publish or
/// replace an unrelated shared service elsewhere (effigy#477).
pub(in crate::runner) fn reconcile_shared_services_for(
    policy: &EffectiveContainerPolicy,
    session_context: RuntimeSessionContext,
) -> bool {
    !(session_context.data_seed_session && !policy.shared_services.is_empty())
}

pub(in crate::runner) fn activate_routed_container_runtime(
    repo_root: &Path,
    container_name: &str,
) -> Result<ContainerTaskActivation, RunnerError> {
    effigy_core::worktree_scope::prepare_checkout_token(repo_root).map_err(|error| {
        RunnerError::task_invocation(format!("cannot record the checkout identity: {error}"))
    })?;
    activate_routed_container_runtime_with(
        repo_root,
        container_name,
        |repo_root, container_name| {
            effigy_containers::load_container_policy(repo_root, Some(container_name))
                .map_err(|error| RunnerError::task_invocation(error.to_string()))
        },
        |repo_root, policy, request, _plan| {
            activate_container_runtime_for_task(repo_root, policy, request)
        },
    )
}

/// Injectable form of [`activate_routed_container_runtime`] for tests.
pub(in crate::runner) fn activate_routed_container_runtime_with(
    repo_root: &Path,
    container_name: &str,
    load_policy: impl FnOnce(&Path, &str) -> Result<EffectiveContainerPolicy, RunnerError>,
    activate: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        ActivationRequest<'_>,
        &RuntimeActivationPlan,
    ) -> Result<ContainerTaskActivation, RunnerError>,
) -> Result<ContainerTaskActivation, RunnerError> {
    let policy = load_policy(repo_root, container_name)?;
    let session_context = crate::runner::runtime_session_context::current_runtime_session_context();
    let plan = build_runtime_activation_plan(
        repo_root,
        policy.name.as_str(),
        Some(container_name),
        Some(repo_root.to_path_buf()),
        RuntimeActivationRoute::Task,
        session_context,
    );
    activate(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: plan.request.container_name.as_deref(),
            repo_override: plan.request.repo_override.clone(),
            route: plan.route,
            session_context,
            reconcile_shared_services: reconcile_shared_services_for(&policy, session_context),
        },
        &plan,
    )
}

pub(in crate::runner) fn build_runtime_activation_plan(
    repo_root: &Path,
    policy_name: &str,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
    route: RuntimeActivationRoute,
    session_context: RuntimeSessionContext,
) -> RuntimeActivationPlan {
    let mut plan_request =
        RuntimeActivationRequest::new(repo_root.to_path_buf(), policy_name.to_owned())
            .repo_override(repo_override.unwrap_or_else(|| repo_root.to_path_buf()))
            .route(route)
            .lease_policy(runtime_lease_policy(session_context));
    if let Some(container_name) = container_name {
        plan_request = plan_request.container_name(container_name.to_owned());
    }
    plan_request.plan()
}

pub(super) fn runtime_activation_plan_from_request(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    request: ActivationRequest<'_>,
) -> RuntimeActivationPlan {
    build_runtime_activation_plan(
        repo_root,
        &policy.name,
        request.container_name,
        request.repo_override,
        request.route,
        request.session_context,
    )
}

pub(super) fn runtime_lease_policy(session_context: RuntimeSessionContext) -> RuntimeLeasePolicy {
    match session_context.lease_refresh_policy {
        LeaseRefreshPolicy::RefreshOnActivation => RuntimeLeasePolicy::RefreshOnActivation,
        LeaseRefreshPolicy::SkipRefresh => RuntimeLeasePolicy::Skip,
    }
}

pub(super) fn activate_container_runtime_plan_for_task_using(
    plan: &RuntimeActivationPlan,
    policy: &EffectiveContainerPolicy,
    reconcile_shared_services: bool,
    ensure_runtime_prepared: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        RuntimeActivationRoute,
        Option<&str>,
        Option<PathBuf>,
        StartPolicy<'_>,
    ) -> Result<bool, RunnerError>,
    ensure_workspace_permissions: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        Option<&str>,
        Option<PathBuf>,
    ) -> Result<(), RunnerError>,
    ensure_gateway_ready: impl FnOnce(&Path, &EffectiveContainerPolicy) -> Result<(), RunnerError>,
) -> Result<ContainerTaskActivation, RunnerError> {
    let repo_root = plan.request.repo_root.as_path();
    // The command's session exists before any bring-up, so the bring-up's
    // activation attaches it and no idle stop can intervene (§5.9).
    let mut hold = Held::begin(repo_root, policy, activation_label(plan.route))?;
    let holders = hold
        .as_ref()
        .and_then(Held::id)
        .into_iter()
        .collect::<Vec<_>>();
    // An activation inside a managed workspace keeps the stack it starts.
    let retention = if plan.lease.policy == RuntimeLeasePolicy::RefreshOnActivation {
        crate::runner::environment::task_retention()
    } else {
        Retention::Persistent
    };
    let system_was_running = ensure_runtime_prepared(
        repo_root,
        policy,
        plan.route,
        plan.request.container_name.as_deref(),
        plan.request.repo_override.clone(),
        StartPolicy {
            retention,
            holders: &holders,
            reconcile_shared_services,
        },
    )?;
    ensure_workspace_permissions(
        repo_root,
        policy,
        plan.request.container_name.as_deref(),
        plan.request.repo_override.clone(),
    )?;
    if plan.aliases.register_gateway_routes {
        ensure_gateway_ready(repo_root, policy)?;
    }
    // A stack that was already running needed no bring-up: attach to it. A
    // stack the reconciler never recorded (a plain exec target started
    // elsewhere) runs unheld until a bring-up records it.
    if let Some(held) = hold.as_mut() {
        if !held.holds() {
            held.attach_active()?;
        }
    }
    Ok(ContainerTaskActivation {
        policy: policy.clone(),
        system_was_running,
        started_held: !system_was_running
            && retention == Retention::Held
            && plan.route != RuntimeActivationRoute::Exec,
        hold,
    })
}

pub(super) fn activation_label(route: RuntimeActivationRoute) -> &'static str {
    match route {
        RuntimeActivationRoute::Exec => "exec",
        _ => "task",
    }
}

#[cfg(test)]
pub(super) fn runtime_activation_report_for_result(
    plan: RuntimeActivationPlan,
    activation: ContainerTaskActivation,
) -> RuntimeActivationReport {
    plan.report(
        activation.system_was_running,
        RuntimeCleanupResult::NotRequired,
    )
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use effigy_containers::EffectiveContainerPolicy;

    use super::super::tests::test_policy;
    use super::activate_routed_container_runtime_with;

    /// A task's environment ensure starts the shared services and delivers the
    /// compose-env secrets before the same apply `container up` runs, so
    /// alternating the two paths recreates nothing (effigy#371).
    /// A data seed's routed activation judges the policy it actually activates: an
    /// environment with shared services is left alone, one without them keeps its
    /// ordinary activation, and an ordinary session still reconciles both
    /// (effigy#477 review).
    #[test]
    fn a_data_seed_session_decides_from_the_policy_being_activated() {
        use crate::runner::db_seed::data_seed_runtime_session_context;
        use crate::runner::runtime_session_context::{
            with_runtime_session_context, RuntimeSessionContext,
        };

        fn policy_with_shared_service() -> EffectiveContainerPolicy {
            let mut policy = test_policy(PathBuf::from("docker-compose.yml"));
            policy.shared_services = vec![effigy_containers::SharedServiceBinding {
                service_name: "mysql".to_owned(),
                catalog: "mariadb".to_owned(),
                domain_label: "mysql".to_owned(),
                project_name: "effigy-shared-mariadb-0721de3e".to_owned(),
                compose_file: PathBuf::from("/machine/shared/mariadb/compose.yml"),
                host: "127.0.0.1".to_owned(),
                host_port: 33_066,
                container_port: 3306,
                host_env_vars: vec![],
                port_env_vars: vec![],
                render_digest: "abc".to_owned(),
            }];
            policy
        }

        fn capture(
            session: RuntimeSessionContext,
            policy: &EffectiveContainerPolicy,
        ) -> Option<bool> {
            let mut captured = None;
            with_runtime_session_context(session, || {
                let _ = activate_routed_container_runtime_with(
                    Path::new("/tmp/demo-repo"),
                    "web",
                    |_repo_root, _container_name| Ok(policy.clone()),
                    |_repo_root, _policy, request, _plan| {
                        captured = Some(request.reconcile_shared_services);
                        Err(crate::runner::error::RunnerError::task_invocation(
                            "stop after capture",
                        ))
                    },
                );
            });
            captured
        }

        let shared = policy_with_shared_service();
        let unshared = test_policy(PathBuf::from("docker-compose.yml"));
        assert_eq!(
            capture(data_seed_runtime_session_context(), &shared),
            Some(false),
            "a data seed must leave an activated environment's shared services alone"
        );
        assert_eq!(
            capture(data_seed_runtime_session_context(), &unshared),
            Some(true),
            "an environment with no shared services keeps its ordinary activation"
        );
        assert_eq!(
            capture(RuntimeSessionContext::default(), &shared),
            Some(true),
            "an ordinary task activation still reconciles shared services"
        );
    }
}
