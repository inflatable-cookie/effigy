use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use effigy_manifest::{load_task_manifest, ManifestWorkspaceConfig};

use crate::ContainerPolicyError;

use super::RenderedWorkspaceMount;

pub(crate) fn build_isolation_mounts(
    repo_root: &Path,
    container_name: &str,
    workspace: &ManifestWorkspaceConfig,
    current_mounts: &[RenderedWorkspaceMount],
) -> Result<Vec<RenderedWorkspaceMount>, ContainerPolicyError> {
    let mut mounts = Vec::new();
    let mut adopted_roots: Vec<(String, PathBuf)> = Vec::new();
    for adoption in &workspace.isolation {
        let producer_root =
            resolve_adopted_isolation_repo(repo_root, container_name, &adoption.repo)?;
        adopted_roots.push((adoption.repo.clone(), producer_root));
    }

    for mount in current_mounts {
        let Some(source) = mount.source.as_ref() else {
            continue;
        };
        if source == repo_root {
            continue;
        }
        let manifest_path = source.join("effigy.toml");
        if !manifest_path.is_file() {
            continue;
        }
        if adopted_roots.iter().any(|(_, root)| root == source) {
            continue;
        }
        adopted_roots.push((source.display().to_string(), source.clone()));
    }

    for (adoption_repo, producer_root) in adopted_roots {
        let producer_manifest_path = producer_root.join("effigy.toml");
        let manifest = load_task_manifest(&producer_manifest_path).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to load isolation contract from `{}` for container `{container_name}`: {error}",
                producer_manifest_path.display()
            ))
        })?;
        let isolation_paths = manifest
            .isolation
            .as_ref()
            .map(|config| config.paths.as_slice())
            .unwrap_or(&[]);
        if isolation_paths.is_empty() {
            continue;
        }
        let target_root = current_mounts
            .iter()
            .find_map(|mount| {
                mount
                    .source
                    .as_ref()
                    .filter(|source| **source == producer_root)
                    .map(|_| mount.target.as_str())
            })
            .ok_or_else(|| {
                ContainerPolicyError::TaskInvocation(format!(
                    "system isolation repo `{}` for container `{container_name}` is not mounted into the workspace runtime",
                    adoption_repo
                ))
            })?;

        for relative in isolation_paths {
            let normalized = normalize_isolation_relative_path(
                &producer_manifest_path,
                relative,
                container_name,
            )?;
            let volume_name = isolation_volume_name(container_name, &producer_root, &normalized);
            let target = Path::new(target_root)
                .join(&normalized)
                .display()
                .to_string();
            mounts.push(RenderedWorkspaceMount {
                target: target.clone(),
                rendered: format!("{volume_name}:{target}"),
                source: None,
                named_volume: Some(volume_name),
            });
        }
    }
    Ok(mounts)
}

fn resolve_adopted_isolation_repo(
    repo_root: &Path,
    container_name: &str,
    raw_repo: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let repo = raw_repo.trim();
    if repo.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` system isolation repo entry must not be empty"
        )));
    }
    let resolved = if Path::new(repo).is_absolute() {
        PathBuf::from(repo)
    } else {
        repo_root.join(repo)
    };
    resolved.canonicalize().map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` system isolation repo `{repo}` is invalid: {error}"
        ))
    })
}

fn normalize_isolation_relative_path(
    manifest_path: &Path,
    raw_path: &str,
    container_name: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "isolation path in {} for container `{container_name}` must not be empty",
            manifest_path.display()
        )));
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "isolation path `{trimmed}` in {} for container `{container_name}` must be relative",
            manifest_path.display()
        )));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(value) => normalized.push(value),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(ContainerPolicyError::TaskInvocation(format!(
                    "isolation path `{trimmed}` in {} for container `{container_name}` must stay under the producer repo root",
                    manifest_path.display()
                )))
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "isolation path `{trimmed}` in {} for container `{container_name}` resolved to an empty path",
            manifest_path.display()
        )));
    }
    Ok(normalized)
}

fn isolation_volume_name(
    container_name: &str,
    producer_root: &Path,
    relative_path: &Path,
) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    container_name.hash(&mut hasher);
    producer_root.hash(&mut hasher);
    relative_path.hash(&mut hasher);
    format!("efi-iso-{:016x}", hasher.finish())
}
