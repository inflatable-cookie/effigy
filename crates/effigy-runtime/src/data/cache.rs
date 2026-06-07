use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_catalog::volumes::{
    list_all_volumes_command, parse_listed_volume_names, parse_volume_usage_bytes_map,
    volume_cache_kind_batch_command, volume_usage_batch_command, DockerCommand,
};
use effigy_containers::ContainerCacheGlobalEntry;

use super::planning::{
    cache_kind_from_volume_name, collect_global_cache_entries_from_names,
    parse_volume_cache_kind_rows,
};
use super::volume_io::inspect_runtime_volume_metadata_batch;
use crate::read::discover_running_environments;
use crate::EffigyRuntimeError;

pub(super) fn collect_global_cache_entries<F>(
    cwd: &Path,
    profile: &str,
    run_runtime_volume_capture: &F,
) -> Result<Vec<ContainerCacheGlobalEntry>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let running_projects = discover_running_environments()?
        .into_iter()
        .map(|environment| environment.policy.project_name)
        .collect::<BTreeSet<_>>();
    let listed = run_runtime_volume_capture(cwd, profile, &list_all_volumes_command())?;
    let names = parse_listed_volume_names(String::from_utf8_lossy(&listed.stdout).as_ref())
        .into_iter()
        .filter(|name| cache_kind_from_volume_name(name).is_some() || name.starts_with("efv-"))
        .collect::<Vec<_>>();
    let metadata =
        inspect_runtime_volume_metadata_batch(cwd, profile, &names, run_runtime_volume_capture)?
            .into_iter()
            .map(|entry| (entry.name.clone(), entry))
            .collect::<BTreeMap<_, _>>();
    let legacy_mount_points = metadata
        .values()
        .filter(|entry| cache_kind_from_volume_name(&entry.name).is_none())
        .filter_map(|entry| entry.mount_point.clone())
        .collect::<Vec<_>>();
    let cache_kind_by_mount_point = if legacy_mount_points.is_empty() {
        BTreeMap::new()
    } else {
        run_runtime_volume_capture(
            cwd,
            profile,
            &volume_cache_kind_batch_command(&legacy_mount_points),
        )
        .ok()
        .map(|output| {
            parse_volume_cache_kind_rows(String::from_utf8_lossy(&output.stdout).as_ref())
        })
        .unwrap_or_default()
    };
    let cache_kind_by_name = metadata
        .values()
        .filter_map(|entry| {
            let mount_point = entry.mount_point.as_ref()?;
            let kind = cache_kind_by_mount_point.get(mount_point)?;
            Some((entry.name.clone(), kind.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let missing_mount_points = metadata
        .values()
        .filter(|entry| entry.size_bytes.is_none())
        .filter_map(|entry| entry.mount_point.clone())
        .collect::<Vec<_>>();
    let usage_by_mount_point = if missing_mount_points.is_empty() {
        BTreeMap::new()
    } else {
        run_runtime_volume_capture(
            cwd,
            profile,
            &volume_usage_batch_command(&missing_mount_points),
        )
        .ok()
        .map(|output| {
            parse_volume_usage_bytes_map(String::from_utf8_lossy(&output.stdout).as_ref())
        })
        .unwrap_or_default()
    };
    Ok(collect_global_cache_entries_from_names(
        names,
        &running_projects,
        &metadata,
        &cache_kind_by_name,
        &usage_by_mount_point,
    ))
}

pub(super) fn project_is_running(
    repo_root: &Path,
    project_name: &str,
) -> Result<bool, EffigyRuntimeError> {
    let target_root = canonicalize_or_original(repo_root);
    Ok(discover_running_environments()?
        .into_iter()
        .any(|environment| {
            canonicalize_or_original(Path::new(&environment.repo_root)) == target_root
                && environment.policy.project_name == project_name
        }))
}

fn canonicalize_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
