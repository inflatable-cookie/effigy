use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

use effigy_containers::{
    compose::compose_args,
    down_report, effective_attach_mode, eject_generated_compose, eject_report,
    exec::{
        capture_compose_ps, capture_running_container_stats, colima_is_running,
        ensure_colima_running, list_running_compose_containers,
        shutdown_container as shutdown_container_via_exec, RunningComposeContainer,
    },
    health::{probe_health_status, wait_for_ready},
    load_all_container_policies, load_container_policy, reset_report, stats_all_report,
    status_all_report, status_report, up_detached_report, validate_container_policy,
    AllocatedPortsSummary, ContainerStatsAllEntry, ContainerStatsService, ContainerStatusAllEntry,
    ContainerStatusService, EffectiveAttachMode, EffectiveContainerPolicy,
};
use effigy_gateway::ports::PortRegistry;
use serde_json::json;

use super::gateway_registration::{
    deregister_gateway_route_for_container, register_gateway_route_for_container,
    RegisteredGatewayRoute,
};
use super::session::{render_attached_session_closeout, run_attached_container_session};
use super::signals::{install_stop_requested_flag, run_docker_capture, spawn_docker_inherit};
use super::{render_container_report, RunnerError};

const DEFAULT_CONTAINER_SHELL: &str = "sh";

