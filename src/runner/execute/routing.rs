use effigy_exec::routing::{route, ExecContext, ExecTarget, RoutingDecision, TaskOverrides};
use effigy_manifest::{
    ManifestContainerConfig, ManifestContainersConfig, ManifestSystemsConfig, ManifestTask,
    ManifestTaskRunIn,
};

use super::api::{
    ensure_inline_workspace_supported, resolve_execution_binding_resolution, ExecutionBindingKind,
    InlineWorkspaceCapabilitySurface,
};
use crate::runner::container_runtime::inside_container_handoff;
use crate::runner::error::RunnerError;

#[derive(Debug)]
pub(in crate::runner) struct RoutedTaskExecution {
    pub(in crate::runner) decision: RoutingDecision,
}

pub(in crate::runner) fn route_standard_task_execution(
    command_name: &str,
    default_run_in: Option<ManifestTaskRunIn>,
    task: &ManifestTask,
    systems: Option<&ManifestSystemsConfig>,
    containers: Option<&ManifestContainersConfig>,
    is_container_running: impl Fn(&str) -> Result<bool, RunnerError>,
) -> Result<RoutedTaskExecution, RunnerError> {
    if inside_container_handoff() {
        return Ok(RoutedTaskExecution {
            decision: RoutingDecision::host("running inside an effigy container handoff"),
        });
    }

    let binding_resolution = resolve_execution_binding_resolution(
        default_run_in,
        systems,
        containers,
        command_name,
        task,
        "standard task routing",
    )?;
    let binding = binding_resolution.binding();
    ensure_inline_workspace_supported(
        binding,
        InlineWorkspaceCapabilitySurface::StandardTaskRouting {
            task_name: command_name,
        },
    )?;

    let task_run_in = task.effective_run_in(default_run_in);
    let binding_is_host = binding_resolution.kind() == ExecutionBindingKind::Host;
    let explicit_container = if binding_resolution.kind() == ExecutionBindingKind::NamedContainer {
        binding_resolution
            .requested_container_name()
            .map(str::to_owned)
            .or_else(|| containers.and_then(|config| config.default.clone()))
    } else {
        None
    };

    let (target_container, target_primary_service) =
        resolve_routing_containers(containers, explicit_container.as_deref())?;
    if task_run_in == ManifestTaskRunIn::Container && target_container.is_none() {
        return Err(RunnerError::task_invocation(format!(
            "task `{command_name}` declares `run_in = \"container\"`, but no container target is defined"
        )));
    }
    let task_overrides = TaskOverrides {
        host: task_run_in == ManifestTaskRunIn::Host || binding_is_host,
        container: if task_run_in == ManifestTaskRunIn::Container {
            target_container.clone()
        } else {
            explicit_container.clone()
        },
    };
    let container_running = match target_container.as_deref() {
        Some(name) => is_container_running(name)?,
        None => false,
    };

    let context = match (target_container, target_primary_service) {
        (Some(container), Some(primary_service)) => ExecContext {
            default_container: Some(container),
            primary_service: Some(primary_service),
            container_running,
        },
        _ => ExecContext::none(),
    };

    Ok(RoutedTaskExecution {
        decision: route(command_name, &context, &task_overrides),
    })
}

fn resolve_routing_containers(
    containers: Option<&ManifestContainersConfig>,
    explicit_container: Option<&str>,
) -> Result<(Option<String>, Option<String>), RunnerError> {
    let Some(containers) = containers else {
        return Ok((None, None));
    };

    let target_container = explicit_container.map(str::to_owned);
    let target_primary_service = target_container
        .as_deref()
        .map(|name| primary_service_for(containers, name))
        .transpose()?;

    Ok((target_container, target_primary_service))
}

fn primary_service_for(
    containers: &ManifestContainersConfig,
    container_name: &str,
) -> Result<String, RunnerError> {
    let config = containers.environments.get(container_name).ok_or_else(|| {
        let available = if containers.environments.is_empty() {
            "<none>".to_owned()
        } else {
            containers
                .environments
                .keys()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        RunnerError::task_invocation(format!(
            "container `{container_name}` is not defined in `[containers]` (available: {available})"
        ))
    })?;
    container_primary_service(container_name, config)
}

fn container_primary_service(
    container_name: &str,
    config: &ManifestContainerConfig,
) -> Result<String, RunnerError> {
    config.primary_service.clone().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "container `{container_name}` must declare `primary_service` for transparent execution"
        ))
    })
}

