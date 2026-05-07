use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_catalog::volumes::{
    list_all_volumes_command, parse_listed_volume_names, parse_volume_usage_bytes_map,
    volume_usage_batch_command, DockerCommand,
};
use effigy_containers::{
    exec::running_colima_profiles, load_all_container_policies, ContainerVolumeGlobalEntry,
};

use super::volume_io::inspect_runtime_volume_metadata;
use crate::EffigyRuntimeError;

const DOCKER_RUNTIME_PROFILE: &str = "docker";
const LABEL_MANAGED: &str = "com.effigy.managed";
const LABEL_PROJECT: &str = "com.effigy.project";
const LABEL_REPO_ROOT: &str = "com.effigy.repo-root";
const LABEL_SERVICE: &str = "com.effigy.service";
const LABEL_MOUNT_TARGET: &str = "com.effigy.mount-target";
const LABEL_PERSIST: &str = "com.effigy.persist";

pub(super) fn collect_global_volume_entries<F>(
    cwd: &Path,
    orphans_only: bool,
    run_runtime_volume_capture: &F,
) -> Result<Vec<ContainerVolumeGlobalEntry>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let mut entries = Vec::new();
    let mut ownership_cache = BTreeMap::<String, RepoOwnershipState>::new();
    let mut runtime_profiles = vec![DOCKER_RUNTIME_PROFILE.to_owned()];
    runtime_profiles.extend(running_colima_profiles(cwd).unwrap_or_default());

    for profile in runtime_profiles {
        let Ok(listed) = run_runtime_volume_capture(cwd, &profile, &list_all_volumes_command())
        else {
            continue;
        };
        let names = parse_listed_volume_names(String::from_utf8_lossy(&listed.stdout).as_ref());
        let metadata = names
            .iter()
            .filter_map(|name| {
                inspect_runtime_volume_metadata(cwd, &profile, name, run_runtime_volume_capture)
                    .ok()
                    .flatten()
                    .filter(|metadata| {
                        metadata.labels.get(LABEL_MANAGED).map(String::as_str) == Some("true")
                    })
            })
            .collect::<Vec<_>>();
        let missing_mount_points = metadata
            .iter()
            .filter(|entry| entry.size_bytes.is_none())
            .filter_map(|entry| entry.mount_point.clone())
            .collect::<Vec<_>>();
        let usage_by_mount_point = if missing_mount_points.is_empty() {
            BTreeMap::new()
        } else {
            run_runtime_volume_capture(
                cwd,
                &profile,
                &volume_usage_batch_command(&missing_mount_points),
            )
            .ok()
            .map(|output| {
                parse_volume_usage_bytes_map(String::from_utf8_lossy(&output.stdout).as_ref())
            })
            .unwrap_or_default()
        };

        for metadata in metadata {
            let repo_root = metadata.labels.get(LABEL_REPO_ROOT).cloned();
            let orphan_reason = repo_root.as_deref().and_then(|repo_root| {
                orphan_reason(repo_root, &metadata.name, &mut ownership_cache)
            });
            let orphaned = orphan_reason.is_some();
            if orphans_only && !orphaned {
                continue;
            }
            let size_bytes = metadata.size_bytes.or_else(|| {
                metadata
                    .mount_point
                    .as_deref()
                    .and_then(|mount| usage_by_mount_point.get(mount).copied())
            });
            entries.push(ContainerVolumeGlobalEntry {
                name: metadata.name,
                backend: if profile == DOCKER_RUNTIME_PROFILE {
                    "docker".to_owned()
                } else {
                    "containerd".to_owned()
                },
                profile: profile.clone(),
                project_name: metadata.labels.get(LABEL_PROJECT).cloned(),
                repo_root,
                service: metadata.labels.get(LABEL_SERVICE).cloned(),
                mount_target: metadata.labels.get(LABEL_MOUNT_TARGET).cloned(),
                persist: metadata
                    .labels
                    .get(LABEL_PERSIST)
                    .map(|value| value.eq_ignore_ascii_case("true")),
                size_bytes,
                orphaned,
                orphan_reason,
            });
        }
    }

    entries.sort_by(|left, right| {
        left.repo_root
            .cmp(&right.repo_root)
            .then_with(|| left.project_name.cmp(&right.project_name))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.profile.cmp(&right.profile))
    });
    Ok(entries)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RepoOwnershipState {
    RepoMissing,
    ManifestMissing,
    Declared(BTreeSet<String>),
    Unresolved,
}

fn orphan_reason(
    repo_root: &str,
    volume_name: &str,
    ownership_cache: &mut BTreeMap<String, RepoOwnershipState>,
) -> Option<String> {
    let state = ownership_cache
        .entry(repo_root.to_owned())
        .or_insert_with(|| load_repo_ownership(repo_root));
    match state {
        RepoOwnershipState::RepoMissing => Some("repo-missing".to_owned()),
        RepoOwnershipState::ManifestMissing => Some("manifest-missing".to_owned()),
        RepoOwnershipState::Declared(volumes) => {
            if volumes.contains(volume_name) {
                None
            } else {
                Some("no-longer-declared".to_owned())
            }
        }
        RepoOwnershipState::Unresolved => None,
    }
}

fn load_repo_ownership(repo_root: &str) -> RepoOwnershipState {
    let repo_root_path = PathBuf::from(repo_root);
    if !repo_root_path.exists() {
        return RepoOwnershipState::RepoMissing;
    }
    if !repo_root_path.join("effigy.toml").is_file() {
        return RepoOwnershipState::ManifestMissing;
    }
    match load_all_container_policies(&repo_root_path) {
        Ok(policies) => RepoOwnershipState::Declared(
            policies
                .into_iter()
                .flat_map(|policy| policy.managed_volumes.into_iter().map(|volume| volume.name))
                .collect(),
        ),
        Err(_) => RepoOwnershipState::Unresolved,
    }
}
