use std::collections::BTreeSet;
use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{
    export_volume_command, import_volume_command, inspect_volume_command, list_volumes_command,
    merge_runtime_volume_metadata, parse_inspect_volume_metadata, parse_listed_volume_names,
    DockerCommand, ManagedVolume,
};
use effigy_containers::{
    data_list_report, data_transfer_report, exec::colima_is_running, load_container_policy,
    validate_compose_backend_runtime, validate_container_policy, ContainerCommandReport,
    ContainerDataTransferAction, ContainerDataVolumeEntry, EffectiveComposeSource,
    EffectiveContainerPolicy,
};

use crate::EffigyRuntimeError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredGatewayRoute {
    pub domain: String,
    pub target: Option<String>,
    pub dns_ip: Option<std::net::Ipv4Addr>,
    pub tls: bool,
}

pub fn run_container_data_list<F>(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    ensure_generated_data_path(&policy, "list")?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;

    let colima_running = colima_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let volumes = hydrate_managed_volumes(
        repo_root,
        &policy,
        colima_running,
        &run_runtime_volume_capture,
    )?
    .into_iter()
    .map(|volume| ContainerDataVolumeEntry {
        name: volume.name,
        service: volume.service,
        persist: volume.persist,
        size_bytes: volume.size_bytes,
        mount_point: volume.mount_point,
    })
    .collect::<Vec<_>>();

    Ok(render_container_report(
        data_list_report(&policy, colima_running, &volumes),
        output_json,
    ))
}

pub fn run_container_data_export<F>(
    repo_root: &Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &Path,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    run_container_data_transfer(
        repo_root,
        name,
        volume_name,
        archive_path,
        output_json,
        ContainerDataTransferAction::Export,
        run_runtime_volume_capture,
    )
}

pub fn run_container_data_import<F>(
    repo_root: &Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &Path,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    run_container_data_transfer(
        repo_root,
        name,
        volume_name,
        archive_path,
        output_json,
        ContainerDataTransferAction::Import,
        run_runtime_volume_capture,
    )
}

fn run_container_data_transfer<F>(
    repo_root: &Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &Path,
    output_json: bool,
    action: ContainerDataTransferAction,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    ensure_generated_data_path(
        &policy,
        match action {
            ContainerDataTransferAction::Export => "export",
            ContainerDataTransferAction::Import => "import",
        },
    )?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_transfer_path(archive_path, action)?;
    if !colima_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?
    {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "Colima profile `{}` is not running for container `{}`",
            policy.profile, policy.name
        )));
    }

    let volume = resolve_managed_volume(&policy, volume_name)?;
    run_volume_transfer(
        repo_root,
        &policy,
        &volume.name,
        archive_path,
        action,
        &run_runtime_volume_capture,
    )?;
    let report = data_transfer_report(
        &policy,
        action,
        &ContainerDataVolumeEntry {
            name: volume.name,
            service: volume.service,
            persist: volume.persist,
            size_bytes: volume.size_bytes,
            mount_point: volume.mount_point,
        },
        archive_path,
    );
    Ok(render_container_report(report, output_json))
}

pub fn run_container_data_pull_production<FShared, FReady, FGateway, FHook>(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
    ensure_shared_services_running: FShared,
    wait_for_container_ready: FReady,
    register_gateway_routes: FGateway,
    execute_pull_production_hook: FHook,
) -> Result<String, EffigyRuntimeError>
where
    FShared: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    FReady: Fn(&EffectiveContainerPolicy) -> Result<Option<&'static str>, EffigyRuntimeError>,
    FGateway: Fn(
        &Path,
        &EffectiveContainerPolicy,
    ) -> Result<Vec<RegisteredGatewayRoute>, EffigyRuntimeError>,
    FHook: Fn(&Path, &EffectiveContainerPolicy, &str) -> Result<(), EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    ensure_generated_data_path(&policy, "pull-production")?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let hook = policy.pull_production_hook.clone().ok_or_else(|| {
        EffigyRuntimeError::task_invocation(format!(
            "container `{}` does not declare `[containers.{}.data].pull_production`",
            policy.name, policy.name
        ))
    })?;

    let colima_started = effigy_containers::exec::ensure_colima_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let shared_service_notes = ensure_shared_services_running(&policy)?;
    crate::signals::run_docker_capture(
        repo_root,
        &policy,
        &effigy_containers::compose::compose_up_args(&policy),
        "docker compose up",
    )?;
    let health = wait_for_container_ready(&policy)?;
    let gateway_routes = register_gateway_routes(repo_root, &policy)?;
    execute_pull_production_hook(repo_root, &policy, &hook)?;

    let mut report = effigy_containers::data_pull_production_report(
        &policy,
        &effigy_containers::ContainerDataHookResult { hook },
        colima_started,
        health,
    );
    annotate_shared_service_notes(&mut report, &shared_service_notes);
    annotate_registered_gateway_routes(&mut report, &gateway_routes);
    annotate_warning_lines(
        &mut report,
        &effigy_containers::exec::colima_profile_warnings(&policy, repo_root),
    );
    Ok(render_container_report(report, output_json))
}

