use effigy_ui::steps::ProgressFrame;
mod cache;
mod planning;
mod progress;
mod report;
mod transfer;
mod volume_io;
mod volumes;

use std::collections::BTreeSet;
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
    load_container_policy, load_container_policy_for_creation, validate_compose_backend_runtime,
    validate_container_policy, volume_list_report,
    volume_prune_report, ContainerCachePruneEntry, ContainerCacheVolumeEntry,
    ContainerDataTransferAction, ContainerDataVolumeEntry, ContainerVolumePruneEntry,
    EffectiveContainerPolicy,
};

use crate::EffigyRuntimeError;
pub use cache::discovered_running_projects;
use cache::{collect_global_cache_inventory, project_is_running, GlobalCachePruneVerdict};
use planning::{cache_kind_label, cache_scope_label, ensure_cache_prune_target_is_stopped};
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
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    // Data list reads volume state and never applies Compose, so the
    // apply-time backend preflight has no effect to admit here.

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
    progress.update(|_| {
        ProgressFrame::new(format!(
            "inspecting cache volumes for `{}`",
            repo_root.display()
        ))
        .label("Inspecting cache volumes")
    });
    let policy = load_container_policy(repo_root, name)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    // Cache list reads volume state and never applies Compose, so the
    // apply-time backend preflight has no effect to admit here.

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
    progress.update(|_| {
        ProgressFrame::new(format!(
            "inspecting cache volumes for `{}`",
            repo_root.display()
        ))
        .count(Some((caches.len() as u64, caches.len() as u64)))
        .event(Some(format!("listed {} cache volumes", caches.len())))
        .label("Inspecting cache volumes")
    });

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
        progress.update(|_| {
            ProgressFrame::new("inspecting cache volumes")
                .count(Some((0, total)))
                .event(Some(format!("found {total} repos")))
        });
        let mut rendered = Vec::new();
        for (index, repo_root) in repos.into_iter().enumerate() {
            progress.update(|_| {
                ProgressFrame::new(format!(
                    "inspecting cache volumes for `{}`",
                    repo_root.display()
                ))
                .count(Some((index as u64 + 1, total)))
                .label("Inspecting cache volumes")
            });
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

pub fn run_container_cache_list_all<FL, F>(
    cwd: &Path,
    profile: Option<&str>,
    project_filter: Option<&str>,
    kind_filter: Option<&str>,
    output_json: bool,
    list_running_projects: FL,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    FL: Fn() -> Result<BTreeSet<String>, EffigyRuntimeError>,
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    CacheVolumeProgress::run(PROBE_VOLUME_INSPECT, |progress| {
        progress.update(|_| ProgressFrame::new("inspecting cache volumes"));
        let resolved_profile = effigy_containers::backend::global_endpoint(profile);
        let running_projects = list_running_projects()?;
        let inventory = collect_global_cache_inventory(
            cwd,
            &resolved_profile,
            &running_projects,
            &run_runtime_volume_capture,
        )?;
        let caches = inventory
            .entries
            .into_iter()
            .filter(|cache| {
                project_filter.is_none_or(|project| cache.project_name.as_deref() == Some(project))
                    && kind_filter.is_none_or(|kind| cache.kind == kind)
            })
            .collect::<Vec<_>>();
        progress.update(|_| {
            ProgressFrame::new("inspecting cache volumes")
                .count(Some((caches.len() as u64, caches.len() as u64)))
                .event(Some(format!("listed {} cache volumes", caches.len())))
        });
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
        progress.update(|_| ProgressFrame::new("inspecting cache volumes"));
        let policy = load_container_policy(repo_root, name)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
        validate_container_policy(repo_root, &policy)
            .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
        // Cache prune removes named volumes through the runtime's volume
        // commands after proving the stack stopped; it never applies Compose,
        // so the apply-time backend preflight has no effect to admit here.

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
        progress.update(|_| {
            ProgressFrame::new("removing cache volumes")
                .count(Some((0, cache_total)))
                .event(Some(format!("listed {cache_total} cache volumes")))
                .waiting_for(PROBE_VOLUME_REMOVE)
        });
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
                    progress.update(|_| {
                        ProgressFrame::new(format!("skipping cache volume `{}`", volume.name))
                            .count(Some((done, cache_total)))
                            .event(Some(format!("left {reason}")))
                    });
                    continue;
                }
                Some(VolumeIdentity::Absent) | None => {
                    progress.update(|_| {
                        ProgressFrame::new(format!("skipping cache volume `{}`", volume.name))
                            .count(Some((done, cache_total)))
                            .event(Some("volume absent".to_owned()))
                    });
                    continue;
                }
            }
            progress.update(|_| {
                ProgressFrame::new(format!("removing cache volume `{}`", volume.name))
                    .count(Some((done, cache_total)))
            });
            run_runtime_volume_capture(repo_root, &endpoint, &remove_volume_command(&volume.name))?;
            progress.update(|_| {
                ProgressFrame::new(format!("removing cache volume `{}`", volume.name))
                    .count(Some((done, cache_total)))
                    .event(Some(format!("removed `{}`", volume.name)))
            });
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

pub fn run_container_cache_prune_all<FL, F>(
    cwd: &Path,
    profile: Option<&str>,
    project_filter: Option<&str>,
    kind_filter: Option<&str>,
    output_json: bool,
    list_running_projects: FL,
    run_runtime_volume_capture: F,
) -> Result<String, EffigyRuntimeError>
where
    FL: Fn() -> Result<BTreeSet<String>, EffigyRuntimeError>,
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    CacheVolumeProgress::run(PROBE_VOLUME_INSPECT, |progress| {
        progress.update(|_| ProgressFrame::new("inspecting cache volumes"));
        let resolved_profile = effigy_containers::backend::global_endpoint(profile);
        let running_projects = list_running_projects()?;
        let inventory = collect_global_cache_inventory(
            cwd,
            &resolved_profile,
            &running_projects,
            &run_runtime_volume_capture,
        )?;
        let caches = inventory
            .entries
            .into_iter()
            .filter(|cache| {
                project_filter.is_none_or(|project| cache.project_name.as_deref() == Some(project))
                    && kind_filter.is_none_or(|kind| cache.kind == kind)
            })
            .collect::<Vec<_>>();
        let total = caches.len() as u64;
        progress.update(|_| {
            ProgressFrame::new("removing cache volumes")
                .count(Some((0, total)))
                .event(Some(format!("listed {total} cache volumes")))
                .waiting_for(PROBE_VOLUME_REMOVE)
        });
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        for (index, cache) in caches.into_iter().enumerate() {
            let done = index as u64 + 1;
            let name = cache.name.clone();
            // Contract 005: cache-like names and content classification are
            // diagnostics and never deletion authority. Only the volume's own
            // Effigy labels admit removal, and a discovered-running owning
            // project stays intact.
            match inventory.verdicts.get(&name) {
                Some(GlobalCachePruneVerdict::Removable) => {
                    progress.update(|_| {
                        ProgressFrame::new(format!("removing cache volume `{name}`"))
                            .count(Some((done, total)))
                    });
                    run_runtime_volume_capture(
                        cwd,
                        &resolved_profile,
                        &remove_volume_command(&name),
                    )?;
                    progress.update(|_| {
                        ProgressFrame::new(format!("removing cache volume `{name}`"))
                            .count(Some((done, total)))
                            .event(Some(format!("removed `{name}`")))
                    });
                    entries.push(ContainerCachePruneEntry {
                        name: cache.name,
                        kind: cache.kind,
                        size_bytes: cache.size_bytes,
                        project_name: cache.project_name,
                        removed: true,
                        in_use: false,
                    });
                }
                Some(GlobalCachePruneVerdict::OwnedInUse { project: _ }) => {
                    progress.update(|_| {
                        ProgressFrame::new(format!("skipping cache volume `{name}`"))
                            .count(Some((done, total)))
                            .event(Some(format!("`{name}` in use")))
                    });
                    entries.push(ContainerCachePruneEntry {
                        name: cache.name,
                        kind: cache.kind,
                        size_bytes: cache.size_bytes,
                        project_name: cache.project_name,
                        removed: false,
                        in_use: true,
                    });
                }
                verdict => {
                    // Foreign labels (or a verdict that could not be resolved)
                    // keep the volume intact and say why.
                    let reason = match verdict {
                        Some(GlobalCachePruneVerdict::Foreign(reason)) => reason.clone(),
                        _ => format!("volume `{name}` has no complete inspection record"),
                    };
                    warnings.push(format!(
                        "left {reason}; it was not created for this environment"
                    ));
                    progress.update(|_| {
                        ProgressFrame::new(format!("skipping cache volume `{name}`"))
                            .count(Some((done, total)))
                            .event(Some(format!("left {reason}")))
                    });
                    entries.push(ContainerCachePruneEntry {
                        name: cache.name,
                        kind: cache.kind,
                        size_bytes: cache.size_bytes,
                        project_name: cache.project_name,
                        removed: false,
                        in_use: cache.in_use,
                    });
                }
            }
        }
        let scope_label =
            cache_scope_label("profile-wide cache inventory", project_filter, kind_filter);

        let mut report = cache_prune_report(&scope_label, &entries);
        annotate_warning_lines(&mut report, &warnings);
        Ok(render_container_report(report, output_json))
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
        progress.update(|_| ProgressFrame::new("inspecting managed volumes"));
        let volumes =
            collect_global_volume_entries(cwd, orphans_only, &run_runtime_volume_capture)?;
        progress.update(|_| {
            ProgressFrame::new("inspecting managed volumes")
                .count(Some((volumes.len() as u64, volumes.len() as u64)))
                .event(Some(format!("listed {} volumes", volumes.len())))
        });
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
        progress.update(|_| {
            ProgressFrame::new(format!("inspecting volumes for `{}`", repo_root.display()))
                .label("Inspecting volumes")
        });
        let volumes =
            collect_repo_volume_entries(repo_root, orphans_only, &run_runtime_volume_capture)?;
        progress.update(|_| {
            ProgressFrame::new(format!("inspecting volumes for `{}`", repo_root.display()))
                .count(Some((volumes.len() as u64, volumes.len() as u64)))
                .event(Some(format!("listed {} volumes", volumes.len())))
                .label("Inspecting volumes")
        });
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
        progress.update(|_| ProgressFrame::new("inspecting managed volumes"));
        let volumes = collect_global_volume_entries(cwd, true, &run_runtime_volume_capture)?;
        let total = volumes.len() as u64;
        progress.update(|_| {
            ProgressFrame::new("removing volumes")
                .count(Some((0, total)))
                .event(Some(format!("listed {total} volumes")))
                .waiting_for(PROBE_VOLUME_REMOVE)
        });
        let mut entries = Vec::new();
        for (index, volume) in volumes.into_iter().enumerate() {
            let done = index as u64 + 1;
            let name = volume.name.clone();
            if volume.in_use {
                progress.update(|_| {
                    ProgressFrame::new(format!("skipping volume `{name}`"))
                        .count(Some((done, total)))
                        .event(Some(format!("`{name}` in use")))
                });
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
            progress.update(|_| {
                ProgressFrame::new(format!("removing volume `{name}`")).count(Some((done, total)))
            });
            run_runtime_volume_capture(cwd, &volume.profile, &remove_volume_command(&name))?;
            progress.update(|_| {
                ProgressFrame::new(format!("removing volume `{name}`"))
                    .count(Some((done, total)))
                    .event(Some(format!("removed `{name}`")))
            });
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
        progress.update(|_| {
            ProgressFrame::new(format!("inspecting volumes for `{}`", repo_root.display()))
                .label("Inspecting volumes")
        });
        let volumes = collect_repo_volume_entries(repo_root, true, &run_runtime_volume_capture)?;
        let total = volumes.len() as u64;
        progress.update(|_| {
            ProgressFrame::new("removing volumes")
                .count(Some((0, total)))
                .event(Some(format!("listed {total} volumes")))
                .waiting_for(PROBE_VOLUME_REMOVE)
        });
        let mut entries = Vec::new();
        for (index, volume) in volumes.into_iter().enumerate() {
            let done = index as u64 + 1;
            let name = volume.name.clone();
            if volume.in_use {
                progress.update(|_| {
                    ProgressFrame::new(format!("skipping volume `{name}`"))
                        .count(Some((done, total)))
                        .event(Some(format!("`{name}` in use")))
                });
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
            progress.update(|_| {
                ProgressFrame::new(format!("removing volume `{name}`")).count(Some((done, total)))
            });
            run_runtime_volume_capture(repo_root, &volume.profile, &remove_volume_command(&name))?;
            progress.update(|_| {
                ProgressFrame::new(format!("removing volume `{name}`"))
                    .count(Some((done, total)))
                    .event(Some(format!("removed `{name}`")))
            });
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
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    // Export and import run one throwaway volume container through the
    // runtime's own run command; Compose is never applied, so the apply-time
    // backend preflight has no effect to admit here.
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
    validate_container_policy(repo_root, &policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    // Pull-production starts the stack, so the apply-time backend preflight
    // (temp-root sharing, nerdctl mounts budget) admits before that apply.
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

/// Producer regressions for `container cache prune --global` (contract 005):
/// cache-like names and legacy content classification stay diagnostic, and
/// only volumes whose own Effigy labels prove ownership reach `volume rm`.
/// Every test drives the destructive producer through a fake runtime that
/// records each command; no real Docker, Colima or volume is touched.
#[cfg(test)]
mod global_cache_prune_tests {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::os::unix::process::ExitStatusExt;
    use std::path::Path;
    use std::process::Output;
    use std::rc::Rc;

    use super::{run_container_cache_list_all, run_container_cache_prune_all};
    use crate::EffigyRuntimeError;
    use effigy_catalog::volumes::DockerCommand;

    const RUNNING_PROJECT: &str = "webshop-dev";
    const STOPPED_PROJECT: &str = "shop-old";
    const OWNED_STOPPED: &str = "shop-old-workspace-api-target";
    const OWNED_RUNNING: &str = "webshop-dev-workspace-cargo-git";
    const FOREIGN_TARGET: &str = "acme-website-target";
    const FOREIGN_NODE_MODULES: &str = "shared-node_modules";
    const FOREIGN_PNPM: &str = "pnpm-store";
    const LEGACY_EFV: &str = "efv-legacy0a";
    const UNRELATED_DATA: &str = "webshop-dev-db-data";

    fn output(stdout: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn label_object(pairs: &[(&str, &str)]) -> String {
        let rendered = pairs
            .iter()
            .map(|(key, value)| format!("\"{key}\":\"{value}\""))
            .collect::<Vec<_>>()
            .join(",");
        format!("{{{rendered}}}")
    }

    fn inspect_entry(name: &str, labels: &str) -> String {
        format!(
            "{{\"Name\":\"{name}\",\"Mountpoint\":\"/var/lib/docker/volumes/{name}/_data\",\"UsageData\":{{\"Size\":4096}},\"Labels\":{labels}}}"
        )
    }

    /// One inspect payload fragment per candidate name; the fake inspect
    /// serves exactly the requested names, so removing or duplicating a
    /// fragment models partial or ambiguous runtime evidence.
    type InspectFragments = BTreeMap<&'static str, String>;

    fn effigy_labels(project: &str, volume_name: &str) -> String {
        label_object(&[
            ("com.effigy.managed", "true"),
            ("com.effigy.project", project),
            ("com.effigy.volume-name", volume_name),
        ])
    }

    fn unlabelled() -> String {
        label_object(&[])
    }

    fn compose_label(project: &str) -> String {
        label_object(&[("com.docker.compose.project", project)])
    }

    fn standard_listing() -> Vec<String> {
        vec![
            FOREIGN_TARGET.to_owned(),
            FOREIGN_NODE_MODULES.to_owned(),
            FOREIGN_PNPM.to_owned(),
            LEGACY_EFV.to_owned(),
            OWNED_STOPPED.to_owned(),
            OWNED_RUNNING.to_owned(),
            // Not cache-like and not opaque: never a candidate.
            UNRELATED_DATA.to_owned(),
        ]
    }

    fn standard_inspect_fragments() -> InspectFragments {
        InspectFragments::from([
            (FOREIGN_TARGET, inspect_entry(FOREIGN_TARGET, &unlabelled())),
            (
                FOREIGN_NODE_MODULES,
                inspect_entry(FOREIGN_NODE_MODULES, &unlabelled()),
            ),
            (FOREIGN_PNPM, inspect_entry(FOREIGN_PNPM, &unlabelled())),
            (
                LEGACY_EFV,
                inspect_entry(LEGACY_EFV, &compose_label("legacy-app")),
            ),
            (
                OWNED_STOPPED,
                inspect_entry(
                    OWNED_STOPPED,
                    &effigy_labels(STOPPED_PROJECT, OWNED_STOPPED),
                ),
            ),
            (
                OWNED_RUNNING,
                inspect_entry(
                    OWNED_RUNNING,
                    &effigy_labels(RUNNING_PROJECT, OWNED_RUNNING),
                ),
            ),
        ])
    }

    struct FakeRuntime {
        listing: Vec<String>,
        inspect_fragments: InspectFragments,
        /// Overrides the inspect response entirely; `Err` models a spawn
        /// failure the way the real adapter produces one.
        inspect_result: Option<Result<String, EffigyRuntimeError>>,
        kind_rows: String,
        removals: Rc<RefCell<Vec<String>>>,
        inspect_requests: Rc<RefCell<Vec<Vec<String>>>>,
        commands: Rc<RefCell<Vec<String>>>,
        fail_removal_of: Option<&'static str>,
    }

    impl FakeRuntime {
        fn standard() -> Self {
            Self {
                listing: standard_listing(),
                inspect_fragments: standard_inspect_fragments(),
                inspect_result: None,
                kind_rows: format!("/var/lib/docker/volumes/{LEGACY_EFV}/_data\trust-target\n"),
                removals: Rc::new(RefCell::new(Vec::new())),
                inspect_requests: Rc::new(RefCell::new(Vec::new())),
                commands: Rc::new(RefCell::new(Vec::new())),
                fail_removal_of: None,
            }
        }

        fn capture(&self, command: &DockerCommand) -> Result<Output, EffigyRuntimeError> {
            self.commands.borrow_mut().push(command.args.join(" "));
            let sub = |index: usize| command.args.get(index).map(String::as_str);
            if sub(0) == Some("volume") && sub(1) == Some("ls") {
                return Ok(output(&self.listing.join("\n")));
            }
            if command.program == "__effigy_volume_cache_kind_batch" {
                return Ok(output(&self.kind_rows));
            }
            if command.program == "__effigy_volume_usage_batch" {
                return Ok(output(""));
            }
            if sub(0) == Some("volume") && sub(1) == Some("inspect") {
                let names = command.args[2..].to_vec();
                self.inspect_requests.borrow_mut().push(names.clone());
                if let Some(result) = &self.inspect_result {
                    return match result {
                        Ok(stdout) => Ok(output(stdout)),
                        Err(error) => Err(EffigyRuntimeError::task_invocation(error.to_string())),
                    };
                }
                let fragments = names
                    .iter()
                    .filter_map(|name| self.inspect_fragments.get(name.as_str()).cloned())
                    .collect::<Vec<_>>()
                    .join(",");
                return Ok(output(&format!("[{fragments}]")));
            }
            if sub(0) == Some("volume") && sub(1) == Some("rm") {
                let name = command.args[2].clone();
                if self.fail_removal_of == Some(name.as_str()) {
                    return Err(EffigyRuntimeError::task_invocation(format!(
                        "Remove volume '{name}' failed (1)"
                    )));
                }
                self.removals.borrow_mut().push(name);
                return Ok(output(""));
            }
            Err(EffigyRuntimeError::task_invocation(format!(
                "unexpected fake runtime command: {}",
                command.args.join(" ")
            )))
        }
    }

    fn running_projects(project: &str) -> Result<BTreeSet<String>, EffigyRuntimeError> {
        Ok(BTreeSet::from([project.to_owned()]))
    }

    fn prune_report(rendered: &str) -> serde_json::Value {
        serde_json::from_str(rendered).expect("prune report is JSON")
    }

    fn cache_entries(report: &serde_json::Value) -> Vec<(String, bool, bool)> {
        report["caches"]
            .as_array()
            .expect("caches array")
            .iter()
            .map(|cache| {
                (
                    cache["name"].as_str().expect("name").to_owned(),
                    cache["removed"].as_bool().expect("removed"),
                    cache["in_use"].as_bool().expect("in_use"),
                )
            })
            .collect()
    }

    fn warning_lines(report: &serde_json::Value) -> Vec<String> {
        report["warnings"]
            .as_array()
            .map(|warnings| {
                warnings
                    .iter()
                    .map(|warning| warning.as_str().expect("warning text").to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// An unmounted volume named like a target, node_modules or pnpm cache is
    /// never deletion authority: only the exactly-labelled owned cache with no
    /// running project reaches `volume rm`.
    #[test]
    fn global_cache_prune_lookalike_and_unlabelled_caches_are_never_removed() {
        let runtime = FakeRuntime::standard();
        let removals = Rc::clone(&runtime.removals);
        let rendered = run_container_cache_prune_all(
            Path::new("."),
            Some("test-profile"),
            None,
            None,
            true,
            || running_projects(RUNNING_PROJECT),
            |_cwd: &Path, _endpoint: &str, command: &DockerCommand| runtime.capture(command),
        )
        .expect("prune succeeds");

        // Exactly the owned stopped cache is removed; every lookalike name
        // stays intact.
        assert_eq!(removals.borrow().as_slice(), [OWNED_STOPPED]);

        let report = prune_report(&rendered);
        assert_eq!(report["removed_count"], 1);
        let entries = cache_entries(&report);
        assert!(entries.contains(&(OWNED_STOPPED.to_owned(), true, false)));
        assert!(entries.contains(&(OWNED_RUNNING.to_owned(), false, true)));
        for foreign in [
            FOREIGN_TARGET,
            FOREIGN_NODE_MODULES,
            FOREIGN_PNPM,
            LEGACY_EFV,
        ] {
            assert!(
                entries.contains(&(foreign.to_owned(), false, false)),
                "{foreign} must be listed as kept"
            );
        }
        // Every foreign volume is reported, not silently kept.
        let warnings = warning_lines(&report);
        assert_eq!(warnings.len(), 4, "warnings: {warnings:?}");
        for foreign in [
            FOREIGN_TARGET,
            FOREIGN_NODE_MODULES,
            FOREIGN_PNPM,
            LEGACY_EFV,
        ] {
            assert!(
                warnings
                    .iter()
                    .any(|warning| warning.contains(foreign) && warning.contains("left")),
                "{foreign} must be reported: {warnings:?}"
            );
        }

        // The unrelated data volume is not even a candidate: it is listed but
        // never inspected and never removed.
        let inspected = runtime.inspect_requests.borrow();
        assert_eq!(inspected.len(), 1, "one batched inspect");
        assert!(!inspected[0].iter().any(|name| name == UNRELATED_DATA));
        assert_eq!(inspected[0].len(), 6);
    }

    /// A partial inspection (a listed volume missing from the inspect rows)
    /// is an error, never absence: nothing is removed.
    #[test]
    fn global_cache_prune_partial_inspection_is_an_error_and_removes_nothing() {
        let mut runtime = FakeRuntime::standard();
        runtime.inspect_fragments.remove(OWNED_STOPPED);
        let removals = Rc::clone(&runtime.removals);
        let error = run_container_cache_prune_all(
            Path::new("."),
            Some("test-profile"),
            None,
            None,
            true,
            || running_projects(RUNNING_PROJECT),
            |_cwd: &Path, _endpoint: &str, command: &DockerCommand| runtime.capture(command),
        )
        .expect_err("partial inspection must fail");
        assert!(error.to_string().contains(OWNED_STOPPED), "{error}");
        assert!(removals.borrow().is_empty());
    }

    /// A failed inspect spawn is an error, never an absence that would admit
    /// removal.
    #[test]
    fn global_cache_prune_inspect_spawn_failure_is_an_error_and_removes_nothing() {
        let mut runtime = FakeRuntime::standard();
        runtime.inspect_result = Some(Err(EffigyRuntimeError::task_invocation(
            "volume inspect failed (1)",
        )));
        let removals = Rc::clone(&runtime.removals);
        let error = run_container_cache_prune_all(
            Path::new("."),
            Some("test-profile"),
            None,
            None,
            true,
            || running_projects(RUNNING_PROJECT),
            |_cwd: &Path, _endpoint: &str, command: &DockerCommand| runtime.capture(command),
        )
        .expect_err("failed inspect must fail the prune");
        assert!(
            error.to_string().contains("volume inspect failed"),
            "{error}"
        );
        assert!(removals.borrow().is_empty());
    }

    /// Ambiguous runtime evidence (duplicate inspect rows for one volume)
    /// refuses rather than trusting the first row.
    #[test]
    fn global_cache_prune_duplicate_inspection_rows_are_an_error() {
        let mut runtime = FakeRuntime::standard();
        let entry = inspect_entry(
            OWNED_STOPPED,
            &effigy_labels(STOPPED_PROJECT, OWNED_STOPPED),
        );
        runtime
            .inspect_fragments
            .insert(OWNED_STOPPED, format!("{entry},{entry}"));
        let removals = Rc::clone(&runtime.removals);
        let error = run_container_cache_prune_all(
            Path::new("."),
            Some("test-profile"),
            None,
            None,
            true,
            || running_projects(RUNNING_PROJECT),
            |_cwd: &Path, _endpoint: &str, command: &DockerCommand| runtime.capture(command),
        )
        .expect_err("duplicate inspect rows must fail");
        assert!(
            error.to_string().contains("duplicate entries")
                && error.to_string().contains(OWNED_STOPPED),
            "{error}"
        );
        assert!(removals.borrow().is_empty());
    }

    /// Malformed inspect output is an error, never missing metadata.
    #[test]
    fn global_cache_prune_malformed_inspection_json_is_an_error() {
        let mut runtime = FakeRuntime::standard();
        runtime.inspect_result = Some(Ok("not json".to_owned()));
        let removals = Rc::clone(&runtime.removals);
        let error = run_container_cache_prune_all(
            Path::new("."),
            Some("test-profile"),
            None,
            None,
            true,
            || running_projects(RUNNING_PROJECT),
            |_cwd: &Path, _endpoint: &str, command: &DockerCommand| runtime.capture(command),
        )
        .expect_err("malformed inspect must fail");
        assert!(error.to_string().contains("JSON"), "{error}");
        assert!(removals.borrow().is_empty());
    }

    /// A running-project discovery failure is an error before any command:
    /// unknown activity grants nothing (contract 052 R3).
    #[test]
    fn global_cache_prune_running_project_discovery_failure_removes_nothing() {
        let runtime = FakeRuntime::standard();
        let commands = Rc::clone(&runtime.commands);
        let error = run_container_cache_prune_all(
            Path::new("."),
            Some("test-profile"),
            None,
            None,
            true,
            || Err(EffigyRuntimeError::task_invocation("discovery failed")),
            |_cwd: &Path, _endpoint: &str, command: &DockerCommand| runtime.capture(command),
        )
        .expect_err("discovery failure must stop the prune");
        assert!(error.to_string().contains("discovery failed"), "{error}");
        assert!(commands.borrow().is_empty());
    }

    /// A failed removal propagates the adapter error and is never reported as
    /// removed; the real adapter already turns nonzero exits into errors, so
    /// the fake models exactly that.
    #[test]
    fn global_cache_prune_removal_failure_is_propagated_not_marked_removed() {
        let mut runtime = FakeRuntime::standard();
        runtime.fail_removal_of = Some(OWNED_STOPPED);
        let removals = Rc::clone(&runtime.removals);
        let error = run_container_cache_prune_all(
            Path::new("."),
            Some("test-profile"),
            None,
            None,
            true,
            || running_projects(RUNNING_PROJECT),
            |_cwd: &Path, _endpoint: &str, command: &DockerCommand| runtime.capture(command),
        )
        .expect_err("failed removal must propagate");
        assert!(
            error.to_string().contains("Remove volume"),
            "error names the failed removal: {error}"
        );
        assert!(removals.borrow().is_empty());
    }

    /// Project and kind filters still select what is considered; a filter that
    /// excludes the only removable cache removes nothing.
    #[test]
    fn global_cache_prune_project_and_kind_filters_still_apply() {
        let runtime = FakeRuntime::standard();
        let removals = Rc::clone(&runtime.removals);
        let rendered = run_container_cache_prune_all(
            Path::new("."),
            Some("test-profile"),
            Some("other-project"),
            None,
            true,
            || running_projects(RUNNING_PROJECT),
            |_cwd: &Path, _endpoint: &str, command: &DockerCommand| runtime.capture(command),
        )
        .expect("prune succeeds");
        assert!(removals.borrow().is_empty());
        assert_eq!(prune_report(&rendered)["removed_count"], 0);
        assert_eq!(
            prune_report(&rendered)["caches"].as_array().map(Vec::len),
            Some(0)
        );

        let runtime = FakeRuntime::standard();
        let removals = Rc::clone(&runtime.removals);
        let rendered = run_container_cache_prune_all(
            Path::new("."),
            Some("test-profile"),
            None,
            Some("node-modules"),
            true,
            || running_projects(RUNNING_PROJECT),
            |_cwd: &Path, _endpoint: &str, command: &DockerCommand| runtime.capture(command),
        )
        .expect("prune succeeds");
        assert!(removals.borrow().is_empty());
        assert_eq!(prune_report(&rendered)["removed_count"], 0);
        let kinds = prune_report(&rendered)["caches"]
            .as_array()
            .expect("caches")
            .iter()
            .all(|cache| cache["kind"] == "node-modules");
        assert!(kinds);
    }

    /// The read-only global listing keeps its legacy behaviour: opaque `efv-*`
    /// volumes are classified by content and stay visible, with no removal
    /// authority behind it.
    #[test]
    fn global_cache_list_keeps_legacy_classification_diagnostic() {
        let runtime = FakeRuntime::standard();
        let rendered = run_container_cache_list_all(
            Path::new("."),
            Some("test-profile"),
            None,
            None,
            true,
            || running_projects(RUNNING_PROJECT),
            |_cwd: &Path, _endpoint: &str, command: &DockerCommand| runtime.capture(command),
        )
        .expect("list succeeds");
        let report = prune_report(&rendered);
        let efv = report["caches"]
            .as_array()
            .expect("caches array")
            .iter()
            .find(|cache| cache["name"] == LEGACY_EFV)
            .expect("legacy efv volume stays listed");
        assert_eq!(efv["kind"], "rust-target");
        assert_eq!(efv["project_name"], "legacy-app");
        assert_eq!(efv["in_use"], false);
        assert!(runtime.removals.borrow().is_empty());
        // The owned running cache is marked in use from its own label.
        let owned_running = report["caches"]
            .as_array()
            .expect("caches array")
            .iter()
            .find(|cache| cache["name"] == OWNED_RUNNING)
            .expect("owned running cache listed");
        assert_eq!(owned_running["in_use"], true);
        assert_eq!(owned_running["project_name"], RUNNING_PROJECT);
    }
}
