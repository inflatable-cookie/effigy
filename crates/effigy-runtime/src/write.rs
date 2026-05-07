use std::ffi::OsString;
use std::path::Path;

use effigy_catalog::volumes::classify_for_reset;
use effigy_container_manager::{ContainerAction, ContainerCleanupResult, ContainerRuntimeState};
use effigy_containers::{
    colima::shutdown_compose_commands, down_report, exec::colima_is_running, load_container_policy,
    reset_report, validate_compose_backend_runtime, validate_container_policy,
    EffectiveComposeSource, EffectiveContainerPolicy,
};
use serde_yaml::{Mapping, Value};

use crate::container_manager::{
    compose_invocation_plan, compose_invocation_plan_from_args, lifecycle_operation_report,
    runtime_invocation_plan,
};
use crate::read::{discover_running_environments, filter_running_environments_for_scope};
use crate::signals::run_compose_plan_capture;
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
        shutdown_container_with_manager_plan(repo_root, &policy)?;
    }
    let _manager_report = lifecycle_operation_report(
        repo_root,
        &policy,
        ContainerAction::Shutdown,
        if colima_running {
            ContainerRuntimeState::Stopped
        } else {
            ContainerRuntimeState::Unknown
        },
        Some(if colima_running {
            ContainerCleanupResult::Completed
        } else {
            ContainerCleanupResult::NotRequested
        }),
    )?;
    let removed_gateway_domains = deregister_gateway_routes(&policy)?;
    let mut report = down_report(&policy, colima_running);
    annotate_left_running_shared_services(&mut report, &policy);
    annotate_removed_gateway_routes(&mut report, &removed_gateway_domains);
    Ok(render_container_report(report, output_json))
}

