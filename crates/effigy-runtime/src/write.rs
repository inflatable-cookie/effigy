mod planning;
mod report;

use std::path::Path;

use effigy_catalog::volumes::classify_for_reset;
use effigy_containers::{
    down_report, exec::runtime_backend_is_running, load_container_policy, reset_report,
    validate_compose_backend_runtime, validate_container_policy, EffectiveComposeSource,
    EffectiveContainerPolicy,
};
use effigy_containers::{ContainerAction, ContainerCleanupResult, ContainerRuntimeState};

use crate::container_manager::{compose_invocation_plan, lifecycle_operation_report};
use crate::read::{discover_running_environments, filter_running_environments_for_scope};
use crate::signals::run_compose_plan_capture;
use crate::EffigyRuntimeError;
pub use planning::select_generated_service_image_refs;
use planning::{
    remove_generated_runtime_artifacts, remove_generated_service_images,
    shutdown_container_with_manager_plan,
};
use report::{
    annotate_left_running_shared_services, annotate_removed_gateway_routes,
    render_container_down_global_report, render_container_report, StoppedContainerEnvironment,
};

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
    let runtime_running = runtime_backend_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    if runtime_running {
        shutdown_container_with_manager_plan(repo_root, &policy)?;
    }
    let _manager_report = lifecycle_operation_report(
        repo_root,
        &policy,
        ContainerAction::Shutdown,
        if runtime_running {
            ContainerRuntimeState::Stopped
        } else {
            ContainerRuntimeState::Unknown
        },
        Some(if runtime_running {
            ContainerCleanupResult::Completed
        } else {
            ContainerCleanupResult::NotRequested
        }),
    )?;
    let removed_gateway_domains = deregister_gateway_routes(&policy)?;
    let mut report = down_report(&policy, runtime_running);
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
        let runtime_running = runtime_backend_is_running(&policy, repo_root)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
        // Run the pre-shutdown hook before we touch compose so any
        // host-side supervisors stop racing with the compose-down.
        pre_shutdown(repo_root, &policy);
        if runtime_running {
            shutdown_container_with_manager_plan(repo_root, &policy)?;
        }
        let _manager_report = lifecycle_operation_report(
            repo_root,
            &policy,
            ContainerAction::Shutdown,
            if runtime_running {
                ContainerRuntimeState::Stopped
            } else {
                ContainerRuntimeState::Unknown
            },
            Some(if runtime_running {
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
            runtime_was_running: runtime_running,
        });
    }

    render_container_down_global_report(&stopped, output_json)
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
        let runtime_running = runtime_backend_is_running(&policy, repo_root)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
        pre_shutdown(repo_root, &policy);
        if runtime_running {
            shutdown_container_with_manager_plan(repo_root, &policy)?;
        }
        let _manager_report = lifecycle_operation_report(
            repo_root,
            &policy,
            ContainerAction::Shutdown,
            if runtime_running {
                ContainerRuntimeState::Stopped
            } else {
                ContainerRuntimeState::Unknown
            },
            Some(if runtime_running {
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
            runtime_was_running: runtime_running,
        });
    }

    render_container_down_global_report(&stopped, output_json)
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
    let runtime_running = runtime_backend_is_running(&policy, repo_root)
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
    if runtime_running {
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
        runtime_running,
        preserve_persistent_data,
        wipe_data,
        volume_actions.as_ref(),
    );
    annotate_left_running_shared_services(&mut report, &policy);
    annotate_removed_gateway_routes(&mut report, &removed_gateway_domains);
    Ok(render_container_report(report, output_json))
}
