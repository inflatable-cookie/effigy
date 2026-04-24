use std::ffi::OsString;
use std::path::Path;

use effigy_catalog::volumes::classify_for_reset;
use effigy_containers::{
    compose::{compose_args, resolve_compose_backend, ComposeBackend},
    down_report,
    exec::{colima_is_running, shutdown_container as shutdown_container_via_exec},
    load_container_policy, reset_report, validate_compose_backend_runtime,
    validate_container_policy, EffectiveComposeSource, EffectiveContainerPolicy,
};
use serde_yaml::{Mapping, Value};

use crate::signals::run_docker_capture;
use crate::EffigyRuntimeError;

pub fn run_container_down<F>(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
    deregister_gateway_routes: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let colima_running = colima_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    if colima_running {
        shutdown_container_via_exec(repo_root, &policy)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    }
    let removed_gateway_domains = deregister_gateway_routes(&policy)?;
    let mut report = down_report(&policy, colima_running);
    annotate_left_running_shared_services(&mut report, &policy);
    annotate_removed_gateway_routes(&mut report, &removed_gateway_domains);
    Ok(render_container_report(report, output_json))
}

pub fn run_container_reset<FDeregister, FRemoveVolumes>(
    repo_root: &Path,
    name: Option<&str>,
    keep_data: bool,
    output_json: bool,
    deregister_gateway_routes: FDeregister,
    remove_reset_volumes: FRemoveVolumes,
) -> Result<String, EffigyRuntimeError>
where
    FDeregister: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    FRemoveVolumes: Fn(
        &Path,
        &EffectiveContainerPolicy,
        &effigy_catalog::volumes::VolumeClassification,
    ) -> Result<(), EffigyRuntimeError>,
{
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    if keep_data && policy.compose_source != EffectiveComposeSource::Generated {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "container `{}` uses direct `compose_file` ownership; `reset --keep-data` is only supported on the generated-compose path in this batch",
            policy.name
        )));
    }
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let colima_running = colima_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
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
    remove_generated_runtime_artifacts(repo_root, &policy)?;
    remove_generated_service_images(repo_root, &policy)?;
    let removed_gateway_domains = deregister_gateway_routes(&policy)?;
    let mut report = reset_report(&policy, colima_running, keep_data, volume_actions.as_ref());
    annotate_left_running_shared_services(&mut report, &policy);
    annotate_removed_gateway_routes(&mut report, &removed_gateway_domains);
    Ok(render_container_report(report, output_json))
}

fn remove_generated_runtime_artifacts(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), EffigyRuntimeError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Ok(());
    }

    let runtime_dir = repo_root.join(".effigy/runtime/compose");
    if !runtime_dir.exists() {
        return Ok(());
    }

    std::fs::remove_dir_all(&runtime_dir).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!(
            "failed to remove generated runtime directory {}: {error}",
            runtime_dir.display()
        ))
    })
}

fn remove_generated_service_images(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), EffigyRuntimeError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Ok(());
    }

    let Some(compose_file) = policy.compose_files.first() else {
        return Ok(());
    };
    if !compose_file.exists() {
        return Ok(());
    }

    let image_refs = load_generated_service_image_refs(compose_file, &policy.project_name)?;
    for image_ref in image_refs {
        remove_runtime_image_allow_missing(repo_root, policy, &image_ref)?;
    }
    Ok(())
}

fn load_generated_service_image_refs(
    compose_file: &Path,
    project_name: &str,
) -> Result<Vec<String>, EffigyRuntimeError> {
    let content = std::fs::read_to_string(compose_file).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!(
            "failed to read generated compose file {} while resolving built images: {error}",
            compose_file.display()
        ))
    })?;
    let parsed: Value = serde_yaml::from_str(&content).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!(
            "failed to parse generated compose file {} while resolving built images: {error}",
            compose_file.display()
        ))
    })?;
    Ok(select_generated_service_image_refs(&parsed, project_name))
}

pub fn select_generated_service_image_refs(parsed: &Value, project_name: &str) -> Vec<String> {
    let Some(services) = parsed.get("services").and_then(Value::as_mapping) else {
        return Vec::new();
    };

    services
        .iter()
        .filter_map(|(service_name, service_def)| {
            let service_name = service_name.as_str()?;
            let service_def: &Mapping = service_def.as_mapping()?;
            service_def.get("build")?;
            if let Some(image) = service_def
                .get("image")
                .and_then(Value::as_str)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
            {
                return Some(image.to_owned());
            }
            Some(format!("{project_name}-{service_name}:latest"))
        })
        .collect()
}

fn remove_runtime_image_allow_missing(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    image_ref: &str,
) -> Result<(), EffigyRuntimeError> {
    let (program, args) = match resolve_compose_backend() {
        ComposeBackend::Docker => (
            "docker",
            vec![
                OsString::from("image"),
                OsString::from("rm"),
                OsString::from("-f"),
                OsString::from(image_ref),
            ],
        ),
        ComposeBackend::ColimaNerdctl => (
            "colima",
            vec![
                OsString::from("nerdctl"),
                OsString::from("--profile"),
                OsString::from(policy.profile.as_str()),
                OsString::from("--"),
                OsString::from("image"),
                OsString::from("rm"),
                OsString::from("-f"),
                OsString::from(image_ref),
            ],
        ),
    };

    let output = std::process::Command::new(program)
        .current_dir(repo_root)
        .args(&args)
        .output()
        .map_err(|error| EffigyRuntimeError::TaskCommandLaunch {
            command: format!(
                "remove generated image `{image_ref}` ({program} {})",
                format_os_args(&args)
            ),
            error,
        })?;
    if output.status.success() || image_remove_failure_is_missing(&output) {
        return Ok(());
    }
    Err(EffigyRuntimeError::task_invocation(format!(
        "failed to remove generated image `{image_ref}` (code {:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )))
}

fn image_remove_failure_is_missing(output: &std::process::Output) -> bool {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    combined.contains("no such image")
        || combined.contains("not found")
        || combined.contains("no such object")
}

fn format_os_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_container_report(
    report: effigy_containers::ContainerCommandReport,
    output_json: bool,
) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

fn annotate_removed_gateway_routes(
    report: &mut effigy_containers::ContainerCommandReport,
    domains: &[String],
) {
    if domains.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_routes".to_owned(),
            serde_json::json!(domains
                .iter()
                .map(|domain| serde_json::json!({
                    "action": "removed",
                    "domain": domain,
                }))
                .collect::<Vec<_>>()),
        );
    }
    for domain in domains {
        report.success_text.push('\n');
        report
            .success_text
            .push_str(&format!("[gateway] removed {domain}"));
    }
}

fn annotate_left_running_shared_services(
    report: &mut effigy_containers::ContainerCommandReport,
    policy: &EffectiveContainerPolicy,
) {
    if policy.shared_services.is_empty() {
        return;
    }
    let notes = policy
        .shared_services
        .iter()
        .map(|service| {
            format!(
                "{} [{}] left running at {}:{}",
                service.service_name, service.catalog, service.host, service.host_port
            )
        })
        .collect::<Vec<_>>();
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            serde_json::json!({
                "action": "left-running",
                "services": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[shared] {note}"));
    }
}
