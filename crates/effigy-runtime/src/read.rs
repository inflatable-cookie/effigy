mod discovery;
mod report;

use std::collections::BTreeMap;
use std::path::Path;

use effigy_containers::{
    exec::{
        capture_running_container_stats_for_profile, colima_is_running, colima_profile_warnings,
        list_running_compose_containers_for_policy, runtime_backend_is_running,
        selected_backend_label,
    },
    health::probe_health_status,
    load_container_exec_working_dir, load_container_policy, logs_report, status_report,
    validate_compose_backend_runtime, validate_container_policy,
};
use effigy_containers::{
    stats_global_report, status_global_report, ContainerStatsAllEntry, ContainerStatsService,
    ContainerStatusService, EffectiveContainerPolicy,
};
use effigy_containers::{ContainerAction, ContainerRuntimeState};
use effigy_containers::{
    ContainerOperationKind, ContainerOperationPlan, ContainerOperationRequest,
    ContainerReadOperation,
};

use crate::container_manager::{compose_invocation_plan, lifecycle_operation_report};
use crate::signals::{run_compose_plan_capture, spawn_compose_plan_inherit};
use crate::EffigyRuntimeError;
pub(crate) use discovery::{
    discover_effigy_repos_under, discover_running_environments,
    filter_running_environments_for_scope, DiscoveredRunningEnvironment,
};
pub use discovery::{resolve_effigy_repo_root, working_dir_belongs_to_repo, MAX_REPO_ROOT_WALKUP};
use report::{annotate_warning_lines, environment_status_entry, render_container_report};

pub fn run_container_status(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, EffigyRuntimeError> {
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let _operation_plan =
        read_operation_plan(repo_root, &policy, ContainerReadOperation::status(false));
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let runtime_running = runtime_backend_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let _manager_report = lifecycle_operation_report(
        repo_root,
        &policy,
        ContainerAction::Status,
        if runtime_running {
            ContainerRuntimeState::Running
        } else {
            ContainerRuntimeState::Stopped
        },
        None,
    )?;
    let services = if runtime_running {
        discover_running_services_for_policy(repo_root, &policy)?
    } else {
        Vec::new()
    };
    let compose_ps = if runtime_running && services.is_empty() {
        let plan = compose_invocation_plan(
            repo_root,
            &policy,
            ["ps"],
            ContainerAction::Status,
            "docker compose ps",
        )?;
        let output = run_compose_plan_capture(&policy, &plan)?;
        Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        None
    };
    let health = if runtime_running {
        probe_health_status(policy.health_check.as_deref())
    } else {
        None
    };
    let (primary_service_exec_ready, primary_service_exec_warning) = if runtime_running {
        probe_primary_service_exec_ready(repo_root, &policy, name)?
    } else {
        (None, None)
    };
    let mut report = status_report(
        &policy,
        selected_backend_label(&policy, repo_root),
        runtime_running,
        health,
        primary_service_exec_ready,
        runtime_running.then_some(services.as_slice()),
        compose_ps.as_deref(),
    );
    annotate_warning_lines(&mut report, &colima_profile_warnings(&policy, repo_root));
    if let Some(warning) = primary_service_exec_warning {
        annotate_warning_lines(&mut report, &[warning]);
    }
    Ok(render_container_report(report, output_json))
}

pub fn run_container_logs(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    follow: bool,
    output_json: bool,
) -> Result<String, EffigyRuntimeError> {
    if follow && output_json {
        return Err(EffigyRuntimeError::task_invocation(
            "`effigy container logs --follow` does not support `--json`",
        ));
    }

    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let _operation_plan = read_operation_plan(
        repo_root,
        &policy,
        ContainerReadOperation::logs(service.map(str::to_owned), follow),
    );
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    if !colima_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?
    {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "Colima profile `{}` is not running for container `{}`",
            policy.profile, policy.name
        )));
    }
    let service = service.unwrap_or(policy.primary_service.as_str());
    let _manager_report = lifecycle_operation_report(
        repo_root,
        &policy,
        ContainerAction::Logs,
        ContainerRuntimeState::Running,
        None,
    )?;

    if follow {
        let plan = compose_invocation_plan(
            repo_root,
            &policy,
            ["logs", "--follow", service],
            ContainerAction::Logs,
            "docker compose logs --follow",
        )?;
        let mut child = spawn_compose_plan_inherit(&plan)?;
        let status = child
            .wait()
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
        if !status.success() {
            return Err(EffigyRuntimeError::task_invocation(format!(
                "docker compose logs --follow exited with status {status}"
            )));
        }
        return Ok(format!(
            "[ok] finished following logs for `{}` service `{service}`",
            policy.name
        ));
    }

    let plan = compose_invocation_plan(
        repo_root,
        &policy,
        ["logs", "--tail", "100", service],
        ContainerAction::Logs,
        "docker compose logs",
    )?;
    let output = run_compose_plan_capture(&policy, &plan)?;
    let rendered = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(render_container_report(
        logs_report(&policy, service, &rendered),
        output_json,
    ))
}