pub(in crate::runner) fn routed_container_target(
    decision: &RoutingDecision,
) -> Option<(&str, &str)> {
    match &decision.target {
        ExecTarget::Container { container, service } => {
            Some((container.as_str(), service.as_str()))
        }
        _ => None,
    }
}

pub(in crate::runner) fn routed_not_running_container(decision: &RoutingDecision) -> Option<&str> {
    match &decision.target {
        ExecTarget::ContainerNotRunning { container } => Some(container.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_test_support::EnvGuard;

    fn manifest_task() -> ManifestTask {
        ManifestTask::default()
    }

    fn containers_toml(body: &str) -> ManifestContainersConfig {
        #[derive(serde::Deserialize)]
        struct Wrapper {
            containers: ManifestContainersConfig,
        }

        toml::from_str::<Wrapper>(body)
            .expect("parse containers")
            .containers
    }

    fn systems_toml(body: &str) -> ManifestSystemsConfig {
        #[derive(serde::Deserialize)]
        struct Wrapper {
            systems: ManifestSystemsConfig,
        }

        toml::from_str::<Wrapper>(body)
            .expect("parse systems")
            .systems
    }

    #[test]
    fn routes_host_when_no_default_system_target_resolves() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let containers = containers_toml(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"
"#,
        );
        let routed = route_standard_task_execution(
            "build",
            None,
            &manifest_task(),
            None,
            Some(&containers),
            |_| panic!("running check should not be used without a default target"),
        )
        .expect("route");
        assert!(matches!(routed.decision.target, ExecTarget::Host));
    }

    #[test]
    fn unrelated_non_default_system_container_does_not_capture_plain_tasks() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let containers = containers_toml(
            r#"
[containers.release]
primary_service = "builder"
"#,
        );
        let systems = systems_toml(
            r#"
[systems.release.workspaces.linux]
container = "release"
"#,
        );
        let routed = route_standard_task_execution(
            "build",
            None,
            &manifest_task(),
            Some(&systems),
            Some(&containers),
            |_| panic!("running check should not be used without a default target"),
        )
        .expect("route");
        assert!(matches!(routed.decision.target, ExecTarget::Host));
    }

    #[test]
    fn routes_to_default_system_workspace_container_without_context_marker() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let containers = containers_toml(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"
"#,
        );
        let systems = systems_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
"#,
        );
        let routed = route_standard_task_execution(
            "build",
            None,
            &manifest_task(),
            Some(&systems),
            Some(&containers),
            |name| {
                assert_eq!(name, "web");
                Ok(true)
            },
        )
        .expect("route");
        assert_eq!(
            routed_container_target(&routed.decision),
            Some(("web", "app"))
        );
    }

    #[test]
    fn routes_to_default_container_when_running() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let containers = containers_toml(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"
"#,
        );
        let systems = systems_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
"#,
        );
        let routed = route_standard_task_execution(
            "build",
            None,
            &manifest_task(),
            Some(&systems),
            Some(&containers),
            |name| {
                assert_eq!(name, "web");
                Ok(true)
            },
        )
        .expect("route");
        assert_eq!(
            routed_container_target(&routed.decision),
            Some(("web", "app"))
        );
    }

    #[test]
    fn host_override_beats_default_execution_container() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let containers = containers_toml(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"
"#,
        );
        let mut task = manifest_task();
        task.run_in = Some(ManifestTaskRunIn::Host);
        let systems = systems_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
"#,
        );

        let routed = route_standard_task_execution(
            "build",
            None,
            &task,
            Some(&systems),
            Some(&containers),
            |_| Ok(true),
        )
        .expect("route");
        assert!(matches!(routed.decision.target, ExecTarget::Host));
    }

    #[test]
    fn container_override_requires_container_target() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let mut task = manifest_task();
        task.run_in = Some(ManifestTaskRunIn::Container);

        let error = route_standard_task_execution("build", None, &task, None, None, |_| Ok(true))
            .expect_err("container override should require a container target");
        assert!(error
            .to_string()
            .contains("declares `run_in = \"container\"`, but no container target is defined"));
    }

    #[test]
    fn container_lifecycle_beats_manifest_default_host_run_in() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let containers = containers_toml(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"
"#,
        );
        let mut task = manifest_task();
        task.container_lifecycle = Some(true);
        let systems = systems_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
"#,
        );

        let routed = route_standard_task_execution(
            "dev",
            Some(ManifestTaskRunIn::Host),
            &task,
            Some(&systems),
            Some(&containers),
            |_| Ok(true),
        )
        .expect("route");
        assert_eq!(
            routed_container_target(&routed.decision),
            Some(("web", "app"))
        );
    }

    #[test]
    fn explicit_host_run_in_still_beats_container_lifecycle() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let containers = containers_toml(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"
"#,
        );
        let mut task = manifest_task();
        task.container_lifecycle = Some(true);
        task.run_in = Some(ManifestTaskRunIn::Host);
        let systems = systems_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
