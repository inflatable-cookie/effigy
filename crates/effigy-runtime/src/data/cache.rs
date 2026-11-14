use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{
    list_all_volumes_command, parse_listed_volume_names, parse_volume_usage_bytes_map,
    volume_cache_kind_batch_command, volume_usage_batch_command, DockerCommand,
};
use effigy_containers::ContainerCacheGlobalEntry;
use effigy_core::repo;

use super::planning::{
    cache_kind_from_volume_name, collect_global_cache_entries_from_names, effigy_owned_project,
    parse_volume_cache_kind_rows,
};
use super::volume_io::inspect_runtime_volume_metadata_batch_strict;
use crate::read::discover_running_environments;
use crate::EffigyRuntimeError;

/// Deletion authority for one global cache candidate (contract 005). Name and
/// content classification stay diagnostic; only the volume's own Effigy
/// labels prove ownership, and a discovered-running owning project stays
/// intact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GlobalCachePruneVerdict {
    /// Effigy's labels prove ownership and the owning project is not
    /// discovered running.
    Removable,
    /// Effigy's labels prove ownership, but the owning project is discovered
    /// running.
    OwnedInUse { project: String },
    /// The labels do not prove Effigy ownership; reported and left alone.
    Foreign(String),
}

/// The profile-wide cache inventory plus the per-candidate prune verdicts
/// derived from complete runtime evidence.
pub(super) struct GlobalCacheInventory {
    /// Diagnostic cache-shaped entries; the report shape is unchanged.
    pub entries: Vec<ContainerCacheGlobalEntry>,
    /// One verdict per listed candidate name.
    pub verdicts: BTreeMap<String, GlobalCachePruneVerdict>,
}

pub(super) fn collect_global_cache_inventory<F>(
    cwd: &Path,
    profile: &str,
    running_projects: &BTreeSet<String>,
    run_runtime_volume_capture: &F,
) -> Result<GlobalCacheInventory, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let listed = run_runtime_volume_capture(cwd, profile, &list_all_volumes_command())?;
    let names = parse_listed_volume_names(String::from_utf8_lossy(&listed.stdout).as_ref())
        .into_iter()
        .filter(|name| cache_kind_from_volume_name(name).is_some() || name.starts_with("efv-"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    // Deletion evidence must be complete: a failed or partial inspection is
    // an error, never missing metadata that reads as removable (contract 005).
    let metadata = inspect_runtime_volume_metadata_batch_strict(
        cwd,
        profile,
        &names,
        run_runtime_volume_capture,
    )?
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
    let entries = collect_global_cache_entries_from_names(
        names,
        running_projects,
        &metadata,
        &cache_kind_by_name,
        &usage_by_mount_point,
    );
    let verdicts = entries
        .iter()
        .map(|cache| {
            (
                cache.name.clone(),
                prune_verdict(&cache.name, &metadata, running_projects),
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok(GlobalCacheInventory { entries, verdicts })
}

/// The prune verdict for one listed candidate. Ownership comes only from the
/// volume's own Effigy labels; activity comes only from the discovered
/// running projects keyed by the labelled project.
fn prune_verdict(
    name: &str,
    metadata_by_name: &BTreeMap<String, effigy_catalog::volumes::RuntimeVolumeMetadata>,
    running_projects: &BTreeSet<String>,
) -> GlobalCachePruneVerdict {
    let Some(metadata) = metadata_by_name.get(name) else {
        // Complete strict inspection records every listed candidate, so this
        // is not reachable through the runtime path; unproven ownership is
        // kept intact rather than treated as removable.
        return GlobalCachePruneVerdict::Foreign(format!(
            "volume `{name}` has no complete inspection record"
        ));
    };
    match effigy_owned_project(name, metadata) {
        Ok(project) if running_projects.contains(&project) => {
            GlobalCachePruneVerdict::OwnedInUse { project }
        }
        Ok(_) => GlobalCachePruneVerdict::Removable,
        Err(reason) => GlobalCachePruneVerdict::Foreign(reason),
    }
}

/// The projects with discovered running environments: the authoritative
/// activity evidence for global cache pruning. A discovery failure is an
/// error, never an empty inventory (contract 052 R3: unknown grants nothing).
pub fn discovered_running_projects() -> Result<BTreeSet<String>, EffigyRuntimeError> {
    Ok(discover_running_environments()?
        .into_iter()
        .map(|environment| environment.policy.project_name)
        .collect())
}

pub(super) fn project_is_running(
    repo_root: &Path,
    project_name: &str,
) -> Result<bool, EffigyRuntimeError> {
    let target_root = match repo::canonicalize_or_original(repo_root) {
        repo::ResolvedPath::Canonical(path) => path,
        // Pruning reads `false` as "the stack is stopped" and then deletes its
        // volumes, so a root that could not be resolved must not answer `false`.
        repo::ResolvedPath::Unresolved { path, error } => {
            return Err(EffigyRuntimeError::task_invocation(format!(
                "failed to resolve {} while checking whether `{project_name}` is running: {error}; check that the checkout path exists and is readable, then retry",
                path.display()
            )));
        }
    };
    Ok(discover_running_environments()?
        .into_iter()
        .any(|environment| {
            repo::canonicalize_or_original(Path::new(&environment.repo_root)).path() == target_root
                && environment.policy.project_name == project_name
        }))
}

#[cfg(test)]
mod tests {
    use super::project_is_running;

    /// Cache prune deletes volumes when this answers `false`, so a root that
    /// could not be resolved must refuse rather than read as a stopped stack.
    #[test]
    fn an_unresolvable_root_refuses_instead_of_reporting_not_running() {
        let parent = tempfile::tempdir().expect("temp dir");
        let missing = parent.path().join("missing-checkout");

        let error = project_is_running(&missing, "web").expect_err("unresolved root must refuse");

        let rendered = error.to_string();
        assert!(rendered.contains("missing-checkout"), "{rendered}");
        assert!(rendered.contains("retry"), "{rendered}");
    }
}
