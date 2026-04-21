use std::collections::BTreeMap;
use std::path::Path;

use effigy_containers::{
    exec::{
        capture_running_container_stats_for_profile, list_running_compose_containers,
        RunningComposeContainer,
    },
    load_all_container_policies, stats_all_report, status_all_report, AllocatedPortsSummary,
    ContainerStatsAllEntry, ContainerStatsService, ContainerStatusAllEntry, ContainerStatusService,
    EffectiveContainerPolicy,
};
use effigy_gateway::ports::PortRegistry;

use super::{render_container_report, RunnerError};

pub(super) fn run_container_status_all(output_json: bool) -> Result<String, RunnerError> {
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

pub(super) fn run_container_stats_all(output_json: bool) -> Result<String, RunnerError> {
    let environments = discover_running_environments()?;
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

#[derive(Debug)]
struct DiscoveredRunningEnvironment {
    repo_root: String,
    policy: EffectiveContainerPolicy,
    services: Vec<RunningComposeContainer>,
}

fn discover_running_environments() -> Result<Vec<DiscoveredRunningEnvironment>, RunnerError> {
    let rows = list_running_compose_containers()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut grouped: BTreeMap<(String, String), Vec<_>> = BTreeMap::new();
    for row in rows {
        let Some(project_name) = row.project_name.clone() else {
            continue;
        };
        let Some(repo_root) = row.working_dir.clone() else {
            continue;
        };
        grouped
            .entry((repo_root, project_name))
            .or_default()
            .push(row);
    }

    let mut environments = Vec::new();
    for ((repo_root, project_name), mut services) in grouped {
        let repo_path = Path::new(&repo_root);
        if !repo_path.join("effigy.toml").is_file() {
            continue;
        }
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

fn load_port_registry() -> Option<PortRegistry> {
    let home = std::env::var_os("HOME")?;
    let path = Path::new(&home).join(".effigy/ports.json");
    PortRegistry::load(&path).ok()
}
