use std::ffi::OsString;
use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{reset_commands, DockerCommand, VolumeClassification};
use effigy_containers::{
    compose::{resolve_compose_backend, ComposeBackend},
    health::wait_for_ready,
    EffectiveContainerPolicy,
};
use serde_json::json;

use super::gateway_registration::RegisteredGatewayRoute;
use super::RunnerError;

pub(super) fn wait_for_container_ready(
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

pub(super) fn rewrite_manifest_for_ejected_compose(
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

pub(super) fn annotate_registered_gateway_routes(
    report: &mut effigy_containers::ContainerCommandReport,
    routes: &[RegisteredGatewayRoute],
) {
    if routes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_routes".to_owned(),
            json!(routes
                .iter()
                .map(|route| json!({
                    "action": "registered",
                    "domain": route.domain,
                    "target": route.target,
                    "tls": route.tls,
                }))
                .collect::<Vec<_>>()),
        );
    }
    for route in routes {
        report.success_text.push('\n');
        report.success_text.push_str(&format!(
            "[gateway] registered {} -> {}",
            route.domain, route.target
        ));
    }
}

pub(super) fn annotate_removed_gateway_routes(
    report: &mut effigy_containers::ContainerCommandReport,
    domains: &[String],
) {
    if domains.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_routes".to_owned(),
            json!(domains
                .iter()
                .map(|domain| json!({
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

pub(super) fn ensure_shared_services_running(
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

pub(super) fn remove_reset_volumes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    classification: &VolumeClassification,
) -> Result<(), RunnerError> {
    for command in reset_commands(classification) {
        run_runtime_volume_capture(repo_root, &policy.profile, &command)?;
    }
    Ok(())
}

pub(super) fn annotate_shared_service_notes(
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

pub(super) fn annotate_left_running_shared_services(
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
            command: format!("{label} ({program} {})", format_args(&args)),
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

pub(super) fn run_runtime_volume_capture(
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
            command: format!("{} ({program} {})", command.description, format_args(&args)),
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

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}