fn ensure_generated_data_path(
    policy: &EffectiveContainerPolicy,
    action: &str,
) -> Result<(), EffigyRuntimeError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "container `{}` uses direct `compose_file` ownership; `data {action}` is only supported on the generated-compose path in this batch",
            policy.name
        )));
    }
    Ok(())
}

fn run_volume_transfer<F>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    volume_name: &str,
    archive_path: &Path,
    action: ContainerDataTransferAction,
    run_runtime_volume_capture: &F,
) -> Result<(), EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let command = match action {
        ContainerDataTransferAction::Export => export_volume_command(volume_name, archive_path),
        ContainerDataTransferAction::Import => import_volume_command(volume_name, archive_path),
    };
    run_runtime_volume_capture(repo_root, &policy.profile, &command)?;
    Ok(())
}

fn hydrate_managed_volumes<F>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_running: bool,
    run_runtime_volume_capture: &F,
) -> Result<Vec<ManagedVolume>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let mut volumes = policy.managed_volumes.clone();
    volumes.sort_by(|left, right| left.name.cmp(&right.name));
    if !colima_running || volumes.is_empty() {
        return Ok(volumes);
    }

    let listed = run_runtime_volume_capture(
        repo_root,
        &policy.profile,
        &list_volumes_command(&policy.project_name),
    )?;
    let listed_names = parse_listed_volume_names(String::from_utf8_lossy(&listed.stdout).as_ref())
        .into_iter()
        .collect::<BTreeSet<_>>();

    let mut runtime = Vec::new();
    for volume in &volumes {
        if !listed_names.contains(&volume.name) {
            continue;
        }
        let output = run_runtime_volume_capture(
            repo_root,
            &policy.profile,
            &inspect_volume_command(&volume.name),
        )?;
        if let Some(metadata) =
            parse_inspect_volume_metadata(String::from_utf8_lossy(&output.stdout).as_ref())
        {
            runtime.push(metadata);
        }
    }

    Ok(merge_runtime_volume_metadata(&volumes, &runtime))
}

fn resolve_managed_volume(
    policy: &EffectiveContainerPolicy,
    volume_name: &str,
) -> Result<ManagedVolume, EffigyRuntimeError> {
    let Some(volume) = policy
        .managed_volumes
        .iter()
        .find(|volume| volume.name == volume_name)
        .cloned()
    else {
        let available = policy
            .managed_volumes
            .iter()
            .map(|volume| format!("`{}`", volume.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(EffigyRuntimeError::task_invocation(format!(
            "managed volume `{volume_name}` is not owned by container `{}` (available: {available})",
            policy.name
        )));
    };
    Ok(volume)
}

fn validate_transfer_path(
    archive_path: &Path,
    action: ContainerDataTransferAction,
) -> Result<(), EffigyRuntimeError> {
    match action {
        ContainerDataTransferAction::Export => {
            let parent = archive_path.parent().unwrap_or(Path::new("."));
            if !parent.is_dir() {
                return Err(EffigyRuntimeError::task_invocation(format!(
                    "export path parent directory does not exist: {}",
                    parent.display()
                )));
            }
        }
        ContainerDataTransferAction::Import => {
            if !archive_path.is_file() {
                return Err(EffigyRuntimeError::task_invocation(format!(
                    "import archive not found: {}",
                    archive_path.display()
                )));
            }
        }
    }
    Ok(())
}

fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

fn annotate_registered_gateway_routes(
    report: &mut ContainerCommandReport,
    routes: &[RegisteredGatewayRoute],
) {
    if routes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_routes".to_owned(),
            serde_json::json!(routes
                .iter()
                .map(|route| serde_json::json!({
                    "action": "registered",
                    "domain": route.domain,
                    "target": route.target,
                    "dns_ip": route.dns_ip.map(|value| value.to_string()),
                    "tls": route.tls,
                }))
                .collect::<Vec<_>>()),
        );
    }
    for route in routes {
        report.success_text.push('\n');
        match route.target.as_deref() {
            Some(target) => report.success_text.push_str(&format!(
                "[gateway] registered {} -> {}",
                route.domain, target
            )),
            None => report.success_text.push_str(&format!(
                "[gateway] registered {} -> dns {}",
                route.domain,
                route
                    .dns_ip
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "default".to_owned())
            )),
        }
    }
}

fn annotate_shared_service_notes(report: &mut ContainerCommandReport, notes: &[String]) {
    if notes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            serde_json::json!({
                "action": "ensured",
                "services": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[shared] {note}"));
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