pub fn run_container_status_all(output_json: bool) -> Result<String, EffigyRuntimeError> {
    let environments = discover_running_environments()?
        .into_iter()
        .map(|environment| {
            let _operation_plan = read_operation_plan(
                Path::new(&environment.repo_root),
                &environment.policy,
                ContainerReadOperation::status(true),
            );
            environment_status_entry(&environment)
        })
        .collect::<Vec<_>>();

    Ok(render_container_report(
        status_global_report(&environments),
        output_json,
    ))
}

pub fn run_container_status_under_path(
    scope_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, EffigyRuntimeError> {
    let environments =
        filter_running_environments_for_scope(discover_running_environments()?, scope_root, name);
    for environment in &environments {
        let _operation_plan = read_operation_plan(
            Path::new(&environment.repo_root),
            &environment.policy,
            ContainerReadOperation::status(true),
        );
    }
    Ok(render_container_report(
        status_global_report(
            &environments
                .iter()
                .map(environment_status_entry)
                .collect::<Vec<_>>(),
        ),
        output_json,
    ))
}

pub fn run_container_stats_all(output_json: bool) -> Result<String, EffigyRuntimeError> {
    let environments = discover_running_environments()?;
    for environment in &environments {
        let _operation_plan = read_operation_plan(
            Path::new(&environment.repo_root),
            &environment.policy,
            ContainerReadOperation::stats(true),
        );
        let _manager_report = lifecycle_operation_report(
            Path::new(&environment.repo_root),
            &environment.policy,
            ContainerAction::Stats,
            ContainerRuntimeState::Running,
            None,
        )?;
    }
    let mut stats_warning_lines = Vec::new();
    let mut stats_by_container = BTreeMap::new();
    let grouped_names = environments.iter().fold(
        BTreeMap::<String, Vec<String>>::new(),
        |mut acc, environment| {
            acc.entry(environment.runtime_profile.clone())
                .or_default()
                .extend(
                    environment
                        .services
                        .iter()
                        .map(|service| service.container_name.clone()),
                );
            acc
        },
    );
    for (profile, container_names) in grouped_names {
        let capture = capture_running_container_stats_for_profile(&profile, &container_names);
        if let Some(warning) = capture.warning {
            stats_warning_lines.push(warning);
        }
        for sample in capture.stats {
            stats_by_container.insert(sample.container_name.clone(), sample);
        }
    }

    let environments = environments
        .into_iter()
        .map(|environment| {
            let policy = environment.policy;
            let repo_root = environment.repo_root;
            ContainerStatsAllEntry {
                repo_root,
                container: policy.name.clone(),
                project_name: policy.project_name.clone(),
                profile: environment.runtime_profile,
                primary_service: policy.primary_service.clone(),
                services: environment
                    .services
                    .into_iter()
                    .map(|service| {
                        let sample = stats_by_container.get(&service.container_name);
                        ContainerStatsService {
                            name: service
                                .service
                                .clone()
                                .unwrap_or_else(|| service.container_name.clone()),
                            container_name: service.container_name,
                            status: service.status,
                            cpu_percent: sample.and_then(|value| value.cpu_percent.clone()),
                            memory_usage: sample.and_then(|value| value.memory_usage.clone()),
                            memory_percent: sample.and_then(|value| value.memory_percent.clone()),
                        }
                    })
                    .collect(),
            }
        })
        .collect::<Vec<_>>();

    let stats_warning = if stats_warning_lines.is_empty() {
        None
    } else {
        Some(stats_warning_lines.join("; "))
    };
    Ok(render_container_report(
        stats_global_report(&environments, stats_warning.as_deref()),
        output_json,
    ))
}

pub fn read_operation_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    operation: ContainerReadOperation,
) -> ContainerOperationPlan {
    ContainerOperationRequest::new(
        repo_root.to_path_buf(),
        policy.name.clone(),
        ContainerOperationKind::read(operation),
    )
    .backend_id(read_backend_id(policy))
    .plan()
}

