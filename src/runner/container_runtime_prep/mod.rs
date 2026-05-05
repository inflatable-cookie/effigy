use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use effigy_cli::{ContainerArgs, ContainerSubcommand};
use effigy_containers::compose::compose_args;
use effigy_containers::exec::{ensure_colima_running, run_docker_capture};
use effigy_containers::session::{managed_gateway_command, resolve_effigy_invocation_prefix};
use effigy_containers::{
    load_container_exec_working_dir, validate_compose_backend_runtime, validate_container_policy,
    EffectiveContainerPolicy,
};

use crate::runner::container_command::support::reconcile_primary_service_tcp_alias_hosts;
use crate::runner::container_command::{register_gateway_routes_for_container, run_container};
use crate::runner::error::RunnerError;
use crate::runner::gateway_command::gateway_up_for_managed_task;
use crate::runner::host_container_lease::refresh_host_container_lease_for_task_activation;
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, RuntimeSessionContext};
use crate::runner::system_command::is_primary_service_running;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) struct ContainerTaskActivation {
    pub(in crate::runner) system_was_running: bool,
    pub(in crate::runner) refreshed_host_container_lease: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum ExecutionSurfaceKind {
    StandardTask,
    DeferredTask,
    ExplicitExec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct ActivationRequest<'a> {
    pub(in crate::runner) surface: ExecutionSurfaceKind,
    pub(in crate::runner) container_name: Option<&'a str>,
    pub(in crate::runner) repo_override: Option<PathBuf>,
    pub(in crate::runner) session_context: RuntimeSessionContext,
}

pub(in crate::runner) fn ensure_container_runtime_prepared(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
) -> Result<bool, RunnerError> {
    validate_policy_runtime(repo_root, policy)?;
    let system_was_running = is_primary_service_running(repo_root, policy)?;
    if !system_was_running {
        run_container(ContainerArgs {
            subcommand: ContainerSubcommand::Up {
                name: container_name.map(str::to_owned),
                attach: false,
                detach: true,
            },
            repo_override,
            output_json: false,
        })?;
    }

    prepare_container_exec_runtime(
        repo_root,
        policy,
        container_name.or(Some(policy.name.as_str())),
    )?;
    Ok(system_was_running)
}

pub(in crate::runner) fn activate_container_runtime_for_task(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    request: ActivationRequest<'_>,
) -> Result<ContainerTaskActivation, RunnerError> {
    activate_container_runtime_for_task_using(
        repo_root,
        policy,
        request,
        ensure_container_runtime_prepared,
        ensure_task_container_gateway_ready,
        refresh_host_container_lease_for_task_activation,
    )
}

fn activate_container_runtime_for_task_using(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    request: ActivationRequest<'_>,
    ensure_runtime_prepared: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        Option<&str>,
        Option<PathBuf>,
    ) -> Result<bool, RunnerError>,
    ensure_gateway_ready: impl FnOnce(&Path, &EffectiveContainerPolicy) -> Result<(), RunnerError>,
    refresh_host_container_lease: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        bool,
    ) -> Result<bool, RunnerError>,
) -> Result<ContainerTaskActivation, RunnerError> {
    let system_was_running = ensure_runtime_prepared(
        repo_root,
        policy,
        request.container_name,
        request.repo_override,
    )?;
    ensure_gateway_ready(repo_root, policy)?;
    let refreshed_host_container_lease = if matches!(
        request.session_context.lease_refresh_policy,
        LeaseRefreshPolicy::RefreshOnActivation
    ) {
        refresh_host_container_lease(repo_root, policy, system_was_running)?
    } else {
        false
    };
    Ok(ContainerTaskActivation {
        system_was_running,
        refreshed_host_container_lease,
    })
}

pub(in crate::runner) fn prepare_container_exec_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
) -> Result<(), RunnerError> {
    validate_policy_runtime(repo_root, policy)?;
    let _colima_started = ensure_colima_running(policy, repo_root)?;
    let working_dir = load_container_exec_working_dir(repo_root, container_name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    run_runtime_prep_steps(
        || prepare_host_bind_mount_dirs(repo_root, policy),
        || {
            let _ = run_docker_capture(
                repo_root,
                policy,
                &compose_args(policy, ["up", "-d"]),
                "docker compose up (idempotent)",
            );
        },
        || ensure_primary_service_exec_ready_for_runtime(repo_root, policy, &working_dir),
        || reconcile_primary_service_tcp_alias_hosts(repo_root, policy).map(|_| ()),
    )
}

pub(in crate::runner) fn container_policy_uses_gateway_surface(
    policy: &EffectiveContainerPolicy,
) -> bool {
    !(policy.dns_routes.is_empty()
        && policy.service_aliases.is_empty()
        && policy.shared_services.is_empty())
}

fn validate_policy_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    validate_container_policy(repo_root, policy).map_err(|error| {
        RunnerError::container_runtime_policy("policy validation", error.to_string())
    })?;
    validate_compose_backend_runtime(repo_root, policy).map_err(|error| {
        RunnerError::container_runtime_policy("backend validation", error.to_string())
    })?;
    Ok(())
}

pub(in crate::runner) fn ensure_primary_service_exec_ready_for_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
) -> Result<(), RunnerError> {
    ensure_primary_service_exec_ready_with_recovery_using(
        policy,
        working_dir,
        |timeout| probe_primary_service_exec_ready(repo_root, policy, working_dir, timeout),
        || restart_primary_service(repo_root, policy),
    )
}

