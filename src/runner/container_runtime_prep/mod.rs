use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, Instant};
#[cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt};

mod gateway;
mod lease;
mod prep;
mod running;
mod validation;

use effigy_containers::compose::compose_args;
use effigy_containers::exec::{ensure_colima_running, run_compose_capture};
use effigy_containers::{load_container_exec_working_dir, EffectiveContainerPolicy};
use effigy_runtime_plan::{
    RuntimeActivationPlan, RuntimeActivationRequest, RuntimeActivationRoute, RuntimeLeasePolicy,
};
#[cfg(test)]
use effigy_runtime_plan::{RuntimeActivationReport, RuntimeCleanupResult};

pub(in crate::runner) use gateway::container_policy_uses_gateway_surface;
use gateway::ensure_runtime_gateway_readiness_stage;
#[cfg(test)]
use gateway::ensure_runtime_gateway_readiness_stage_using;
use lease::refresh_runtime_lease_stage;
use prep::{
    ensure_runtime_exec_readiness_stage, prepare_runtime_mounts_stage,
    reconcile_runtime_aliases_stage, run_runtime_compose_up_stage,
};
#[cfg(test)]
use prep::{ensure_runtime_exec_readiness_stage_using, reconcile_runtime_aliases_stage_using};
use running::{check_runtime_running_state_stage, ensure_runtime_running_stage};
use validation::validate_policy_runtime;
#[cfg(test)]
use validation::validate_runtime_activation_stage;

use crate::runner::container_command::run_container;
use crate::runner::error::RunnerError;
use crate::runner::host_container_lease::refresh_host_container_lease_for_task_activation;
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, RuntimeSessionContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) struct ContainerTaskActivation {
    pub(in crate::runner) system_was_running: bool,
    pub(in crate::runner) refreshed_host_container_lease: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct ActivationRequest<'a> {
    pub(in crate::runner) container_name: Option<&'a str>,
    pub(in crate::runner) repo_override: Option<PathBuf>,
    pub(in crate::runner) route: RuntimeActivationRoute,
    pub(in crate::runner) session_context: RuntimeSessionContext,
}

pub(in crate::runner) fn ensure_container_runtime_prepared(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
) -> Result<bool, RunnerError> {
    validate_policy_runtime(repo_root, policy)?;
    let system_was_running = check_runtime_running_state_stage(repo_root, policy)?;
    ensure_runtime_running_stage(
        system_was_running,
        container_name.map(str::to_owned),
        repo_override,
        run_container,
    )?;

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
    let plan = runtime_activation_plan_from_request(repo_root, policy, request);
    activate_container_runtime_plan_for_task_using(
        &plan,
        policy,
        ensure_container_runtime_prepared,
        crate::runner::system_command::ensure_workspace_permissions_ready,
        ensure_runtime_gateway_readiness_stage,
        refresh_host_container_lease_for_task_activation,
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

fn runtime_activation_plan_from_request(
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

fn runtime_lease_policy(session_context: RuntimeSessionContext) -> RuntimeLeasePolicy {
    match session_context.lease_refresh_policy {
        LeaseRefreshPolicy::RefreshOnActivation => RuntimeLeasePolicy::RefreshOnActivation,
        LeaseRefreshPolicy::SkipRefresh => RuntimeLeasePolicy::Skip,
    }
}

fn activate_container_runtime_plan_for_task_using(
    plan: &RuntimeActivationPlan,
    policy: &EffectiveContainerPolicy,
    ensure_runtime_prepared: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        Option<&str>,
        Option<PathBuf>,
    ) -> Result<bool, RunnerError>,
    ensure_workspace_permissions: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        Option<&str>,
        Option<PathBuf>,
    ) -> Result<(), RunnerError>,
    ensure_gateway_ready: impl FnOnce(&Path, &EffectiveContainerPolicy) -> Result<(), RunnerError>,
    refresh_host_container_lease: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        bool,
    ) -> Result<bool, RunnerError>,
) -> Result<ContainerTaskActivation, RunnerError> {
    let repo_root = plan.request.repo_root.as_path();
    let system_was_running = ensure_runtime_prepared(
        repo_root,
        policy,
        plan.request.container_name.as_deref(),
        plan.request.repo_override.clone(),
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
    let refreshed_host_container_lease = refresh_runtime_lease_stage(
        plan,
        policy,
        system_was_running,
        refresh_host_container_lease,
    )?;
    Ok(ContainerTaskActivation {
        system_was_running,
        refreshed_host_container_lease,
    })
}

#[cfg(test)]
fn runtime_activation_report_for_result(
    plan: RuntimeActivationPlan,
    activation: ContainerTaskActivation,
) -> RuntimeActivationReport {
    plan.report(
        activation.system_was_running,
        RuntimeCleanupResult::NotRequired,
    )
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
        || prepare_runtime_mounts_stage(repo_root, policy),
        || {
            run_runtime_compose_up_stage(repo_root, policy, |repo_root, policy, args, label| {
                Ok(run_compose_capture(repo_root, policy, args, label).map(|_| ())?)
            })
        },
        || {
            ensure_runtime_exec_readiness_stage(repo_root, policy, &working_dir)?;
            if effigy_core::worktree_scope::load_or_create(repo_root)
                .map_err(|error| {
                    RunnerError::task_invocation(format!("cannot identify checkout scope: {error}"))
                })?
                .is_some()
            {
                verify_live_checkout_mount(repo_root, policy, &working_dir)?;
                refresh_changed_cargo_inputs(repo_root, policy, &working_dir)?;
            }
            Ok(())
        },
        || reconcile_runtime_aliases_stage(repo_root, policy),
    )
}