fn read_backend_id(policy: &EffectiveContainerPolicy) -> &'static str {
    match policy.driver {
        effigy_manifest::ManifestContainerDriver::Colima => "colima",
    }
}

fn discover_running_services_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<ContainerStatusService>, EffigyRuntimeError> {
    Ok(
        list_running_compose_containers_for_policy(repo_root, policy)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?
            .into_iter()
            .map(|service| ContainerStatusService {
                name: service
                    .service
                    .clone()
                    .unwrap_or_else(|| service.container_name.clone()),
                container_name: service.container_name,
                status: service.status,
                ports: service.ports,
            })
            .collect(),
    )
}

fn probe_primary_service_exec_ready(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    requested_name: Option<&str>,
) -> Result<(Option<bool>, Option<String>), EffigyRuntimeError> {
    let working_dir = load_container_exec_working_dir(repo_root, requested_name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let working_dir_str = working_dir.to_string_lossy().into_owned();
    let plan = compose_invocation_plan(
        repo_root,
        policy,
        [
            "exec",
            "-T",
            "-w",
            working_dir_str.as_str(),
            policy.primary_service.as_str(),
            "true",
        ],
        ContainerAction::Status,
        "docker compose exec readiness status probe",
    )?;
    let output = run_compose_plan_capture(&policy, &plan)?;
    Ok(primary_service_exec_readiness(
        policy.primary_service.as_str(),
        &working_dir,
        output.status.success(),
    ))
}

fn primary_service_exec_readiness(
    primary_service: &str,
    working_dir: &Path,
    exec_ready: bool,
) -> (Option<bool>, Option<String>) {
    if exec_ready {
        return (Some(true), None);
    }

    (
        Some(false),
        Some(format!(
            "primary service `{}` is not exec-ready in `{}`; runtime state may be drifted",
            primary_service,
            working_dir.display()
        )),
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{primary_service_exec_readiness, read_operation_plan};
    use effigy_containers::EffectiveContainerPolicy;
    use effigy_containers::{
        ContainerOperationKind, ContainerReadOperation, ContainerSideEffectClass,
    };
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };

    #[test]
    fn read_operation_plan_keeps_policy_identity_and_backend_id() {
        let policy = stub_policy("web");
        let plan = read_operation_plan(
            std::path::Path::new("/tmp/repo"),
            &policy,
            ContainerReadOperation::status(false),
        );

        assert_eq!(
            plan.request.repo_root,
            std::path::PathBuf::from("/tmp/repo")
        );
        assert_eq!(plan.request.policy_name, "web");
        assert_eq!(plan.request.backend_id.as_deref(), Some("colima"));
        assert_eq!(plan.side_effect, ContainerSideEffectClass::ReadsRuntime);
    }

    #[test]
    fn read_logs_operation_plan_keeps_service_and_follow_flags() {
        let policy = stub_policy("web");
        let plan = read_operation_plan(
            std::path::Path::new("/tmp/repo"),
            &policy,
            ContainerReadOperation::logs(Some("worker".to_owned()), true),
        );

        match plan.request.kind {
            ContainerOperationKind::Read(ContainerReadOperation::Logs(operation)) => {
                assert_eq!(operation.service.as_deref(), Some("worker"));
                assert!(operation.follow);
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }

    #[test]
    fn primary_service_exec_readiness_reports_ready_state_without_warning() {
        let (exec_ready, warning) =
            primary_service_exec_readiness("workspace", Path::new("/workspace-root/app"), true);

        assert_eq!(exec_ready, Some(true));
        assert_eq!(warning, None);
    }

    #[test]
    fn primary_service_exec_readiness_reports_drift_warning_with_context() {
        let (exec_ready, warning) =
            primary_service_exec_readiness("workspace", Path::new("/workspace-root/app"), false);

        assert_eq!(exec_ready, Some(false));
        let warning = warning.expect("warning");
        assert!(warning.contains("primary service `workspace`"));
        assert!(warning.contains("/workspace-root/app"));
        assert!(warning.contains("runtime state may be drifted"));
    }

    fn stub_policy(name: &str) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: name.to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: effigy_containers::EffectiveComposeSource::Generated,
            compose_files: vec![],
            compose_file_display: String::new(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: format!("{name}-project"),
            primary_service: "app".to_owned(),
            dns_domain: None,
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
            host_processes: vec![],
        }
    }
}