pub fn run_container_down_all<F>(
    output_json: bool,
    deregister_gateway_routes: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
{
    run_container_down_all_with_hook(
        output_json,
        deregister_gateway_routes,
        |_repo_root, _policy| {},
    )
}

pub fn run_container_down_all_with_hook<F, H>(
    output_json: bool,
    deregister_gateway_routes: F,
    pre_shutdown: H,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    H: Fn(&Path, &EffectiveContainerPolicy),
{
    let environments = discover_running_environments()?;
    let mut stopped = Vec::new();

    for environment in environments {
        let repo_root = Path::new(&environment.repo_root);
        let policy = environment.policy;
        let colima_running = colima_is_running(&policy, repo_root)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
        // Run the pre-shutdown hook before we touch compose so any
        // host-side supervisors stop racing with the compose-down.
        pre_shutdown(repo_root, &policy);
        if colima_running {
            shutdown_container_with_manager_plan(repo_root, &policy)?;
        }
        let _manager_report = lifecycle_operation_report(
            repo_root,
            &policy,
            ContainerAction::Shutdown,
            if colima_running {
                ContainerRuntimeState::Stopped
            } else {
                ContainerRuntimeState::Unknown
            },
            Some(if colima_running {
                ContainerCleanupResult::Completed
            } else {
                ContainerCleanupResult::NotRequested
            }),
        )?;
        let removed_gateway_domains = deregister_gateway_routes(&policy)?;
        stopped.push(StoppedContainerEnvironment {
            repo_root: environment.repo_root,
            container: policy.name.clone(),
            project_name: policy.project_name.clone(),
            profile: policy.profile.clone(),
            removed_gateway_domains,
            left_running_shared_services: policy
                .shared_services
                .iter()
                .map(|service| service.service_name.clone())
                .collect(),
            runtime_was_running: colima_running,
        });
    }

    render_container_down_all_report(&stopped, output_json)
}

pub fn run_container_down_under_path_with_hook<F, H>(
    scope_root: &Path,
    name: Option<&str>,
    output_json: bool,
    deregister_gateway_routes: F,
    pre_shutdown: H,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&EffectiveContainerPolicy) -> Result<Vec<String>, EffigyRuntimeError>,
    H: Fn(&Path, &EffectiveContainerPolicy),
{
    let environments =
        filter_running_environments_for_scope(discover_running_environments()?, scope_root, name);
    let mut stopped = Vec::new();

    for environment in environments {
        let repo_root = Path::new(&environment.repo_root);
        let policy = environment.policy;
        let colima_running = colima_is_running(&policy, repo_root)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
        pre_shutdown(repo_root, &policy);
        if colima_running {
            shutdown_container_with_manager_plan(repo_root, &policy)?;
        }
        let _manager_report = lifecycle_operation_report(
            repo_root,
            &policy,
            ContainerAction::Shutdown,
            if colima_running {
                ContainerRuntimeState::Stopped
            } else {
                ContainerRuntimeState::Unknown
            },
            Some(if colima_running {
                ContainerCleanupResult::Completed
            } else {
                ContainerCleanupResult::NotRequested
            }),
        )?;
        let removed_gateway_domains = deregister_gateway_routes(&policy)?;
        stopped.push(StoppedContainerEnvironment {
            repo_root: environment.repo_root,
            container: policy.name.clone(),
            project_name: policy.project_name.clone(),
            profile: policy.profile.clone(),
            removed_gateway_domains,
            left_running_shared_services: policy
                .shared_services
                .iter()
                .map(|service| service.service_name.clone())
                .collect(),
            runtime_was_running: colima_running,
        });
    }

    render_container_down_all_report(&stopped, output_json)
}

pub fn run_container_reset<FDeregister, FRemoveVolumes>(
    repo_root: &Path,
    name: Option<&str>,
    _keep_data: bool,
    wipe_data: bool,
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
    if _keep_data && wipe_data {
        return Err(EffigyRuntimeError::task_invocation(
            "`reset` does not accept both `--keep-data` and `--wipe-data`",
        ));
    }
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let colima_running = colima_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let preserve_persistent_data = !wipe_data;
    let volume_actions = if policy.compose_source == EffectiveComposeSource::Generated {
        Some(classify_for_reset(
            &policy.managed_volumes,
            preserve_persistent_data,
        ))
    } else {
        None
    };
    if colima_running {
        if preserve_persistent_data {
            let plan = compose_invocation_plan(
                repo_root,
                &policy,
                ["down", "--remove-orphans"],
                ContainerAction::Shutdown,
                "docker compose down",
            )?;
            run_compose_plan_capture(&policy, &plan)?;
            if let Some(classification) = volume_actions.as_ref() {
                remove_reset_volumes(repo_root, &policy, classification)?;
            }
        } else {
            let plan = compose_invocation_plan(
                repo_root,
                &policy,
                ["down", "-v", "--remove-orphans"],
                ContainerAction::Shutdown,
                "docker compose down -v",
            )?;
            run_compose_plan_capture(&policy, &plan)?;
        }
    }
    remove_generated_runtime_artifacts(repo_root, &policy)?;
    remove_generated_service_images(repo_root, &policy)?;
    let removed_gateway_domains = deregister_gateway_routes(&policy)?;
    let mut report = reset_report(
        &policy,
        colima_running,
        preserve_persistent_data,
        wipe_data,
        volume_actions.as_ref(),
    );
    annotate_left_running_shared_services(&mut report, &policy);
    annotate_removed_gateway_routes(&mut report, &removed_gateway_domains);
    Ok(render_container_report(report, output_json))
}

fn shutdown_container_with_manager_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), EffigyRuntimeError> {
    for (args, label) in shutdown_compose_commands(policy) {
        let plan = compose_invocation_plan_from_args(
            repo_root,
            policy,
            args,
            ContainerAction::Shutdown,
            label,
        )?;
        run_compose_plan_capture(policy, &plan)?;
    }
    Ok(())
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
    let docker_args = vec![
        OsString::from("image"),
        OsString::from("rm"),
        OsString::from("-f"),
        OsString::from(image_ref),
    ];
    let plan = runtime_invocation_plan(
        repo_root,
        policy,
        "docker",
        &docker_args,
        ContainerAction::Shutdown,
        &format!("remove generated image `{image_ref}`"),
    )?;

    let output = std::process::Command::new(&plan.program)
        .current_dir(repo_root)
        .args(&plan.args)
        .output()
        .map_err(|error| EffigyRuntimeError::TaskCommandLaunch {
            command: format!(
                "remove generated image `{image_ref}` ({} {})",
                plan.program.to_string_lossy(),
                format_os_args(&plan.args)
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

struct StoppedContainerEnvironment {
    repo_root: String,
    container: String,
    project_name: String,
    profile: String,
    removed_gateway_domains: Vec<String>,
    left_running_shared_services: Vec<String>,
    runtime_was_running: bool,
}

fn render_container_down_all_report(
    stopped: &[StoppedContainerEnvironment],
    output_json: bool,
) -> Result<String, EffigyRuntimeError> {
    if output_json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "schema": "effigy.container.down-all.v1",
            "schema_version": 1,
            "ok": true,
            "count": stopped.len(),
            "environments": stopped.iter().map(|entry| {
                serde_json::json!({
                    "repo_root": entry.repo_root,
                    "container": entry.container,
                    "project_name": entry.project_name,
                    "profile": entry.profile,
                    "runtime_was_running": entry.runtime_was_running,
                    "removed_gateway_domains": entry.removed_gateway_domains,
                    "left_running_shared_services": entry.left_running_shared_services,
                })
            }).collect::<Vec<_>>(),
        }))
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()));
    }

    if stopped.is_empty() {
        return Ok("[ok] no running Effigy-managed container environments found".to_owned());
    }

    let mut lines = vec![format!(
        "[ok] stopped {} running Effigy-managed container environment{}",
        stopped.len(),
        if stopped.len() == 1 { "" } else { "s" }
    )];
    for entry in stopped {
        lines.push(format!(
            "{} ({}) [{}]",
            entry.repo_root, entry.container, entry.profile
        ));
        if !entry.removed_gateway_domains.is_empty() {
            lines.push(format!(
                "[info] removed gateway routes: {}",
                entry.removed_gateway_domains.join(", ")
            ));
        }
        if !entry.left_running_shared_services.is_empty() {
            lines.push(format!(
                "[warn] shared services left running: {}",
                entry.left_running_shared_services.join(", ")
            ));
        }
    }
    Ok(lines.join("\n"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_down_all_report_renders_text_and_json() {
        let stopped = vec![StoppedContainerEnvironment {
            repo_root: "/tmp/alpha".to_owned(),
            container: "web".to_owned(),
            project_name: "alpha-dev".to_owned(),
            profile: "effigy".to_owned(),
            removed_gateway_domains: vec!["alpha.test".to_owned()],
            left_running_shared_services: vec!["db".to_owned()],
            runtime_was_running: true,
        }];

        let text = render_container_down_all_report(&stopped, false).expect("render text report");
        assert!(text.contains("[ok] stopped 1 running Effigy-managed container environment"));
        assert!(text.contains("/tmp/alpha (web) [effigy]"));
        assert!(text.contains("[info] removed gateway routes: alpha.test"));
        assert!(text.contains("[warn] shared services left running: db"));

        let json = render_container_down_all_report(&stopped, true).expect("render json report");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json report");
        assert_eq!(parsed["schema"], "effigy.container.down-all.v1");
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["environments"][0]["repo_root"], "/tmp/alpha");
        assert_eq!(parsed["environments"][0]["container"], "web");
        assert_eq!(
            parsed["environments"][0]["removed_gateway_domains"][0],
            "alpha.test"
        );
        assert_eq!(
            parsed["environments"][0]["left_running_shared_services"][0],
            "db"
        );
    }
}