"#,
        );

        let routed = route_standard_task_execution(
            "dev",
            Some(ManifestTaskRunIn::Either),
            &task,
            Some(&systems),
            Some(&containers),
            |_| Ok(true),
        )
        .expect("route");
        assert!(matches!(routed.decision.target, ExecTarget::Host));
    }

    #[test]
    fn multiple_containers_without_default_system_do_not_capture_plain_tasks() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let containers = containers_toml(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.jobs]
primary_service = "worker"
"#,
        );

        let routed = route_standard_task_execution(
            "build",
            None,
            &manifest_task(),
            None,
            Some(&containers),
            |_| Ok(true),
        )
        .expect("route");
        assert!(matches!(routed.decision.target, ExecTarget::Host));
    }

    #[test]
    fn explicit_workspace_container_uses_target_primary_service() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let containers = containers_toml(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.jobs]
primary_service = "worker"
"#,
        );
        let systems = systems_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "jobs"

[systems.dev.workspaces.jobs]
container = "jobs"
"#,
        );
        let mut task = manifest_task();
        task.workspace = Some("jobs".to_owned());

        let routed = route_standard_task_execution(
            "build",
            None,
            &task,
            Some(&systems),
            Some(&containers),
            |name| {
                assert_eq!(name, "jobs");
                Ok(true)
            },
        )
        .expect("route");
        assert_eq!(
            routed_container_target(&routed.decision),
            Some(("jobs", "worker"))
        );
    }

    #[test]
    fn manifest_default_run_in_host_beats_default_execution_container() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let containers = containers_toml(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"
"#,
        );
        let systems = systems_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
"#,
        );

        let routed = route_standard_task_execution(
            "build",
            Some(ManifestTaskRunIn::Host),
            &manifest_task(),
            Some(&systems),
            Some(&containers),
            |_| Ok(true),
        )
        .expect("route");
        assert!(matches!(routed.decision.target, ExecTarget::Host));
    }

    #[test]
    fn workspace_binding_errors_until_runtime_support_lands() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let systems = systems_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
"#,
        );
        let mut task = manifest_task();
        task.workspace = Some("app".to_owned());

        let error =
            route_standard_task_execution("build", None, &task, Some(&systems), None, |_| {
                panic!("routing should fail before runtime lookup")
            })
            .expect_err("workspace binding should not route through legacy path");
        assert!(error
            .to_string()
            .contains("requires that workspace to declare a backing container"));
    }

    #[test]
    fn workspace_binding_uses_named_container_for_routing() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let systems = systems_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "jobs"
"#,
        );
        let containers = containers_toml(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.jobs]
primary_service = "worker"
"#,
        );
        let mut task = manifest_task();
        task.workspace = Some("app".to_owned());

        let routed = route_standard_task_execution(
            "build",
            None,
            &task,
            Some(&systems),
            Some(&containers),
            |name| {
                assert_eq!(name, "jobs");
                Ok(true)
            },
        )
        .expect("route");
        assert_eq!(
            routed_container_target(&routed.decision),
            Some(("jobs", "worker"))
        );
    }

    #[test]
    fn implied_default_workspace_uses_default_container_for_routing() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let systems = systems_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
"#,
        );
        let containers = containers_toml(
            r#"
[containers]
default = "app"

[containers.app]
primary_service = "workspace"
"#,
        );

        let routed = route_standard_task_execution(
            "build",
            None,
            &manifest_task(),
            Some(&systems),
            Some(&containers),
            |name| {
                assert_eq!(name, "app");
                Ok(true)
            },
        )
        .expect("route");
        assert_eq!(
            routed_container_target(&routed.decision),
            Some(("app", "workspace"))
        );
    }

    #[test]
    fn inline_workspace_container_errors_until_policy_support_lands() {
        let _env = EnvGuard::set_many(&[("EFFIGY_INTERNAL_CONTAINER_HANDOFF", None)]);
        let systems = systems_toml(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace" }
"#,
        );
        let mut task = manifest_task();
        task.workspace = Some("app".to_owned());

        let error =
            route_standard_task_execution("build", None, &task, Some(&systems), None, |_| {
                panic!("routing should fail before runtime lookup")
            })
            .expect_err("inline workspace container should not route through legacy path");
        assert!(error
            .to_string()
            .contains("standard task routing does not support inline workspace containers yet"));
    }
}