fn ensure_primary_service_exec_ready_with_recovery_using(
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    mut probe: impl FnMut(Duration) -> bool,
    mut restart: impl FnMut() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    if probe(Duration::from_secs(2)) {
        return Ok(());
    }
    if restart().is_ok() && probe(Duration::from_secs(15)) {
        return Ok(());
    }
    Err(RunnerError::container_runtime_exec_not_ready(
        policy,
        working_dir,
    ))
}

fn run_runtime_prep_steps(
    prepare_mounts: impl FnOnce() -> Result<(), RunnerError>,
    compose_up: impl FnOnce(),
    ensure_exec_ready: impl FnOnce() -> Result<(), RunnerError>,
    reconcile_aliases: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    // The routing decision only checks whether the primary service is running.
    // Sibling services may still be missing after a failed compose bring-up.
    prepare_mounts()?;
    compose_up();
    ensure_exec_ready()?;
    reconcile_aliases()?;
    Ok(())
}

fn ensure_task_container_gateway_ready(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if !container_policy_uses_gateway_surface(policy) {
        return Ok(());
    }
    let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    let command = managed_gateway_command(&executable);
    gateway_up_for_managed_task(&command)?;
    let _ = register_gateway_routes_for_container(repo_root, policy)?;
    Ok(())
}

fn probe_primary_service_exec_ready(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    timeout: Duration,
) -> bool {
    let working_dir_str = working_dir.to_string_lossy().into_owned();
    let probe_args = compose_args(
        policy,
        [
            "exec",
            "-T",
            "-w",
            working_dir_str.as_str(),
            policy.primary_service.as_str(),
            "true",
        ],
    );
    let started = Instant::now();
    loop {
        if let Ok(output) = run_docker_capture(
            repo_root,
            policy,
            &probe_args,
            "container exec readiness probe",
        ) {
            if output.status.success() {
                return true;
            }
        }
        if started.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn restart_primary_service(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    restart_primary_service_using(repo_root, policy, |repo_root, policy, args, label| {
        Ok(run_docker_capture(repo_root, policy, args, label)?)
    })
}

fn restart_primary_service_using(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    run_compose: impl Fn(
        &Path,
        &EffectiveContainerPolicy,
        &[std::ffi::OsString],
        &str,
    ) -> Result<std::process::Output, RunnerError>,
) -> Result<(), RunnerError> {
    let restart_args = compose_args(policy, ["restart", policy.primary_service.as_str()]);
    run_compose(
        repo_root,
        policy,
        &restart_args,
        "docker compose restart primary service",
    )?;

    let dependent_services = load_services_depending_on_primary(repo_root, policy)?;
    if dependent_services.is_empty() {
        return Ok(());
    }

    for service in dependent_services {
        let restart_args = compose_args(policy, ["restart", service.as_str()]);
        run_compose(
            repo_root,
            policy,
            &restart_args,
            "docker compose restart dependent service",
        )?;
    }
    Ok(())
}

fn load_services_depending_on_primary(
    _repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let mut services = Vec::new();
    for compose_file in &policy.compose_files {
        let raw = match std::fs::read_to_string(compose_file) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let yaml: serde_yaml::Value = match serde_yaml::from_str(&raw) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(service_map) = yaml.get("services").and_then(|value| value.as_mapping()) else {
            continue;
        };
        for (service_name, service_value) in service_map {
            let Some(service_name) = service_name.as_str() else {
                continue;
            };
            if service_name == policy.primary_service {
                continue;
            }
            let depends_on = service_value.get("depends_on");
            if service_depends_on_primary(depends_on, &policy.primary_service)
                && !services.iter().any(|existing| existing == service_name)
            {
                services.push(service_name.to_owned());
            }
        }
    }
    Ok(services)
}

fn service_depends_on_primary(
    depends_on: Option<&serde_yaml::Value>,
    primary_service: &str,
) -> bool {
    let Some(depends_on) = depends_on else {
        return false;
    };
    if let Some(sequence) = depends_on.as_sequence() {
        return sequence
            .iter()
            .filter_map(|value| value.as_str())
            .any(|value| value == primary_service);
    }
    if let Some(mapping) = depends_on.as_mapping() {
        return mapping
            .keys()
            .filter_map(|value| value.as_str())
            .any(|value| value == primary_service);
    }
    false
}

fn prepare_host_bind_mount_dirs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    for compose_file in &policy.compose_files {
        let raw = match std::fs::read_to_string(compose_file) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let yaml: serde_yaml::Value = match serde_yaml::from_str(&raw) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(services) = yaml.get("services").and_then(|s| s.as_mapping()) else {
            continue;
        };
        for (_service_name, service_value) in services {
            let Some(volumes) = service_value.get("volumes").and_then(|v| v.as_sequence()) else {
                continue;
            };
            for volume in volumes {
                let Some(spec) = volume.as_str() else {
                    continue;
                };
                let Some(host_path) = parse_bind_mount_host_path(spec) else {
                    continue;
                };
                let host_path = Path::new(host_path);
                if !host_path.is_absolute() || !host_path.starts_with(repo_root) {
                    continue;
                }
                if let Some(extension) = host_path.extension() {
                    let ext_str = extension.to_string_lossy();
                    if matches!(
                        ext_str.as_ref(),
                        "conf" | "yml" | "yaml" | "toml" | "json" | "sql" | "ini" | "env"
                    ) {
                        continue;
                    }
                }
                let _ = std::fs::create_dir_all(host_path);
            }
        }
    }
    Ok(())
}

fn parse_bind_mount_host_path(spec: &str) -> Option<&str> {
    let host = spec.split(':').next()?;
    if host.starts_with('/') || host.starts_with('.') || host.starts_with('~') {
        Some(host)
    } else {
        None
    }
}

#[cfg(test)]
#[cfg(test)]
mod tests;
