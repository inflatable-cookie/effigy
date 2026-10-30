mod cache;
mod planning;
mod progress;
mod report;
mod transfer;
mod volume_io;
mod volumes;

use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{
    remove_volume_command, DockerCommand, ManagedVolume, VolumeIdentity,
};
use effigy_containers::ContainerAction;
use effigy_containers::{
    annotate_workspace_extra_mount_decisions, cache_list_global_report, cache_list_report,
    cache_prune_report, data_list_report, data_transfer_report,
    exec::{runtime_backend_is_running, selected_backend_label},
    load_container_policy, load_container_policy_for_creation, user_global_colima_profile,
    validate_compose_backend_runtime, validate_container_policy, volume_list_report,
    volume_prune_report, ContainerCachePruneEntry, ContainerCacheVolumeEntry,
    ContainerDataTransferAction, ContainerDataVolumeEntry, ContainerVolumePruneEntry,
    EffectiveContainerPolicy,
};
use effigy_containers::{
    ContainerCacheOperation, ContainerDataOperation, ContainerVolumeOperation,
};

use crate::EffigyRuntimeError;
use cache::{collect_global_cache_entries, project_is_running};
use planning::{cache_kind_label, cache_scope_label, ensure_cache_prune_target_is_stopped};
pub use planning::{
    cache_operation_plan, data_operation_plan, global_cache_operation_plan,
    global_volume_operation_plan,
};
use progress::{CacheVolumeProgress, PROBE_VOLUME_INSPECT, PROBE_VOLUME_REMOVE};
pub use report::RegisteredGatewayRoute;
use report::{
    annotate_registered_gateway_routes, annotate_shared_service_notes, annotate_warning_lines,
    render_container_report,
};
use transfer::{resolve_managed_volume, validate_transfer_path};
pub use volume_io::volume_identities;
use volume_io::{hydrate_managed_volumes, run_volume_transfer};
use volumes::{collect_global_volume_entries, collect_repo_volume_entries};

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
    CacheVolumeProgress::run(PROBE_VOLUME_INSPECT, |progress| {
        list_cache_for_repo(
            repo_root,
            name,
            output_json,
            &run_runtime_volume_capture,
            progress,
        )
    })
}