fn refresh_changed_cargo_inputs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
) -> Result<(), RunnerError> {
    refresh_changed_cargo_inputs_using(repo_root, |changed| {
        for batch in changed.chunks(128) {
            let workdir = working_dir.to_string_lossy();
            let mut args = vec![
                "exec".to_owned(),
                "-T".to_owned(),
                "-w".to_owned(),
                workdir.into_owned(),
                policy.primary_service.clone(),
                "touch".to_owned(),
                "-c".to_owned(),
                "--".to_owned(),
            ];
            args.extend(batch.iter().map(|path| path.to_string_lossy().into_owned()));
            run_compose_capture(
                repo_root,
                policy,
                &compose_args(policy, args.iter().map(String::as_str)),
                "refresh changed Cargo inputs",
            )
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "cannot refresh changed Cargo inputs in selected checkout: {error}"
                ))
            })?;
        }
        Ok(())
    })
}

fn refresh_changed_cargo_inputs_using(
    repo_root: &Path,
    touch_in_container: impl FnOnce(&[PathBuf]) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "-z", "HEAD", "--"])
        .current_dir(repo_root)
        .output()
        .map_err(|error| {
            RunnerError::task_invocation(format!("cannot list changed Cargo inputs: {error}"))
        })?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "cannot list changed Cargo inputs in {}: {}",
            repo_root.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let changed = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| std::path::PathBuf::from(String::from_utf8_lossy(bytes).as_ref()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "rs")
                || matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some("Cargo.toml" | "Cargo.lock")
                )
        })
        .filter(|path| repo_root.join(path).is_file())
        .collect::<Vec<_>>();
    if changed.is_empty() {
        return Ok(());
    }
    touch_in_container(&changed)
}

/// A running Compose service can be healthy while its bind mount still points
/// at an earlier checkout. Prove the selected checkout is visible before a
/// task lease is refreshed or any user command runs.
fn verify_live_checkout_mount(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
) -> Result<(), RunnerError> {
    verify_live_checkout_mount_using(
        repo_root,
        working_dir,
        |probe_path| {
            let workdir = working_dir.to_string_lossy();
            let args = compose_args(
                policy,
                [
                    "exec",
                    "-T",
                    "-w",
                    workdir.as_ref(),
                    policy.primary_service.as_str(),
                    "cat",
                    probe_path,
                ],
            );
            let output = run_compose_capture(repo_root, policy, &args, "checkout mount read probe")
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            Ok(output.stdout)
        },
        |probe_path| {
            let workdir = working_dir.to_string_lossy();
            let args = compose_args(
                policy,
                [
                    "exec",
                    "-T",
                    "-w",
                    workdir.as_ref(),
                    policy.primary_service.as_str(),
                    "touch",
                    "--",
                    probe_path,
                ],
            );
            run_compose_capture(repo_root, policy, &args, "checkout mount write probe")
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            Ok(())
        },
    )
}

