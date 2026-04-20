use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use effigy_manifest::ManifestWorkspaceConfig;

use crate::{ContainerPolicyError, EffectiveContainerPolicy};

pub(crate) fn materialize_runtime_workspace_mount_rewrite(
    repo_root: &Path,
    container_name: &str,
    workspace: &ManifestWorkspaceConfig,
    working_dir: &Path,
    primary_service: &str,
    compose_files: &mut Vec<PathBuf>,
) -> Result<(), ContainerPolicyError> {
    let Some(source_compose) = compose_files.first().cloned() else {
        return Ok(());
    };
    let rewritten = rewrite_workspace_mounts_for_direct_compose(
        repo_root,
        container_name,
        workspace,
        primary_service,
        &source_compose,
        working_dir,
    )?;
    compose_files[0] = rewritten;
    Ok(())
}

pub fn load_workspace_ownership_targets(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, ContainerPolicyError> {
    let mut targets = std::collections::BTreeSet::new();
    if let Some(home) = policy
        .workspace_home
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        targets.insert(home.to_owned());
    }
    for compose_file in &policy.compose_files {
        let content =
            std::fs::read_to_string(compose_file).map_err(|error| ContainerPolicyError::Read {
                path: compose_file.clone(),
                error,
            })?;
        let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to parse compose file {} for workspace ownership targets: {error}",
                compose_file.display()
            ))
        })?;
        let Some(service) = parsed
            .get("services")
            .and_then(|services| services.get(policy.primary_service.as_str()))
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        let Some(volumes) = service
            .get("volumes")
            .and_then(serde_yaml::Value::as_sequence)
        else {
            continue;
        };
        for target in volumes
            .iter()
            .filter_map(compose_volume_ownership_target)
            .filter(|value| !value.trim().is_empty())
        {
            targets.insert(target);
        }
    }
    Ok(targets.into_iter().collect())
}

#[derive(Debug, Clone)]
struct RenderedWorkspaceMount {
    target: String,
    rendered: String,
}

fn rewrite_workspace_mounts_for_direct_compose(
    repo_root: &Path,
    container_name: &str,
    workspace: &ManifestWorkspaceConfig,
    primary_service: &str,
    source_compose: &Path,
    working_dir: &Path,
) -> Result<PathBuf, ContainerPolicyError> {
    let workspace_root = working_dir.parent().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace exec working dir `{}` must have a parent directory",
            working_dir.display()
        ))
    })?;
    let compose_dir = source_compose
        .parent()
        .ok_or_else(|| ContainerPolicyError::Read {
            path: source_compose.to_path_buf(),
            error: std::io::Error::other("compose file has no parent directory"),
        })?;
    let content =
        std::fs::read_to_string(source_compose).map_err(|error| ContainerPolicyError::Read {
            path: source_compose.to_path_buf(),
            error,
        })?;
    let mut parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to parse compose file {} for workspace mount rewrite: {error}",
            source_compose.display()
        ))
    })?;
    let services = parsed
        .get_mut("services")
        .and_then(serde_yaml::Value::as_mapping_mut)
        .ok_or_else(|| {
            ContainerPolicyError::TaskInvocation(format!(
                "compose file {} is missing a `services` mapping for workspace mount rewrite",
                source_compose.display()
            ))
        })?;
    let service = services
        .get_mut(serde_yaml::Value::String(primary_service.to_owned()))
        .and_then(serde_yaml::Value::as_mapping_mut)
        .ok_or_else(|| {
            ContainerPolicyError::TaskInvocation(format!(
                "compose file {} does not define primary service `{primary_service}` for workspace mount rewrite",
                source_compose.display()
            ))
        })?;
    let injected_mounts =
        build_workspace_runtime_mounts(repo_root, container_name, workspace, working_dir)?;
    rewrite_workspace_service_volumes(
        service,
        compose_dir,
        repo_root,
        workspace_root,
        working_dir,
        &injected_mounts,
    )?;

    let rewrite_path = compose_dir.join(format!(".effigy-{container_name}.workspace.compose.yml"));
    let rendered = serde_yaml::to_string(&parsed).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to serialize workspace mount rewrite for `{container_name}`: {error}"
        ))
    })?;
    std::fs::write(&rewrite_path, rendered).map_err(|error| ContainerPolicyError::Read {
        path: rewrite_path.clone(),
        error,
    })?;
    Ok(rewrite_path)
}

fn build_workspace_runtime_mounts(
    repo_root: &Path,
    container_name: &str,
    workspace: &ManifestWorkspaceConfig,
    working_dir: &Path,
) -> Result<Vec<RenderedWorkspaceMount>, ContainerPolicyError> {
    let workspace_root = working_dir.parent().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace exec working dir `{}` must have a parent directory",
            working_dir.display()
        ))
    })?;
    let canonical_repo_root =
        repo_root
            .canonicalize()
            .map_err(|error| ContainerPolicyError::Read {
                path: repo_root.to_path_buf(),
                error,
            })?;
    let mut mounts = vec![RenderedWorkspaceMount {
        target: working_dir.display().to_string(),
        rendered: format!(
            "{}:{}",
            canonical_repo_root.display(),
            working_dir.display()
        ),
    }];
    for raw in &workspace.mounts {
        mounts.push(parse_workspace_extra_mount(
            repo_root,
            container_name,
            workspace_root,
            raw,
        )?);
    }
    Ok(mounts)
}