pub(in crate::runner) fn run_task_container_session(
    repo_root: &Path,
    task_name: &str,
    container_name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    if output_json {
        return Err(RunnerError::task_invocation(format!(
            "task `{task_name}` uses `container_session` and does not support `--json` because the session is interactive"
        )));
    }

    let stop_requested = install_stop_requested_flag()?;
    let policy = load_container_policy(
        repo_root,
        normalize_task_container_reference(container_name),
    )?;
    validate_container_policy(repo_root, &policy)?;
    if stop_requested.load(std::sync::atomic::Ordering::Relaxed) {
        return render_attached_session_closeout(repo_root, &policy, false, "signal");
    }
    let colima_started = ensure_colima_running(&policy, repo_root)?;
    if stop_requested.load(std::sync::atomic::Ordering::Relaxed) {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    run_docker_capture(
        repo_root,
        &policy,
        &compose_args(&policy, ["up", "-d"]),
        "docker compose up",
    )?;
    if stop_requested.load(std::sync::atomic::Ordering::Relaxed) {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    let health = wait_for_container_ready(&policy, Some(stop_requested.as_ref()))?;
    if stop_requested.load(std::sync::atomic::Ordering::Relaxed) {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    run_attached_container_session(repo_root, &policy, colima_started, health, Some(task_name))
}

fn normalize_task_container_reference(container_name: Option<&str>) -> Option<&str> {
    match container_name.map(str::trim) {
        Some("default") => None,
        Some("") => None,
        other => other,
    }
}

pub(super) fn run_container_up(
    repo_root: &Path,
    name: Option<&str>,
    attach: bool,
    detach: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if attach && detach {
        return Err(RunnerError::task_invocation(
            "`effigy container up` cannot combine `--attach` and `--detach`",
        ));
    }

    let startup_stop_requested = install_stop_requested_flag()?;
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let attach_mode = effective_attach_mode(&policy, attach, detach);
    let stop_requested = if attach_mode == EffectiveAttachMode::Attached {
        Some(startup_stop_requested)
    } else {
        None
    };
    let colima_started = ensure_colima_running(&policy, repo_root)?;
    if stop_requested
        .as_ref()
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    run_docker_capture(
        repo_root,
        &policy,
        &compose_args(&policy, ["up", "-d"]),
        "docker compose up",
    )?;
    if stop_requested
        .as_ref()
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    let health = wait_for_container_ready(&policy, stop_requested.as_deref())?;
    if stop_requested
        .as_ref()
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    {
        return render_attached_session_closeout(repo_root, &policy, colima_started, "signal");
    }
    let gateway_route = register_gateway_route_for_container(repo_root, &policy)?;

    if attach_mode == EffectiveAttachMode::Detached {
        let mut report = up_detached_report(&policy, colima_started, health);
        annotate_registered_gateway_route(&mut report, gateway_route.as_ref());
        return Ok(render_container_report(report, output_json));
    }

    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container up --json` is only supported for detached bring-up; attached sessions stream live output instead",
        ));
    }

    run_attached_container_session(repo_root, &policy, colima_started, health, None)
}

pub(super) fn run_container_down(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let colima_running = colima_is_running(&policy, repo_root)?;
    if colima_running {
        shutdown_container_via_exec(repo_root, &policy)?;
    }
    let removed_gateway_domain = deregister_gateway_route_for_container(&policy)?;
    let mut report = down_report(&policy, colima_running);
    annotate_removed_gateway_route(&mut report, removed_gateway_domain.as_deref());
    Ok(render_container_report(report, output_json))
}

pub(super) fn run_container_reset(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let colima_running = colima_is_running(&policy, repo_root)?;
    if colima_running {
        run_docker_capture(
            repo_root,
            &policy,
            &compose_args(&policy, ["down", "-v", "--remove-orphans"]),
            "docker compose down -v",
        )?;
    }
    let removed_gateway_domain = deregister_gateway_route_for_container(&policy)?;
    let mut report = reset_report(&policy, colima_running);
    annotate_removed_gateway_route(&mut report, removed_gateway_domain.as_deref());
    Ok(render_container_report(report, output_json))
}

pub(super) fn run_container_eject(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let result = eject_generated_compose(repo_root, &policy)?;
    rewrite_manifest_for_ejected_compose(repo_root, &policy.name, &result.compose_path)?;
    Ok(render_container_report(
        eject_report(&policy, &result),
        output_json,
    ))
}

pub(super) fn run_container_status(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let colima_running = colima_is_running(&policy, repo_root)?;
    let compose_ps = if colima_running {
        Some(capture_compose_ps(
            repo_root,
            &policy,
            &compose_args(&policy, ["ps"]),
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
    Ok(render_container_report(
        status_report(&policy, colima_running, health, compose_ps.as_deref()),
        output_json,
    ))
}

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
    let stats = capture_running_container_stats(
        &environments
            .iter()
            .flat_map(|environment| {
                environment
                    .services
                    .iter()
                    .map(|service| service.container_name.clone())
            })
            .collect::<Vec<_>>(),
    );
    let stats_by_container = stats
        .stats
        .into_iter()
        .map(|sample| {
            let container_name = sample.container_name.clone();
            (container_name, sample)
        })
        .collect::<BTreeMap<_, _>>();

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

    Ok(render_container_report(
        stats_all_report(&environments, stats.warning.as_deref()),
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

pub(super) fn run_container_logs(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    follow: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if follow && output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container logs --follow` does not support `--json`",
        ));
    }

    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    if !colima_is_running(&policy, repo_root)? {
        return Err(RunnerError::task_invocation(format!(
            "Colima profile `{}` is not running for container `{}`",
            policy.profile, policy.name
        )));
    }
    let service = service.unwrap_or(policy.primary_service.as_str());

    if follow {
        let mut child = spawn_docker_inherit(
            repo_root,
            &policy,
            &compose_args(&policy, ["logs", "--follow", service]),
            "docker compose logs --follow",
        )?;
        let status = child
            .wait()
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        if !status.success() {
            return Err(RunnerError::task_invocation(format!(
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
        &compose_args(&policy, ["logs", "--tail", "100", service]),
        "docker compose logs",
    )?;
    let rendered = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(render_container_report(
        effigy_containers::logs_report(&policy, service, &rendered),
        output_json,
    ))
}

pub(super) fn run_container_shell(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container shell` does not support `--json` because it is interactive",
        ));
    }

    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    if !colima_is_running(&policy, repo_root)? {
        return Err(RunnerError::task_invocation(format!(
            "Colima profile `{}` is not running for container `{}`",
            policy.profile, policy.name
        )));
    }
    let service = service.unwrap_or(policy.primary_service.as_str());
    let args = if let Some(command) = command {
        let mut args = compose_args(&policy, ["exec", service, "sh", "-lc"]);
        args.push(OsString::from(command));
        args
    } else {
        compose_args(&policy, ["exec", service, DEFAULT_CONTAINER_SHELL])
    };
    let status = spawn_docker_inherit(repo_root, &policy, &args, "docker compose exec")?
        .wait()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if !status.success() {
        return Err(RunnerError::task_invocation(format!(
            "docker compose exec exited with status {status}"
        )));
    }
    Ok(format!(
        "[ok] finished container shell for `{}` service `{service}`",
        policy.name
    ))
}

fn wait_for_container_ready(
    policy: &EffectiveContainerPolicy,
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Option<&'static str>, RunnerError> {
    wait_for_ready(
        &policy.name,
        policy.health_check.as_deref(),
        policy.health_timeout_secs,
        stop_requested,
    )
    .map_err(RunnerError::task_invocation)
}

fn rewrite_manifest_for_ejected_compose(
    repo_root: &Path,
    container_name: &str,
    compose_path: &Path,
) -> Result<(), RunnerError> {
    let manifest_path = repo_root.join("effigy.toml");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|error| RunnerError::task_invocation_failed_read(&manifest_path, error))?;
    let mut document = raw.parse::<toml_edit::DocumentMut>().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse {} while finalizing eject: {error}",
            manifest_path.display()
        ))
    })?;

    let containers = document["containers"].as_table_like_mut().ok_or_else(|| {
        RunnerError::task_invocation("manifest missing `[containers]` while finalizing eject")
    })?;
    let container = containers
        .get_mut(container_name)
        .and_then(|item| item.as_table_like_mut())
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "manifest missing `[containers.{container_name}]` while finalizing eject"
            ))
        })?;
    container.remove("services");
    let relative = compose_path
        .strip_prefix(repo_root)
        .unwrap_or(compose_path)
        .display()
        .to_string();
    container.insert("compose_file", toml_edit::value(relative));

    std::fs::write(&manifest_path, document.to_string())
        .map_err(|error| RunnerError::task_invocation_failed_write(&manifest_path, error))
}

fn annotate_registered_gateway_route(
    report: &mut effigy_containers::ContainerCommandReport,
    route: Option<&RegisteredGatewayRoute>,
) {
    let Some(route) = route else {
        return;
    };
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_route".to_owned(),
            json!({
                "action": "registered",
                "domain": route.domain,
                "target": route.target,
                "tls": route.tls,
            }),
        );
    }
    report.success_text.push('\n');
    report.success_text.push_str(&format!(
        "[gateway] registered {} -> {}",
        route.domain, route.target
    ));
}

fn annotate_removed_gateway_route(
    report: &mut effigy_containers::ContainerCommandReport,
    domain: Option<&str>,
) {
    let Some(domain) = domain else {
        return;
    };
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_route".to_owned(),
            json!({
                "action": "removed",
                "domain": domain,
            }),
        );
    }
    report.success_text.push('\n');
    report
        .success_text
        .push_str(&format!("[gateway] removed {domain}"));
}

fn load_port_registry() -> Option<PortRegistry> {
    let home = std::env::var_os("HOME")?;
    let path = Path::new(&home).join(".effigy/ports.json");
    PortRegistry::load(&path).ok()
}

#[cfg(test)]
mod tests {
    use super::run_container_eject;
    use std::fs;
    use std::path::PathBuf;

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-container-eject-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        root
    }

    #[test]
    fn run_container_eject_promotes_generated_compose() {
        let root = temp_repo("generated");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let rendered = run_container_eject(&root, None, false).expect("eject");
        assert!(rendered.contains("ejected catalog-backed compose ownership"));
        assert!(root.join("infra/dev/docker-compose.yml").exists());
        assert!(!root
            .join("infra/dev/.effigy-compose.generated.yml")
            .exists());
        let manifest = fs::read_to_string(root.join("effigy.toml")).expect("read manifest");
        assert!(manifest.contains("compose_file = \"infra/dev/docker-compose.yml\""));
        assert!(!manifest.contains("[containers.web.services.app]"));
    }
}
