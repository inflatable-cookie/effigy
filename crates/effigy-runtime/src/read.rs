use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use effigy_container_manager::{ContainerAction, ContainerRuntimeState};
use effigy_containers::{
    exec::{
        capture_compose_ps, capture_running_container_stats_for_profile, colima_is_running,
        colima_profile_warnings, infer_host_working_dir_for_container,
        list_running_compose_containers_profiled, RunningComposeContainer,
    },
    health::probe_health_status,
    load_all_container_policies, load_container_policy, logs_report, status_report,
    validate_compose_backend_runtime, validate_container_policy,
};
use effigy_containers::{
    stats_all_report, status_all_report, AllocatedPortsSummary, ContainerCommandReport,
    ContainerStatsAllEntry, ContainerStatsService, ContainerStatusAllEntry, ContainerStatusService,
    EffectiveContainerPolicy,
};
use effigy_gateway::ports::PortRegistry;

use crate::container_manager::lifecycle_operation_report;
use crate::signals::{run_docker_capture, spawn_docker_inherit};
use crate::EffigyRuntimeError;

pub fn run_container_status(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, EffigyRuntimeError> {
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let colima_running = colima_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let _manager_report = lifecycle_operation_report(
        repo_root,
        &policy,
        ContainerAction::Status,
        if colima_running {
            ContainerRuntimeState::Running
        } else {
            ContainerRuntimeState::Stopped
        },
        None,
    )?;
    let compose_ps = if colima_running {
        Some(capture_compose_ps(
            repo_root,
            &policy,
            &effigy_containers::compose::compose_args(&policy, ["ps"]),
            "docker compose ps",
        )?)
    } else {
        None
    };
    let health = if colima_running {
        probe_health_status(policy.health_check.as_deref())
    } else {
        None
    };
    let mut report = status_report(&policy, colima_running, health, compose_ps.as_deref());
    annotate_warning_lines(&mut report, &colima_profile_warnings(&policy, repo_root));
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
        let mut child = spawn_docker_inherit(
            repo_root,
            &policy,
            &effigy_containers::compose::compose_args(&policy, ["logs", "--follow", service]),
            "docker compose logs --follow",
        )?;
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

    let output = run_docker_capture(
        repo_root,
        &policy,
        &effigy_containers::compose::compose_args(&policy, ["logs", "--tail", "100", service]),
        "docker compose logs",
    )?;
    let rendered = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(render_container_report(
        logs_report(&policy, service, &rendered),
        output_json,
    ))
}

pub fn run_container_status_all(output_json: bool) -> Result<String, EffigyRuntimeError> {
    let registry = load_port_registry();
    let environments = discover_running_environments()?
        .into_iter()
        .map(|environment| {
            let policy = environment.policy;
            let repo_root = environment.repo_root;
            ContainerStatusAllEntry {
                repo_root,
                container: policy.name.clone(),
                project_name: policy.project_name.clone(),
                profile: policy.profile.clone(),
                primary_service: policy.primary_service.clone(),
                dns_domain: policy.dns_domain.clone(),
                dns_tls: policy.dns_tls,
                declared_ports: policy.declared_ports.clone(),
                allocated_ports: registry
                    .as_ref()
                    .and_then(|value| value.port_map(&policy.project_name))
                    .map(|ports| AllocatedPortsSummary {
                        base: ports.base,
                        http: ports.http,
                        mysql: ports.mysql,
                        postgres: ports.postgres,
                        redis: ports.redis,
                        memcached: ports.memcached,
                    }),
                services: environment
                    .services
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
            }
        })
        .collect::<Vec<_>>();

    Ok(render_container_report(
        status_all_report(&environments),
        output_json,
    ))
}

pub fn run_container_stats_all(output_json: bool) -> Result<String, EffigyRuntimeError> {
    let environments = discover_running_environments()?;
    for environment in &environments {
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
            acc.entry(environment.policy.profile.clone())
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
                profile: policy.profile.clone(),
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
        stats_all_report(&environments, stats_warning.as_deref()),
        output_json,
    ))
}

fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

fn annotate_warning_lines(report: &mut ContainerCommandReport, warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("warnings".to_owned(), serde_json::json!(warnings));
    }
    for warning in warnings {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[warn] {warning}"));
    }
}

#[derive(Debug)]
pub(crate) struct DiscoveredRunningEnvironment {
    pub(crate) repo_root: String,
    pub(crate) policy: EffectiveContainerPolicy,
    pub(crate) services: Vec<RunningComposeContainer>,
}

/// Maximum directory levels to walk up from a container's
/// `com.docker.compose.project.working_dir` label looking for an
/// `effigy.toml` marker.
///
/// Generated compose stacks live at `<repo>/.effigy/runtime/compose/`,
/// which Docker labels as the project working_dir (three levels deep).
/// `MAX_REPO_ROOT_WALKUP` allows for that plus a small grace margin so
/// future relocations of the compose payload still resolve.
pub const MAX_REPO_ROOT_WALKUP: usize = 6;

