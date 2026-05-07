mod cache;
mod planning;
mod report;
mod transfer;
mod volume_io;
mod volumes;

use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{remove_volume_command, DockerCommand, ManagedVolume};
use effigy_container_manager::ContainerAction;
use effigy_container_ops::{ContainerCacheOperation, ContainerDataOperation};
use effigy_containers::{
    cache_list_all_report, cache_list_report, cache_prune_report, data_list_report,
    data_transfer_report,
    exec::{runtime_backend_is_running, selected_backend_label},
    load_container_policy, user_global_colima_profile, validate_compose_backend_runtime,
    validate_container_policy, volume_list_report, ContainerCachePruneEntry,
    ContainerCacheVolumeEntry, ContainerDataTransferAction, ContainerDataVolumeEntry,
    EffectiveContainerPolicy,
};

use crate::EffigyRuntimeError;
use cache::{collect_global_cache_entries, project_is_running};
use planning::{cache_kind_label, cache_scope_label, ensure_cache_prune_target_is_stopped};
pub use planning::{cache_operation_plan, data_operation_plan, global_cache_operation_plan};
pub use report::RegisteredGatewayRoute;
use report::{
    annotate_registered_gateway_routes, annotate_shared_service_notes, annotate_warning_lines,
    render_container_report,
};
use transfer::{ensure_generated_data_path, resolve_managed_volume, validate_transfer_path};
use volume_io::{hydrate_managed_volumes, run_volume_transfer};
use volumes::collect_global_volume_entries;

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
    let _operation_plan = data_operation_plan(repo_root, &policy, ContainerDataOperation::list());
    ensure_generated_data_path(&policy, "list")?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;

    let colima_running = runtime_backend_is_running(&policy, repo_root)
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

pub fn run_container_cache_list<F>(
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
    let _operation_plan = cache_operation_plan(
        repo_root,
        &policy,
        ContainerCacheOperation::list(false, None, None),
    );
    ensure_generated_data_path(&policy, "cache list")?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;

    let colima_running = runtime_backend_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let caches = hydrate_managed_volumes(
        repo_root,
        &policy,
        colima_running,
        &run_runtime_volume_capture,
    )?
    .into_iter()
    .filter_map(cache_volume_entry)
    .collect::<Vec<_>>();

    Ok(render_container_report(
        cache_list_report(&policy, colima_running, &caches),
        output_json,
    ))
}

