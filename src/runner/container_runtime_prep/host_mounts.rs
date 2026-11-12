//! The host side of a Compose bind mount: create the directories a service
//! needs, and grant the container's own user the access it must have.

use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt};

use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;

pub(super) fn prepare_host_bind_mount_dirs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let runtime_data_root = repo_root.join(".effigy/runtime/data");
    for compose_file in &policy.compose_files {
        let raw = match policy.read_compose(compose_file) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let yaml: serde_yaml::Value = match serde_yaml::from_str(&raw) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(services) = yaml.get("services").and_then(|s| s.as_mapping()) else {
            continue;
        };
        for (_service_name, service_value) in services {
            let Some(volumes) = service_value.get("volumes").and_then(|v| v.as_sequence()) else {
                continue;
            };
            let bind_roots = volumes
                .iter()
                .filter_map(|volume| volume.as_str())
                .filter_map(|spec| bind_mount_root(repo_root, spec))
                .collect::<Vec<_>>();
            for volume in volumes {
                let Some(spec) = volume.as_str() else {
                    continue;
                };
                for host_path in candidate_host_mount_paths(repo_root, spec, &bind_roots) {
                    if let Some(extension) = host_path.extension() {
                        let ext_str = extension.to_string_lossy();
                        if matches!(
                            ext_str.as_ref(),
                            "conf" | "yml" | "yaml" | "toml" | "json" | "sql" | "ini" | "env"
                        ) {
                            continue;
                        }
                    }
                    let _ = std::fs::create_dir_all(&host_path);
                    #[cfg(unix)]
                    if host_path.starts_with(&runtime_data_root) {
                        let _ = relax_runtime_data_permissions(&host_path);
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn candidate_host_mount_paths(
    repo_root: &Path,
    spec: &str,
    bind_roots: &[(PathBuf, String)],
) -> Vec<PathBuf> {
    if let Some(host_path) = parse_bind_mount_host_path(spec) {
        let host_path = Path::new(host_path);
        if host_path.is_absolute() && host_path.starts_with(repo_root) {
            return vec![host_path.to_path_buf()];
        }
        return Vec::new();
    }

    let Some((_source, target, _options)) = parse_mount_parts(spec) else {
        return Vec::new();
    };
    let target_path = Path::new(target);
    let Some((bind_source, _bind_target, suffix)) = bind_roots
        .iter()
        .filter_map(|(source, mounted_target)| {
            target_path
                .strip_prefix(Path::new(mounted_target))
                .ok()
                .map(|suffix| (source, mounted_target, suffix))
        })
        .max_by_key(|(_source, mounted_target, _suffix)| mounted_target.len())
    else {
        return Vec::new();
    };

    if !bind_source.starts_with(repo_root) {
        return Vec::new();
    }
    vec![bind_source.join(suffix)]
}

pub(super) fn bind_mount_root(repo_root: &Path, spec: &str) -> Option<(PathBuf, String)> {
    let host_path = parse_bind_mount_host_path(spec)?;
    let (source, target, _options) = parse_mount_parts(spec)?;
    let host_path = Path::new(host_path);
    if !host_path.is_absolute() || !host_path.starts_with(repo_root) {
        return None;
    }
    Some((PathBuf::from(source), target.to_owned()))
}

#[cfg(unix)]
pub(super) fn relax_runtime_data_permissions(root: &Path) -> std::io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    relax_runtime_data_permissions_entry(root)?;
    if root.is_dir() {
        for entry in walk_runtime_data(root)? {
            relax_runtime_data_permissions_entry(&entry)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn walk_runtime_data(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut discovered = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path.clone());
            }
            discovered.push(path);
        }
    }
    Ok(discovered)
}

#[cfg(unix)]
pub(super) fn relax_runtime_data_permissions_entry(path: &Path) -> std::io::Result<()> {
    let metadata = fs::metadata(path)?;
    let mode = if metadata.is_dir() { 0o777 } else { 0o666 };
    let permissions = fs::Permissions::from_mode(mode);
    fs::set_permissions(path, permissions)
}

pub(super) fn parse_bind_mount_host_path(spec: &str) -> Option<&str> {
    let host = spec.split(':').next()?;
    if host.starts_with('/') || host.starts_with('.') || host.starts_with('~') {
        Some(host)
    } else {
        None
    }
}

pub(super) fn parse_mount_parts(spec: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = spec.splitn(3, ':');
    let source = parts.next()?.trim();
    let target = parts.next()?.trim();
    let options = parts.next().map(str::trim);
    Some((source, target, options))
}
