use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root;
use effigy_manifest::{LibraryMount, ManifestContainerConfig, ManifestWorkspaceConfig};

use crate::ContainerPolicyError;

use super::{
    build_workspace_runtime_environment, build_workspace_runtime_mounts, RenderedWorkspaceMount,
};

pub(super) fn rewrite_workspace_mounts_for_direct_compose(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    workspace: &ManifestWorkspaceConfig,
    primary_service: &str,
    source_compose: &Path,
    working_dir: &Path,
    library_mounts: &[LibraryMount],
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
    let injected_mounts = build_workspace_runtime_mounts(
        repo_root,
        container_name,
        config,
        workspace,
        primary_service,
        working_dir,
        library_mounts,
    )?;
    inject_workspace_named_volumes(&mut parsed, &injected_mounts);
    let injected_env = build_workspace_runtime_environment(repo_root, config, primary_service)?;
    {
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
        rewrite_workspace_service_volumes(
            service,
            compose_dir,
            repo_root,
            workspace_root,
            working_dir,
            &injected_mounts,
        )?;
        if !injected_env.is_empty() {
            inject_workspace_service_environment(service, &injected_env);
        }
    }
    compact_workspace_named_volume_mounts(&mut parsed, primary_service);
    normalize_runtime_rewrite_paths(&mut parsed, compose_dir);

    let rewrite_dir = repo_root.join(".effigy").join("runtime").join("compose");
    ensure_effigy_ignored_in_git_root(repo_root).map_err(|error| ContainerPolicyError::Read {
        path: repo_root.join(".gitignore"),
        error,
    })?;
    std::fs::create_dir_all(&rewrite_dir).map_err(|error| ContainerPolicyError::Read {
        path: rewrite_dir.clone(),
        error,
    })?;
    let rewrite_path = rewrite_dir.join(format!("{container_name}.workspace.compose.yml"));
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

#[cfg_attr(test, allow(dead_code))]
pub(super) fn inject_workspace_service_environment(
    service: &mut serde_yaml::Mapping,
    additions: &std::collections::BTreeMap<String, String>,
) {
    if additions.is_empty() {
        return;
    }
    let key = serde_yaml::Value::String("environment".to_owned());
    let entry = service
        .entry(key)
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    match entry {
        serde_yaml::Value::Mapping(mapping) => {
            for (name, value) in additions {
                let env_key = serde_yaml::Value::String(name.clone());
                if !mapping.contains_key(&env_key) {
                    mapping.insert(env_key, serde_yaml::Value::String(value.clone()));
                }
            }
        }
        serde_yaml::Value::Sequence(sequence) => {
            let existing: std::collections::BTreeSet<String> = sequence
                .iter()
                .filter_map(|item| item.as_str())
                .map(|raw| {
                    raw.split_once('=')
                        .map(|(k, _)| k.to_owned())
                        .unwrap_or_else(|| raw.to_owned())
                })
                .collect();
            for (name, value) in additions {
                if !existing.contains(name) {
                    sequence.push(serde_yaml::Value::String(format!("{name}={value}")));
                }
            }
        }
        _ => {
            let mut mapping = serde_yaml::Mapping::new();
            for (name, value) in additions {
                mapping.insert(
                    serde_yaml::Value::String(name.clone()),
                    serde_yaml::Value::String(value.clone()),
                );
            }
            *entry = serde_yaml::Value::Mapping(mapping);
        }
    }
}

fn inject_workspace_named_volumes(
    parsed: &mut serde_yaml::Value,
    mounts: &[RenderedWorkspaceMount],
) {
    let names = mounts
        .iter()
        .filter_map(|mount| mount.named_volume.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    if names.is_empty() {
        return;
    }
    let Some(root) = parsed.as_mapping_mut() else {
        return;
    };
    let key = serde_yaml::Value::String("volumes".to_owned());
    let volumes = root
        .entry(key)
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    let Some(mapping) = volumes.as_mapping_mut() else {
        return;
    };
    for name in names {
        let entry_key = serde_yaml::Value::String(name.to_owned());
        mapping.entry(entry_key).or_insert_with(|| {
            let mut entry = serde_yaml::Mapping::new();
            entry.insert(
                serde_yaml::Value::String("name".to_owned()),
                serde_yaml::Value::String(name.to_owned()),
            );
            serde_yaml::Value::Mapping(entry)
        });
    }
}

fn compact_workspace_named_volume_mounts(parsed: &mut serde_yaml::Value, primary_service: &str) {
    let Some(root) = parsed.as_mapping_mut() else {
        return;
    };
    let service_key = serde_yaml::Value::String(primary_service.to_owned());
    let mut renamed: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    {
        let Some(service) = root
            .get_mut(serde_yaml::Value::String("services".to_owned()))
            .and_then(serde_yaml::Value::as_mapping_mut)
            .and_then(|services| services.get_mut(&service_key))
            .and_then(serde_yaml::Value::as_mapping_mut)
        else {
            return;
        };
        let Some(volumes) = service
            .get_mut(serde_yaml::Value::String("volumes".to_owned()))
            .and_then(serde_yaml::Value::as_sequence_mut)
        else {
            return;
        };
        for entry in volumes.iter_mut() {
            let Some(raw) = entry.as_str() else {
                continue;
            };
            let Some((source, target, options)) = parse_mount_parts(raw) else {
                continue;
            };
            if looks_like_bind_mount_source(source) {
                continue;
            }
            let short = renamed
                .entry(source.to_owned())
                .or_insert_with(|| compact_named_volume_name(source))
                .clone();
            let rendered = match options.filter(|value| !value.is_empty()) {
                Some(options) => format!("{short}:{target}:{options}"),
                None => format!("{short}:{target}"),
            };
            *entry = serde_yaml::Value::String(rendered);
        }
    }
    if renamed.is_empty() {
        return;
    }
    let volumes_key = serde_yaml::Value::String("volumes".to_owned());
    let Some(volumes_root) = root
        .entry(volumes_key)
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()))
        .as_mapping_mut()
    else {
        return;
    };
    for (original, short) in renamed {
        let original_key = serde_yaml::Value::String(original);
        let short_key = serde_yaml::Value::String(short.clone());
        let mut entry = volumes_root
            .get(&original_key)
            .and_then(serde_yaml::Value::as_mapping)
            .cloned()
            .unwrap_or_default();
        entry.insert(
            serde_yaml::Value::String("name".to_owned()),
            serde_yaml::Value::String(short.clone()),
        );
        volumes_root.insert(short_key, serde_yaml::Value::Mapping(entry));
    }
}