pub(crate) fn discover_running_environments(
) -> Result<Vec<DiscoveredRunningEnvironment>, EffigyRuntimeError> {
    let rows = list_running_compose_containers_profiled()
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let mut grouped: BTreeMap<(String, String), Vec<_>> = BTreeMap::new();
    for profiled in rows {
        let row = profiled.row;
        let Some(project_name) = row.project_name.clone() else {
            continue;
        };
        let Some(working_dir) = row.working_dir.clone().or_else(|| {
            infer_host_working_dir_for_container(&profiled.profile, &row.container_name)
                .ok()
                .flatten()
        }) else {
            continue;
        };
        let Some(repo_path) =
            resolve_effigy_repo_root(Path::new(&working_dir), MAX_REPO_ROOT_WALKUP)
        else {
            continue;
        };
        let repo_root = repo_path.display().to_string();
        grouped
            .entry((repo_root, project_name))
            .or_default()
            .push(row);
    }

    let mut environments = Vec::new();
    for ((repo_root, project_name), mut services) in grouped {
        let repo_path = Path::new(&repo_root);
        let Ok(policies) = load_all_container_policies(repo_path) else {
            continue;
        };
        let Some(policy) = policies
            .into_iter()
            .find(|policy| policy.project_name == project_name)
        else {
            continue;
        };
        services.sort_by(|left, right| {
            left.service
                .as_deref()
                .unwrap_or(left.container_name.as_str())
                .cmp(
                    right
                        .service
                        .as_deref()
                        .unwrap_or(right.container_name.as_str()),
                )
        });

        environments.push(DiscoveredRunningEnvironment {
            repo_root,
            policy,
            services,
        });
    }
    environments.sort_by(|left, right| {
        left.repo_root
            .cmp(&right.repo_root)
            .then(left.policy.name.cmp(&right.policy.name))
    });
    Ok(environments)
}

/// Walk up from `start` looking for an `effigy.toml` marker.
///
/// Returns the first ancestor directory (inclusive of `start`) that
/// contains an `effigy.toml`, or `None` if no marker is found within
/// `max_depth` ancestors.
pub fn resolve_effigy_repo_root(start: &Path, max_depth: usize) -> Option<PathBuf> {
    let mut current = start;
    for _ in 0..=max_depth {
        if current.join("effigy.toml").is_file() {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
    None
}

/// Whether a Docker compose `working_dir` label points into `repo_root`.
///
/// Generated compose stacks emit a label like
/// `<repo>/.effigy/runtime/compose/`, which is not equal to the repo root
/// itself. Walk up from the labelled directory looking for `effigy.toml`
/// and compare the resolved owning repo against `repo_root`. Falls back to
/// an exact match when no marker is found within the walk-up budget.
pub fn working_dir_belongs_to_repo(working_dir: &str, repo_root: &Path) -> bool {
    let working_dir_path = Path::new(working_dir);
    match resolve_effigy_repo_root(working_dir_path, MAX_REPO_ROOT_WALKUP) {
        Some(resolved) => resolved == repo_root,
        None => working_dir_path == repo_root,
    }
}

fn load_port_registry() -> Option<PortRegistry> {
    let home = std::env::var_os("HOME")?;
    let path = Path::new(&home).join(".effigy/ports.json");
    PortRegistry::load(&path).ok()
}

#[cfg(test)]
mod tests {
    use super::{resolve_effigy_repo_root, MAX_REPO_ROOT_WALKUP};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn resolve_effigy_repo_root_returns_repo_when_started_at_repo_root() {
        let temp = tempdir().expect("tempdir");
        let repo = temp.path();
        fs::write(repo.join("effigy.toml"), "[manifest]\n").expect("write manifest");

        let resolved = resolve_effigy_repo_root(repo, MAX_REPO_ROOT_WALKUP).expect("resolved");

        assert_eq!(resolved, repo);
    }

    #[test]
    fn resolve_effigy_repo_root_walks_up_from_generated_compose_dir() {
        let temp = tempdir().expect("tempdir");
        let repo = temp.path();
        fs::write(repo.join("effigy.toml"), "[manifest]\n").expect("write manifest");
        let compose_dir = repo.join(".effigy/runtime/compose");
        fs::create_dir_all(&compose_dir).expect("compose dir");

        let resolved =
            resolve_effigy_repo_root(&compose_dir, MAX_REPO_ROOT_WALKUP).expect("resolved");

        assert_eq!(resolved, repo);
    }

    #[test]
    fn resolve_effigy_repo_root_returns_none_when_no_marker_present() {
        let temp = tempdir().expect("tempdir");
        let stray = temp.path().join("a/b/c");
        fs::create_dir_all(&stray).expect("stray dir");

        let resolved = resolve_effigy_repo_root(&stray, 2);

        assert!(resolved.is_none(), "got: {resolved:?}");
    }

    #[test]
    fn resolve_effigy_repo_root_respects_max_depth() {
        let temp = tempdir().expect("tempdir");
        let repo = temp.path();
        fs::write(repo.join("effigy.toml"), "[manifest]\n").expect("write manifest");
        let deep = repo.join("a/b/c/d/e/f/g/h");
        fs::create_dir_all(&deep).expect("deep dir");

        // Eight levels deep — exceeds MAX_REPO_ROOT_WALKUP.
        let resolved = resolve_effigy_repo_root(&deep, MAX_REPO_ROOT_WALKUP);
        assert!(
            resolved.is_none(),
            "expected None for excessive depth, got: {resolved:?}"
        );
    }
}