fn verify_live_checkout_mount_using(
    repo_root: &Path,
    working_dir: &Path,
    read_in_container: impl FnOnce(&str) -> Result<Vec<u8>, RunnerError>,
    write_in_container: impl FnOnce(&str) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    use std::io::Write;
    // The probe must live in the checkout, at the same relative location as
    // the container working directory. A fresh name also defeats stale copies.
    let name = format!(
        ".effigy-checkout-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
            .as_nanos()
    );
    let host_path = repo_root.join(&name);
    let write_name = format!("{name}-write");
    let host_write_path = repo_root.join(&write_name);
    let payload = format!("{}:{name}", repo_root.display());
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&host_path)
        .map_err(|error| {
            RunnerError::task_invocation(format!("cannot create checkout mount probe: {error}"))
        })?;
    let result = (|| {
        file.write_all(payload.as_bytes()).map_err(|error| {
            RunnerError::task_invocation(format!("cannot write checkout mount probe: {error}"))
        })?;
        file.sync_all().map_err(|error| {
            RunnerError::task_invocation(format!("cannot sync checkout mount probe: {error}"))
        })?;
        let container_path = working_dir.join(&name);
        let actual = read_in_container(&container_path.to_string_lossy())?;
        if actual != payload.as_bytes() {
            return Err(RunnerError::task_invocation(format!(
                "container checkout mount at {} does not match selected checkout {}; refusing task execution",
                working_dir.display(), repo_root.display()
            )));
        }
        write_in_container(&working_dir.join(&write_name).to_string_lossy())?;
        if !host_write_path.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "container checkout mount at {} did not write to selected checkout {}; refusing task execution",
                working_dir.display(), repo_root.display()
            )));
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&host_path);
    let _ = std::fs::remove_file(&host_write_path);
    result.map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot verify container checkout mount for {}: {error}",
            repo_root.display()
        ))
    })
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
        if let Ok(output) = run_compose_capture(
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
        Ok(run_compose_capture(repo_root, policy, args, label)?)
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
    let runtime_data_root = repo_root.join(".effigy/runtime/data");
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
            let bind_roots = volumes
                .iter()
                .filter_map(|volume| volume.as_str())
                .filter_map(|spec| bind_mount_root(repo_root, spec))
                .collect::<Vec<_>>();
            for volume in volumes {
                let Some(spec) = volume.as_str() else {
                    continue;
                };
                for host_path in candidate_host_mount_paths(repo_root, spec, &bind_roots) {
                    if let Some(extension) = host_path.extension() {
                        let ext_str = extension.to_string_lossy();
                        if matches!(
                            ext_str.as_ref(),
                            "conf" | "yml" | "yaml" | "toml" | "json" | "sql" | "ini" | "env"
                        ) {
                            continue;
                        }
                    }
                    let _ = std::fs::create_dir_all(&host_path);
                    #[cfg(unix)]
                    if host_path.starts_with(&runtime_data_root) {
                        let _ = relax_runtime_data_permissions(&host_path);
                    }
                }
            }
        }
    }
    Ok(())
}

fn candidate_host_mount_paths(
    repo_root: &Path,
    spec: &str,
    bind_roots: &[(PathBuf, String)],
) -> Vec<PathBuf> {
    if let Some(host_path) = parse_bind_mount_host_path(spec) {
        let host_path = Path::new(host_path);
        if host_path.is_absolute() && host_path.starts_with(repo_root) {
            return vec![host_path.to_path_buf()];
        }
        return Vec::new();
    }

    let Some((_source, target, _options)) = parse_mount_parts(spec) else {
        return Vec::new();
    };
    let target_path = Path::new(target);
    let Some((bind_source, _bind_target, suffix)) = bind_roots
        .iter()
        .filter_map(|(source, mounted_target)| {
            target_path
                .strip_prefix(Path::new(mounted_target))
                .ok()
                .map(|suffix| (source, mounted_target, suffix))
        })
        .max_by_key(|(_source, mounted_target, _suffix)| mounted_target.len())
    else {
        return Vec::new();
    };

    if !bind_source.starts_with(repo_root) {
        return Vec::new();
    }
    vec![bind_source.join(suffix)]
}

fn bind_mount_root(repo_root: &Path, spec: &str) -> Option<(PathBuf, String)> {
    let host_path = parse_bind_mount_host_path(spec)?;
    let (source, target, _options) = parse_mount_parts(spec)?;
    let host_path = Path::new(host_path);
    if !host_path.is_absolute() || !host_path.starts_with(repo_root) {
        return None;
    }
    Some((PathBuf::from(source), target.to_owned()))
}

#[cfg(unix)]
fn relax_runtime_data_permissions(root: &Path) -> std::io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    relax_runtime_data_permissions_entry(root)?;
    if root.is_dir() {
        for entry in walk_runtime_data(root)? {
            relax_runtime_data_permissions_entry(&entry)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn walk_runtime_data(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut discovered = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path.clone());
            }
            discovered.push(path);
        }
    }
    Ok(discovered)
}

#[cfg(unix)]
fn relax_runtime_data_permissions_entry(path: &Path) -> std::io::Result<()> {
    let metadata = fs::metadata(path)?;
    let mode = if metadata.is_dir() { 0o777 } else { 0o666 };
    let permissions = fs::Permissions::from_mode(mode);
    fs::set_permissions(path, permissions)
}

fn parse_bind_mount_host_path(spec: &str) -> Option<&str> {
    let host = spec.split(':').next()?;
    if host.starts_with('/') || host.starts_with('.') || host.starts_with('~') {
        Some(host)
    } else {
        None
    }
}

fn parse_mount_parts(spec: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = spec.splitn(3, ':');
    let source = parts.next()?.trim();
    let target = parts.next()?.trim();
    let options = parts.next().map(str::trim);
    Some((source, target, options))
}

#[cfg(test)]
mod tests;