fn parse_workspace_extra_mount(
    repo_root: &Path,
    container_name: &str,
    workspace_root: &Path,
    raw: &str,
) -> Result<RenderedWorkspaceMount, ContainerPolicyError> {
    let mut parts = raw.splitn(3, ':');
    let source_raw = parts.next().unwrap_or_default().trim();
    let target_raw = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let options = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if source_raw.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace extra mount `{raw}` is invalid: source path is empty"
        )));
    }
    let source_path = Path::new(source_raw);
    let resolved_source = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        repo_root.join(source_path)
    };
    let canonical_source = resolved_source.canonicalize().map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace extra mount source `{source_raw}` is invalid: {error}"
        ))
    })?;
    let target = if let Some(target) = target_raw {
        target.to_owned()
    } else {
        let basename = canonical_source
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ContainerPolicyError::TaskInvocation(format!(
                    "container `{container_name}` workspace extra mount `{raw}` must declare an explicit target because the source has no basename"
                ))
            })?;
        workspace_root.join(basename).display().to_string()
    };
    let mut rendered = format!("{}:{target}", canonical_source.display());
    if let Some(options) = options {
        rendered.push(':');
        rendered.push_str(options);
    }
    Ok(RenderedWorkspaceMount { target, rendered })
}

fn rewrite_workspace_service_volumes(
    service: &mut serde_yaml::Mapping,
    compose_dir: &Path,
    repo_root: &Path,
    workspace_root: &Path,
    working_dir: &Path,
    injected_mounts: &[RenderedWorkspaceMount],
) -> Result<(), ContainerPolicyError> {
    let key = serde_yaml::Value::String("volumes".to_owned());
    let volumes = match service.get_mut(&key) {
        Some(serde_yaml::Value::Sequence(sequence)) => sequence,
        Some(_) => {
            return Err(ContainerPolicyError::TaskInvocation(
                "workspace mount rewrite only supports sequence `volumes` entries".to_owned(),
            ))
        }
        None => {
            service.insert(key.clone(), serde_yaml::Value::Sequence(Vec::new()));
            service
                .get_mut(&key)
                .and_then(serde_yaml::Value::as_sequence_mut)
                .expect("volumes sequence should exist after insertion")
        }
    };

    let canonical_repo_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let workspace_root_rendered = workspace_root.display().to_string();
    let working_dir_rendered = working_dir.display().to_string();
    let injected_targets = injected_mounts
        .iter()
        .map(|mount| mount.target.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut rewritten = injected_mounts
        .iter()
        .map(|mount| serde_yaml::Value::String(mount.rendered.clone()))
        .collect::<Vec<_>>();

    for entry in volumes.iter() {
        let Some(raw) = entry.as_str() else {
            rewritten.push(entry.clone());
            continue;
        };
        let Some((source, target, _options)) = parse_mount_parts(raw) else {
            rewritten.push(entry.clone());
            continue;
        };
        if injected_targets.contains(target) {
            continue;
        }
        if target == workspace_root_rendered {
            if let Some(canonical_source) = resolve_bind_mount_source(compose_dir, source) {
                if canonical_repo_root.starts_with(&canonical_source) {
                    continue;
                }
            }
        }
        if target == working_dir_rendered {
            if let Some(canonical_source) = resolve_bind_mount_source(compose_dir, source) {
                if canonical_source == canonical_repo_root {
                    continue;
                }
            }
        }
        rewritten.push(entry.clone());
    }

    *volumes = rewritten;
    Ok(())
}

fn parse_mount_parts(mount: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = mount.splitn(3, ':');
    let source = parts.next()?.trim();
    let target = parts.next()?.trim();
    let options = parts.next().map(str::trim);
    Some((source, target, options))
}

fn compose_volume_ownership_target(entry: &serde_yaml::Value) -> Option<String> {
    let raw = entry.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some((source, target, _options)) = parse_mount_parts(raw) {
        if looks_like_bind_mount_source(source) {
            return None;
        }
        return Some(target.to_owned());
    }
    raw.starts_with('/').then(|| raw.to_owned())
}

fn resolve_bind_mount_source(compose_dir: &Path, source: &str) -> Option<PathBuf> {
    if !looks_like_bind_mount_source(source) {
        return None;
    }
    let source_path = Path::new(source);
    let resolved = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        compose_dir.join(source_path)
    };
    Some(resolved.canonicalize().unwrap_or(resolved))
}

fn looks_like_bind_mount_source(source: &str) -> bool {
    source.starts_with('/')
        || source.starts_with("./")
        || source.starts_with("../")
        || source == "."
        || source == ".."
        || source.contains('/')
}