pub fn run_container_cache_list_under_path<F>(
    scope_root: &Path,
    name: Option<&str>,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError> + Copy,
{
    let mut rendered = Vec::new();
    for repo_root in crate::read::discover_effigy_repos_under(scope_root) {
        match run_container_cache_list(&repo_root, name, output_json, run_runtime_volume_capture) {
            Ok(output) => rendered.push(output),
            Err(error) => {
                rendered.push(format!("[warn] skipped `{}`: {error}", repo_root.display()))
            }
        }
    }
    if rendered.is_empty() {
        return Ok(format!(
            "[info] no Effigy repos found under {}",
            scope_root.display()
        ));
    }
    Ok(rendered.join("\n"))
}

pub fn run_container_cache_list_all<F>(
    cwd: &Path,
    profile: Option<&str>,
    project_filter: Option<&str>,
    kind_filter: Option<&str>,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let resolved_profile = profile
        .map(str::to_owned)
        .or_else(user_global_colima_profile)
        .unwrap_or_else(|| "effigy".to_owned());
    let _operation_plan = global_cache_operation_plan(
        cwd,
        &resolved_profile,
        ContainerCacheOperation::list(
            true,
            project_filter.map(str::to_owned),
            kind_filter.map(str::to_owned),
        ),
    );
    let caches = collect_global_cache_entries(cwd, &resolved_profile, &run_runtime_volume_capture)?
        .into_iter()
        .filter(|cache| {
            project_filter.is_none_or(|project| cache.project_name.as_deref() == Some(project))
                && kind_filter.is_none_or(|kind| cache.kind == kind)
        })
        .collect::<Vec<_>>();
    let scope_label =
        cache_scope_label("profile-wide cache inventory", project_filter, kind_filter);

    Ok(render_container_report(
        cache_list_all_report(&resolved_profile, &scope_label, &caches),
        output_json,
    ))
}

pub fn run_container_cache_prune<F>(
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
    let _operation_plan = cache_operation_plan(
        repo_root,
        &policy,
        ContainerCacheOperation::prune(false, None, None, false),
    );
    ensure_generated_data_path(&policy, "cache prune")?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;

    ensure_cache_prune_target_is_stopped(
        &policy.name,
        project_is_running(repo_root, &policy.project_name)?,
    )?;

    let volumes = hydrate_managed_volumes(repo_root, &policy, false, &run_runtime_volume_capture)?;
    let mut entries = Vec::new();
    for volume in volumes {
        let Some(kind) = volume.cache_kind() else {
            continue;
        };
        run_runtime_volume_capture(
            repo_root,
            &policy.profile,
            &remove_volume_command(&volume.name),
        )?;
        entries.push(ContainerCachePruneEntry {
            name: volume.name,
            kind: cache_kind_label(kind).to_owned(),
            size_bytes: volume.size_bytes,
            project_name: Some(policy.project_name.clone()),
            removed: true,
            in_use: false,
        });
    }

    Ok(render_container_report(
        cache_prune_report(&format!("container `{}`", policy.name), &entries),
        output_json,
    ))
}

pub fn run_container_cache_prune_all<F>(
    cwd: &Path,
    profile: Option<&str>,
    project_filter: Option<&str>,
    kind_filter: Option<&str>,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let resolved_profile = profile
        .map(str::to_owned)
        .or_else(user_global_colima_profile)
        .unwrap_or_else(|| "effigy".to_owned());
    let _operation_plan = global_cache_operation_plan(
        cwd,
        &resolved_profile,
        ContainerCacheOperation::prune(
            true,
            project_filter.map(str::to_owned),
            kind_filter.map(str::to_owned),
            false,
        ),
    );
    let caches = collect_global_cache_entries(cwd, &resolved_profile, &run_runtime_volume_capture)?
        .into_iter()
        .filter(|cache| {
            project_filter.is_none_or(|project| cache.project_name.as_deref() == Some(project))
                && kind_filter.is_none_or(|kind| cache.kind == kind)
        })
        .collect::<Vec<_>>();
    let mut entries = Vec::new();
    for cache in caches {
        let removed = if cache.in_use {
            false
        } else {
            run_runtime_volume_capture(
                cwd,
                &resolved_profile,
                &remove_volume_command(&cache.name),
            )?;
            true
        };
        entries.push(ContainerCachePruneEntry {
            name: cache.name,
            kind: cache.kind,
            size_bytes: cache.size_bytes,
            project_name: cache.project_name,
            removed,
            in_use: cache.in_use,
        });
    }
    let scope_label =
        cache_scope_label("profile-wide cache inventory", project_filter, kind_filter);

    Ok(render_container_report(
        cache_prune_report(&scope_label, &entries),
        output_json,
    ))
}

pub fn run_container_volume_list<F>(
    cwd: &Path,
    orphans_only: bool,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let volumes = collect_global_volume_entries(cwd, orphans_only, &run_runtime_volume_capture)?;
    Ok(render_container_report(
        volume_list_report("global runtime volume inventory", orphans_only, &volumes),
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
    let _operation_plan = data_operation_plan(
        repo_root,
        &policy,
        match action {
            ContainerDataTransferAction::Export => {
                ContainerDataOperation::export(volume_name, archive_path.to_path_buf())
            }
            ContainerDataTransferAction::Import => {
                ContainerDataOperation::import(volume_name, archive_path.to_path_buf())
            }
        },
    );
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
    if !runtime_backend_is_running(&policy, repo_root)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?
    {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "{} runtime is not available for container `{}`",
            selected_backend_label(&policy, repo_root),
            policy.name
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
    let _operation_plan = data_operation_plan(
        repo_root,
        &policy,
        ContainerDataOperation::pull_production(false),
    );
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

    let colima_started =
        effigy_containers::exec::ensure_runtime_backend_running(&policy, repo_root)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let shared_service_notes = ensure_shared_services_running(&policy)?;
    let plan = crate::container_manager::compose_up_invocation_plan(
        repo_root,
        &policy,
        ContainerAction::Activate,
        "docker compose up",
    )?;
    crate::signals::run_compose_plan_capture(&policy, &plan)?;
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

fn cache_volume_entry(volume: ManagedVolume) -> Option<ContainerCacheVolumeEntry> {
    let kind = volume.cache_kind()?;
    Some(ContainerCacheVolumeEntry {
        name: volume.name,
        service: volume.service,
        kind: cache_kind_label(kind).to_owned(),
        size_bytes: volume.size_bytes,
        mount_point: volume.mount_point,
        mount_target: volume.mount_target,
    })
}