fn list_cache_for_repo<F>(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
    run_runtime_volume_capture: &F,
    progress: &CacheVolumeProgress,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    progress.update(
        format!("inspecting cache volumes for `{}`", repo_root.display()),
        None,
        None,
    );
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let _operation_plan = cache_operation_plan(
        repo_root,
        &policy,
        ContainerCacheOperation::list(false, None, None),
    );
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
        run_runtime_volume_capture,
    )?
    .into_iter()
    .filter_map(cache_volume_entry)
    .collect::<Vec<_>>();
    progress.update(
        format!("inspecting cache volumes for `{}`", repo_root.display()),
        Some((caches.len() as u64, caches.len() as u64)),
        Some(format!("listed {} cache volumes", caches.len())),
    );

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
    CacheVolumeProgress::run(PROBE_VOLUME_INSPECT, |progress| {
        let repos = crate::read::discover_effigy_repos_under(scope_root);
        let total = repos.len() as u64;
        progress.update(
            "inspecting cache volumes",
            Some((0, total)),
            Some(format!("found {total} repos")),
        );
        let mut rendered = Vec::new();
        for (index, repo_root) in repos.into_iter().enumerate() {
            progress.update(
                format!("inspecting cache volumes for `{}`", repo_root.display()),
                Some((index as u64 + 1, total)),
                None,
            );
            match list_cache_for_repo(
                &repo_root,
                name,
                output_json,
                &run_runtime_volume_capture,
                progress,
            ) {
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
    })
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
    CacheVolumeProgress::run(PROBE_VOLUME_INSPECT, |progress| {
        progress.update("inspecting cache volumes", None, None);
        let resolved_profile = effigy_containers::backend::global_endpoint(profile);
        let _operation_plan = global_cache_operation_plan(
            cwd,
            &resolved_profile,
            ContainerCacheOperation::list(
                true,
                project_filter.map(str::to_owned),
                kind_filter.map(str::to_owned),
            ),
        );
        let caches =
            collect_global_cache_entries(cwd, &resolved_profile, &run_runtime_volume_capture)?
                .into_iter()
                .filter(|cache| {
                    project_filter
                        .is_none_or(|project| cache.project_name.as_deref() == Some(project))
                        && kind_filter.is_none_or(|kind| cache.kind == kind)
                })
                .collect::<Vec<_>>();
        progress.update(
            "inspecting cache volumes",
            Some((caches.len() as u64, caches.len() as u64)),
            Some(format!("listed {} cache volumes", caches.len())),
        );
        let scope_label =
            cache_scope_label("profile-wide cache inventory", project_filter, kind_filter);

        Ok(render_container_report(
            cache_list_global_report(&resolved_profile, &scope_label, &caches),
            output_json,
        ))
    })
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
    CacheVolumeProgress::run(PROBE_VOLUME_INSPECT, |progress| {
        progress.update("inspecting cache volumes", None, None);
        let policy = load_container_policy(repo_root, name)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
        let _operation_plan = cache_operation_plan(
            repo_root,
            &policy,
            ContainerCacheOperation::prune(false, None, None, false),
        );
        validate_container_policy(repo_root, &policy)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
        validate_compose_backend_runtime(repo_root, &policy)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;

        ensure_cache_prune_target_is_stopped(
            &policy.name,
            project_is_running(repo_root, &policy.project_name)?,
        )?;

        let volumes =
            hydrate_managed_volumes(repo_root, &policy, false, &run_runtime_volume_capture)?;
        let endpoint = volume_io::policy_endpoint(repo_root, &policy)?;
        let caches = volumes
            .iter()
            .filter(|volume| volume.cache_kind().is_some())
            .map(|volume| volume.name.clone())
            .collect::<Vec<_>>();
        let identities = volume_identities(
            repo_root,
            &endpoint,
            &policy.project_name,
            &caches,
            &run_runtime_volume_capture,
        )?;
        let cache_total = caches.len() as u64;
        progress.now_waiting_for(PROBE_VOLUME_REMOVE);
        progress.update(
            "removing cache volumes",
            Some((0, cache_total)),
            Some(format!("listed {cache_total} cache volumes")),
        );
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        let mut done = 0u64;
        for volume in volumes {
            let Some(kind) = volume.cache_kind() else {
                continue;
            };
            done += 1;
            match identities.get(&volume.name) {
                Some(VolumeIdentity::Owned) => {}
                Some(VolumeIdentity::Lookalike(reason)) => {
                    warnings.push(format!(
                        "left {reason}; it was not created for this environment"
                    ));
                    progress.update(
                        format!("skipping cache volume `{}`", volume.name),
                        Some((done, cache_total)),
                        Some(format!("left {reason}")),
                    );
                    continue;
                }
                Some(VolumeIdentity::Absent) | None => {
                    progress.update(
                        format!("skipping cache volume `{}`", volume.name),
                        Some((done, cache_total)),
                        Some("volume absent".to_owned()),
                    );
                    continue;
                }
            }
            progress.update(
                format!("removing cache volume `{}`", volume.name),
                Some((done, cache_total)),
                None,
            );
            run_runtime_volume_capture(repo_root, &endpoint, &remove_volume_command(&volume.name))?;
            progress.update(
                format!("removing cache volume `{}`", volume.name),
                Some((done, cache_total)),
                Some(format!("removed `{}`", volume.name)),
            );
            entries.push(ContainerCachePruneEntry {
                name: volume.name,
                kind: cache_kind_label(kind).to_owned(),
                size_bytes: volume.size_bytes,
                project_name: Some(policy.project_name.clone()),
                removed: true,
                in_use: false,
            });
        }

        let mut report = cache_prune_report(&format!("container `{}`", policy.name), &entries);
        annotate_warning_lines(&mut report, &warnings);
        Ok(render_container_report(report, output_json))
    })
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
    CacheVolumeProgress::run(PROBE_VOLUME_INSPECT, |progress| {
        progress.update("inspecting cache volumes", None, None);
        let resolved_profile = effigy_containers::backend::global_endpoint(profile);
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
        let caches =
            collect_global_cache_entries(cwd, &resolved_profile, &run_runtime_volume_capture)?
                .into_iter()
                .filter(|cache| {
                    project_filter
                        .is_none_or(|project| cache.project_name.as_deref() == Some(project))
                        && kind_filter.is_none_or(|kind| cache.kind == kind)
                })
                .collect::<Vec<_>>();
        let total = caches.len() as u64;
        progress.now_waiting_for(PROBE_VOLUME_REMOVE);
        progress.update(
            "removing cache volumes",
            Some((0, total)),
            Some(format!("listed {total} cache volumes")),
        );
        let mut entries = Vec::new();
        for (index, cache) in caches.into_iter().enumerate() {
            let done = index as u64 + 1;
            let name = cache.name.clone();
            if cache.in_use {
                progress.update(
                    format!("skipping cache volume `{name}`"),
                    Some((done, total)),
                    Some(format!("`{name}` in use")),
                );
                entries.push(ContainerCachePruneEntry {
                    name: cache.name,
                    kind: cache.kind,
                    size_bytes: cache.size_bytes,
                    project_name: cache.project_name,
                    removed: false,
                    in_use: true,
                });
                continue;
            }
            progress.update(
                format!("removing cache volume `{name}`"),
                Some((done, total)),
                None,
            );
            run_runtime_volume_capture(cwd, &resolved_profile, &remove_volume_command(&name))?;
            progress.update(
                format!("removing cache volume `{name}`"),
                Some((done, total)),
                Some(format!("removed `{name}`")),
            );
            entries.push(ContainerCachePruneEntry {
                name: cache.name,
                kind: cache.kind,
                size_bytes: cache.size_bytes,
                project_name: cache.project_name,
                removed: true,
                in_use: false,
            });
        }
        let scope_label =
            cache_scope_label("profile-wide cache inventory", project_filter, kind_filter);

        Ok(render_container_report(
            cache_prune_report(&scope_label, &entries),
            output_json,
        ))
    })
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
    CacheVolumeProgress::run(PROBE_VOLUME_INSPECT, |progress| {
        progress.update("inspecting managed volumes", None, None);
        let profile = user_global_colima_profile().unwrap_or_else(|| "effigy".to_owned());
        let _operation_plan = global_volume_operation_plan(
            cwd,
            &profile,
            ContainerVolumeOperation::list(orphans_only, Some(profile.clone())),
        );
        let volumes =
            collect_global_volume_entries(cwd, orphans_only, &run_runtime_volume_capture)?;
        progress.update(
            "inspecting managed volumes",
            Some((volumes.len() as u64, volumes.len() as u64)),
            Some(format!("listed {} volumes", volumes.len())),
        );
        Ok(render_container_report(
            volume_list_report(
                "global runtime volume inventory",
                orphans_only.then_some("orphans"),
                &volumes,
            ),
            output_json,
        ))
    })
}

pub fn run_container_volume_list_for_repo<F>(
    repo_root: &Path,
    orphans_only: bool,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    CacheVolumeProgress::run(PROBE_VOLUME_INSPECT, |progress| {
        progress.update(
            format!("inspecting volumes for `{}`", repo_root.display()),
            None,
            None,
        );
        let profile = user_global_colima_profile().unwrap_or_else(|| "effigy".to_owned());
        let _operation_plan = global_volume_operation_plan(
            repo_root,
            &profile,
            ContainerVolumeOperation::list(orphans_only, Some(profile.clone())),
        );
        let volumes =
            collect_repo_volume_entries(repo_root, orphans_only, &run_runtime_volume_capture)?;
        progress.update(
            format!("inspecting volumes for `{}`", repo_root.display()),
            Some((volumes.len() as u64, volumes.len() as u64)),
            Some(format!("listed {} volumes", volumes.len())),
        );
        Ok(render_container_report(
            volume_list_report(
                &format!("repo volume inventory for {}", repo_root.display()),
                orphans_only.then_some("dormant"),
                &volumes,
            ),
            output_json,
        ))
    })
}

pub fn run_container_volume_prune_global<F>(
    cwd: &Path,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    CacheVolumeProgress::run(PROBE_VOLUME_INSPECT, |progress| {
        progress.update("inspecting managed volumes", None, None);
        let profile = user_global_colima_profile().unwrap_or_else(|| "effigy".to_owned());
        let _operation_plan = global_volume_operation_plan(
            cwd,
            &profile,
            ContainerVolumeOperation::prune(true, Some(profile.clone())),
        );
        let volumes = collect_global_volume_entries(cwd, true, &run_runtime_volume_capture)?;
        let total = volumes.len() as u64;
        progress.now_waiting_for(PROBE_VOLUME_REMOVE);
        progress.update(
            "removing volumes",
            Some((0, total)),
            Some(format!("listed {total} volumes")),
        );
        let mut entries = Vec::new();
        for (index, volume) in volumes.into_iter().enumerate() {
            let done = index as u64 + 1;
            let name = volume.name.clone();
            if volume.in_use {
                progress.update(
                    format!("skipping volume `{name}`"),
                    Some((done, total)),
                    Some(format!("`{name}` in use")),
                );
                entries.push(ContainerVolumePruneEntry {
                    name: volume.name,
                    backend: volume.backend,
                    profile: volume.profile,
                    project_name: volume.project_name,
                    size_bytes: volume.size_bytes,
                    removed: false,
                    in_use: true,
                    orphan_reason: volume.orphan_reason,
                });
                continue;
            }
            progress.update(
                format!("removing volume `{name}`"),
                Some((done, total)),
                None,
            );
            run_runtime_volume_capture(cwd, &volume.profile, &remove_volume_command(&name))?;
            progress.update(
                format!("removing volume `{name}`"),
                Some((done, total)),
                Some(format!("removed `{name}`")),
            );
            entries.push(ContainerVolumePruneEntry {
                name: volume.name,
                backend: volume.backend,
                profile: volume.profile,
                project_name: volume.project_name,
                size_bytes: volume.size_bytes,
                removed: true,
                in_use: false,
                orphan_reason: volume.orphan_reason,
            });
        }
        Ok(render_container_report(
            volume_prune_report("global runtime volume inventory", "orphans", &entries),
            output_json,
        ))
    })
}

pub fn run_container_volume_prune_for_repo<F>(
    repo_root: &Path,
    output_json: bool,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    CacheVolumeProgress::run(PROBE_VOLUME_INSPECT, |progress| {
        progress.update(
            format!("inspecting volumes for `{}`", repo_root.display()),
            None,
            None,
        );
        let profile = user_global_colima_profile().unwrap_or_else(|| "effigy".to_owned());
        let _operation_plan = global_volume_operation_plan(
            repo_root,
            &profile,
            ContainerVolumeOperation::prune(true, Some(profile.clone())),
        );
        let volumes = collect_repo_volume_entries(repo_root, true, &run_runtime_volume_capture)?;
        let total = volumes.len() as u64;
        progress.now_waiting_for(PROBE_VOLUME_REMOVE);
        progress.update(
            "removing volumes",
            Some((0, total)),
            Some(format!("listed {total} volumes")),
        );
        let mut entries = Vec::new();
        for (index, volume) in volumes.into_iter().enumerate() {
            let done = index as u64 + 1;
            let name = volume.name.clone();
            if volume.in_use {
                progress.update(
                    format!("skipping volume `{name}`"),
                    Some((done, total)),
                    Some(format!("`{name}` in use")),
                );
                entries.push(ContainerVolumePruneEntry {
                    name: volume.name,
                    backend: volume.backend,
                    profile: volume.profile,
                    project_name: volume.project_name,
                    size_bytes: volume.size_bytes,
                    removed: false,
                    in_use: true,
                    orphan_reason: volume.orphan_reason,
                });
                continue;
            }
            progress.update(
                format!("removing volume `{name}`"),
                Some((done, total)),
                None,
            );
            run_runtime_volume_capture(repo_root, &volume.profile, &remove_volume_command(&name))?;
            progress.update(
                format!("removing volume `{name}`"),
                Some((done, total)),
                Some(format!("removed `{name}`")),
            );
            entries.push(ContainerVolumePruneEntry {
                name: volume.name,
                backend: volume.backend,
                profile: volume.profile,
                project_name: volume.project_name,
                size_bytes: volume.size_bytes,
                removed: true,
                in_use: false,
                orphan_reason: volume.orphan_reason,
            });
        }
        Ok(render_container_report(
            volume_prune_report(
                &format!("repo volume inventory for {}", repo_root.display()),
                "dormant",
                &entries,
            ),
            output_json,
        ))
    })
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
    let mut policy = effigy_containers::prepare_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let _operation_plan = data_operation_plan(
        repo_root,
        &policy,
        ContainerDataOperation::pull_production(false),
    );
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
    let (creation_policy, workspace_mount_decisions) =
        load_container_policy_for_creation(repo_root, Some(policy.name.as_str()))
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    policy = creation_policy;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    let plan = crate::container_manager::compose_up_invocation_plan(
        repo_root,
        &policy,
        ContainerAction::Activate,
        "docker compose up",
    )?;
    crate::signals::run_compose_plan_capture(&plan)?;
    // The stack is started, so the runtime can be told to keep it up without
    // racing Compose's create/start (effigy#383).
    effigy_containers::policy::restart::apply_after_start(
        repo_root,
        &policy,
        &policy.project_name,
        &policy.compose_files,
        None,
    );
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
    annotate_workspace_extra_mount_decisions(&mut report, &workspace_mount_decisions);
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

#[cfg(test)]
mod workspace_mount_report_tests {
    use super::annotate_workspace_extra_mount_decisions;
    use effigy_containers::{
        ContainerCommandReport, WorkspaceExtraMountDecision, WorkspaceExtraMountDecisionKind,
    };

    #[test]
    fn data_pull_workspace_mount_decisions_are_bounded_and_explicit() {
        let long_path = format!("/workspace/{}", "x".repeat(500));
        let mut report = ContainerCommandReport {
            json: serde_json::json!({"schema": "effigy.container.data-pull-production.v1"}),
            success_text: "pulled production data".to_owned(),
        };

        annotate_workspace_extra_mount_decisions(
            &mut report,
            &[WorkspaceExtraMountDecision {
                source: long_path,
                target: "/workspace/extra".to_owned(),
                decision: WorkspaceExtraMountDecisionKind::IncludedInComposeSpec,
            }],
        );

        assert_eq!(
            report.json["workspace_extra_mounts"]["compose_up_dispatch"],
            "succeeded"
        );
        assert_eq!(
            report.json["workspace_extra_mounts"]["runtime_mount_verification"],
            "not_performed"
        );
        assert_eq!(
            report.json["workspace_extra_mounts"]["decisions"][0]["decision"],
            "included_in_compose_spec"
        );
        assert_eq!(
            report.json["workspace_extra_mounts"]["decisions"][0]["source"]
                .as_str()
                .expect("source is string")
                .chars()
                .count(),
            385
        );
        assert!(report
            .success_text
            .contains("actual runtime mount state was not verified"));
        assert!(!report.success_text.contains("applied"));
    }
}
