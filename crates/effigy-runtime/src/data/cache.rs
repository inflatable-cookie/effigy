use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_catalog::volumes::{
    list_all_volumes_command, parse_listed_volume_names, parse_volume_usage_bytes_map,
    volume_usage_batch_command, DockerCommand,
};
use effigy_containers::ContainerCacheGlobalEntry;

use super::planning::{cache_kind_from_volume_name, collect_global_cache_entries_from_names};
use super::volume_io::inspect_runtime_volume_metadata;
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
        .filter(|name| cache_kind_from_volume_name(name).is_some())
        .collect::<Vec<_>>();
    let metadata = names
        .iter()
        .filter_map(|name| {
            inspect_runtime_volume_metadata(cwd, profile, name, run_runtime_volume_capture)
                .ok()
                .flatten()
        })
        .map(|entry| (entry.name.clone(), entry))
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