fn compact_named_volume_name(source: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    format!("efv-{:016x}", hasher.finish())
}

fn parse_mount_parts(mount: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = mount.splitn(3, ':');
    let source = parts.next()?.trim();
    let target = parts.next()?.trim();
    let options = parts.next().map(str::trim);
    Some((source, target, options))
}

pub(super) fn compose_volume_ownership_target(entry: &serde_yaml::Value) -> Option<String> {
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

fn normalize_runtime_rewrite_paths(parsed: &mut serde_yaml::Value, compose_dir: &Path) {
    let Some(services) = parsed
        .get_mut("services")
        .and_then(serde_yaml::Value::as_mapping_mut)
    else {
        return;
    };
    for service in services.values_mut() {
        let Some(service) = service.as_mapping_mut() else {
            continue;
        };
        normalize_service_build_paths(service, compose_dir);
        normalize_service_env_file_paths(service, compose_dir);
        normalize_service_volume_sources(service, compose_dir);
    }
}

fn normalize_service_build_paths(service: &mut serde_yaml::Mapping, compose_dir: &Path) {
    let Some(build) = service.get_mut(serde_yaml::Value::String("build".to_owned())) else {
        return;
    };
    match build {
        serde_yaml::Value::String(path) => {
            if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                *path = normalized;
            }
        }
        serde_yaml::Value::Mapping(mapping) => {
            let key = serde_yaml::Value::String("context".to_owned());
            if let Some(serde_yaml::Value::String(path)) = mapping.get_mut(&key) {
                if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                    *path = normalized;
                }
            }
        }
        _ => {}
    }
}

fn normalize_service_env_file_paths(service: &mut serde_yaml::Mapping, compose_dir: &Path) {
    let Some(env_file) = service.get_mut(serde_yaml::Value::String("env_file".to_owned())) else {
        return;
    };
    match env_file {
        serde_yaml::Value::String(path) => {
            if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                *path = normalized;
            }
        }
        serde_yaml::Value::Sequence(entries) => {
            for entry in entries {
                if let serde_yaml::Value::String(path) = entry {
                    if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                        *path = normalized;
                    }
                }
            }
        }
        _ => {}
    }
}

fn normalize_service_volume_sources(service: &mut serde_yaml::Mapping, compose_dir: &Path) {
    let Some(serde_yaml::Value::Sequence(volumes)) =
        service.get_mut(serde_yaml::Value::String("volumes".to_owned()))
    else {
        return;
    };
    for volume in volumes {
        let Some(raw) = volume.as_str() else {
            continue;
        };
        let Some((source, target, options)) = parse_mount_parts(raw) else {
            continue;
        };
        let Some(normalized_source) = normalize_bind_mount_source(source, compose_dir) else {
            continue;
        };
        let mut rendered = format!("{normalized_source}:{target}");
        if let Some(options) = options.filter(|value| !value.is_empty()) {
            rendered.push(':');
            rendered.push_str(options);
        }
        *volume = serde_yaml::Value::String(rendered);
    }
}

fn normalize_bind_mount_source(source: &str, compose_dir: &Path) -> Option<String> {
    looks_like_bind_mount_source(source).then(|| render_compose_relative_path(source, compose_dir))
}

fn normalize_compose_relative_path(path: &str, compose_dir: &Path) -> Option<String> {
    (!path.is_empty() && !Path::new(path).is_absolute() && !looks_like_remote_build_context(path))
        .then(|| render_compose_relative_path(path, compose_dir))
}

fn render_compose_relative_path(path: &str, compose_dir: &Path) -> String {
    let resolved = compose_dir.join(path);
    resolved
        .canonicalize()
        .unwrap_or(resolved)
        .display()
        .to_string()
}

fn looks_like_remote_build_context(path: &str) -> bool {
    path.contains("://") || path.starts_with("git@")
}
