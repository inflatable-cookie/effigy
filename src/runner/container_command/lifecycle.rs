use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{
    classify_for_reset, reset_commands, DockerCommand, VolumeClassification,
};
use effigy_containers::{
    compose::{compose_args, resolve_compose_backend, ComposeBackend},
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
    ContainerStatusService, EffectiveAttachMode, EffectiveComposeSource, EffectiveContainerPolicy,
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
    let shared_service_notes = ensure_shared_services_running(&policy)?;
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
        annotate_shared_service_notes(&mut report, &shared_service_notes);
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
    annotate_left_running_shared_services(&mut report, &policy);
    annotate_removed_gateway_route(&mut report, removed_gateway_domain.as_deref());
    Ok(render_container_report(report, output_json))
}

pub(super) fn run_container_reset(
    repo_root: &Path,
    name: Option<&str>,
    keep_data: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    if keep_data && policy.compose_source != EffectiveComposeSource::Generated {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` uses direct `compose_file` ownership; `reset --keep-data` is only supported on the generated-compose path in this batch",
            policy.name
        )));
    }
    let colima_running = colima_is_running(&policy, repo_root)?;
    let volume_actions = if keep_data {
        Some(classify_for_reset(&policy.managed_volumes, true))
    } else if !policy.managed_volumes.is_empty() {
        Some(classify_for_reset(&policy.managed_volumes, false))
    } else {
        None
    };
    if colima_running {
        if keep_data {
            run_docker_capture(
                repo_root,
                &policy,
                &compose_args(&policy, ["down", "--remove-orphans"]),
                "docker compose down",
            )?;
            if let Some(classification) = volume_actions.as_ref() {
                remove_reset_volumes(repo_root, &policy, classification)?;
            }
        } else {
            run_docker_capture(
                repo_root,
                &policy,
                &compose_args(&policy, ["down", "-v", "--remove-orphans"]),
                "docker compose down -v",
            )?;
        }
    }
    let removed_gateway_domain = deregister_gateway_route_for_container(&policy)?;
    let mut report = reset_report(&policy, colima_running, keep_data, volume_actions.as_ref());
    annotate_left_running_shared_services(&mut report, &policy);
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

fn ensure_shared_services_running(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let mut notes = Vec::new();
    for service in &policy.shared_services {
        let workdir = service.compose_file.parent().ok_or_else(|| {
            RunnerError::task_invocation("shared service compose file has no parent directory")
        })?;
        run_shared_compose_capture(
            workdir,
            &policy.profile,
            &shared_compose_args(service, ["up", "-d"]),
            &format!("docker compose up (shared {})", service.service_name),
        )?;
        notes.push(format!(
            "{} [{}] -> {}:{}",
            service.service_name, service.catalog, service.host, service.host_port
        ));
    }
    Ok(notes)
}

fn remove_reset_volumes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    classification: &VolumeClassification,
) -> Result<(), RunnerError> {
    for command in reset_commands(classification) {
        run_runtime_volume_capture(repo_root, &policy.profile, &command)?;
    }
    Ok(())
}

fn annotate_shared_service_notes(
    report: &mut effigy_containers::ContainerCommandReport,
    notes: &[String],
) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            json!({
                "action": "ensured",
                "services": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        report
            .success_text
            .push_str(&format!("[shared] ensured {note}"));
    }
}

fn annotate_left_running_shared_services(
    report: &mut effigy_containers::ContainerCommandReport,
    policy: &EffectiveContainerPolicy,
) {
    if policy.shared_services.is_empty() {
        return;
    }
    let services = policy
        .shared_services
        .iter()
        .map(|service| {
            format!(
                "{} [{}] -> {}:{}",
                service.service_name, service.catalog, service.host, service.host_port
            )
        })
        .collect::<Vec<_>>();
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            json!({
                "action": "left-running",
                "services": services,
            }),
        );
    }
    for service in services {
        report.success_text.push('\n');
        report
            .success_text
            .push_str(&format!("[shared] left running {service}"));
    }
}

fn shared_compose_args<'a>(
    service: &effigy_containers::SharedServiceBinding,
    tail: impl IntoIterator<Item = &'a str>,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("compose"),
        OsString::from("-f"),
        service.compose_file.as_os_str().to_os_string(),
        OsString::from("-p"),
        OsString::from(service.project_name.as_str()),
    ];
    args.extend(tail.into_iter().map(OsString::from));
    args
}

fn run_shared_compose_capture(
    repo_root: &Path,
    profile: &str,
    args: &[OsString],
    label: &str,
) -> Result<std::process::Output, RunnerError> {
    let (program, args) = match resolve_compose_backend() {
        ComposeBackend::Docker => ("docker", args.to_vec()),
        ComposeBackend::ColimaNerdctl => {
            let mut resolved = vec![
                OsString::from("nerdctl"),
                OsString::from("--profile"),
                OsString::from(profile),
                OsString::from("--"),
            ];
            resolved.extend(args.iter().cloned());
            ("colima", resolved)
        }
    };
    std::process::Command::new(program)
        .current_dir(repo_root)
        .args(&args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{label} ({program} {})", format_shared_args(&args)),
            error,
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(RunnerError::task_invocation(format!(
                    "{label} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
}

fn run_runtime_volume_capture(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<Output, RunnerError> {
    let (program, args) = match resolve_compose_backend() {
        ComposeBackend::Docker => (command.program.as_str(), runtime_args(&command.args)),
        ComposeBackend::ColimaNerdctl => {
            let mut resolved = vec![
                OsString::from("nerdctl"),
                OsString::from("--profile"),
                OsString::from(profile),
                OsString::from("--"),
            ];
            resolved.extend(runtime_args(&command.args));
            ("colima", resolved)
        }
    };
    std::process::Command::new(program)
        .current_dir(repo_root)
        .args(&args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} ({program} {})",
                command.description,
                format_shared_args(&args)
            ),
            error,
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(RunnerError::task_invocation(format!(
                    "{} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    command.description,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
}

fn runtime_args(args: &[String]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn format_shared_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn load_port_registry() -> Option<PortRegistry> {
    let home = std::env::var_os("HOME")?;
    let path = Path::new(&home).join(".effigy/ports.json");
    PortRegistry::load(&path).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        annotate_left_running_shared_services, annotate_shared_service_notes, run_container_eject,
        run_container_reset,
    };
    use effigy_containers::{
        down_report, up_detached_report, EffectiveComposeSource, EffectiveContainerPolicy,
        SharedServiceBinding,
    };
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};

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

    fn test_policy(shared_services: Vec<SharedServiceBinding>) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "default".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services,
            project_name: "demo-web-dev".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            declared_ports: vec!["8080:80".to_owned()],
            ports_declared_explicitly: true,
            declared_mounts: vec![],
            health_check: None,
            health_timeout_secs: 60,
            ui_tabs: vec![],
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
        }
    }

    fn shared_service(name: &str, catalog: &str, host_port: u16) -> SharedServiceBinding {
        SharedServiceBinding {
            service_name: name.to_owned(),
            catalog: catalog.to_owned(),
            project_name: format!("effigy-shared-{catalog}"),
            compose_file: Path::new("/tmp").join(format!("{name}-{catalog}.yml")),
            host: "host.docker.internal".to_owned(),
            host_port,
            container_port: match catalog {
                "mariadb" => 3306,
                "postgres" => 5432,
                "redis" => 6379,
                "memcached" => 11211,
                other => panic!("unexpected shared catalog {other}"),
            },
        }
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

    #[test]
    fn annotate_shared_service_notes_adds_text_and_json() {
        let policy = test_policy(vec![]);
        let mut report = up_detached_report(&policy, false, Some("ready"));

        annotate_shared_service_notes(
            &mut report,
            &[
                "db [mariadb] -> host.docker.internal:8106".to_owned(),
                "cache [redis] -> host.docker.internal:8110".to_owned(),
            ],
        );

        assert!(report
            .success_text
            .contains("[shared] ensured db [mariadb] -> host.docker.internal:8106"));
        assert!(report
            .success_text
            .contains("[shared] ensured cache [redis] -> host.docker.internal:8110"));
        assert_eq!(
            report.json["shared_service_actions"],
            json!({
                "action": "ensured",
                "services": [
                    "db [mariadb] -> host.docker.internal:8106",
                    "cache [redis] -> host.docker.internal:8110"
                ]
            })
        );
    }

    #[test]
    fn annotate_left_running_shared_services_adds_text_and_json() {
        let policy = test_policy(vec![
            shared_service("db", "mariadb", 8106),
            shared_service("cache", "redis", 8110),
        ]);
        let mut report = down_report(&policy, true);

        annotate_left_running_shared_services(&mut report, &policy);

        assert!(report
            .success_text
            .contains("[shared] left running db [mariadb] -> host.docker.internal:8106"));
        assert!(report
            .success_text
            .contains("[shared] left running cache [redis] -> host.docker.internal:8110"));
        assert_eq!(
            report.json["shared_service_actions"],
            json!({
                "action": "left-running",
                "services": [
                    "db [mariadb] -> host.docker.internal:8106",
                    "cache [redis] -> host.docker.internal:8110"
                ]
            })
        );
    }

    #[test]
    fn run_container_reset_keep_data_rejects_direct_compose_ownership() {
        let root = temp_repo("reset-keep-data-direct");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let error = run_container_reset(&root, None, true, false).expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`reset --keep-data` is only supported on the generated-compose path"));
    }
}
