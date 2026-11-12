//! The host side of a Compose bind mount: create the directories a service
//! needs, and grant the container's own user the access it must have.

use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt};

use effigy_containers::parse_mount_parts;
use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;

pub(super) fn prepare_host_bind_mount_dirs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let runtime_data_root = repo_root.join(".effigy/runtime/data");
    for compose_file in &policy.compose_files {
        // A Compose input this preparation cannot read or parse is not a stack
        // whose bind mounts were prepared: applying it would start services
        // whose host directories are missing, so the reason is reported with
        // the file that carries it instead of being skipped.
        let raw = policy.read_compose(compose_file).map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot prepare the bind mounts of container `{}`: its Compose input {} could not be read ({error}); fix or remove that input, then re-run",
                policy.name,
                compose_file.display()
            ))
        })?;
        let yaml: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot prepare the bind mounts of container `{}`: its Compose input {} is not valid YAML ({error}); fix that input, then re-run",
                policy.name,
                compose_file.display()
            ))
        })?;
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
                    if matches!(
                        std::fs::symlink_metadata(&host_path),
                        Err(ref error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                            )
                    ) {
                        std::fs::create_dir_all(&host_path).map_err(|error| {
                            RunnerError::task_invocation(format!(
                                "cannot create bind mount directory {} for container `{}`: {error}; Compose will refuse to start the service until that path exists and is writable by you",
                                host_path.display(),
                                policy.name
                            ))
                        })?;
                    }
                    #[cfg(unix)]
                    if host_path.starts_with(&runtime_data_root) {
                        grant_runtime_data_access(&host_path)?;
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

/// Give the container's own user the access a runtime data bind mount needs.
///
/// The guest side of `.effigy/runtime/data/<service>` is written by the image's
/// user, not by the invoking host user: `mariadb` runs as its own uid 999. POSIX
/// mode bits can only grant a foreign uid access through the `other` class, so
/// the narrowest mode that lets such a service write is `0777` for a directory
/// (`0750` refuses its writes and the service cannot start) and `0666` for a
/// file the host created there. The widening is confined to that tree: every
/// other bind source keeps the mode the host gave it.
///
/// Entries the service's own user created are left alone — that uid already has
/// owner access, and the host cannot chmod them anyway. Symlinks are neither
/// followed nor widened, so no permission outside the tree is ever changed.
#[cfg(unix)]
pub(super) fn grant_runtime_data_access(root: &Path) -> Result<(), RunnerError> {
    if !root.exists() {
        return Ok(());
    }
    widen_runtime_data_entry(root)?;
    if fs::symlink_metadata(root)
        .map_err(|error| runtime_data_error(root, "inspect", error))?
        .is_dir()
    {
        for entry in walk_runtime_data(root)? {
            widen_runtime_data_entry(&entry)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn widen_runtime_data_entry(path: &Path) -> Result<(), RunnerError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| runtime_data_error(path, "inspect", error))?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    let mode = if metadata.is_dir() { 0o777 } else { 0o666 };
    match fs::set_permissions(path, fs::Permissions::from_mode(mode)) {
        Ok(()) => Ok(()),
        // The service created this entry, so its owner already has the access
        // this widening exists for, and only that owner may chmod it.
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Ok(()),
        Err(error) => Err(runtime_data_error(path, "widen the mode of", error)),
    }
}

#[cfg(unix)]
fn runtime_data_error(path: &Path, action: &str, error: std::io::Error) -> RunnerError {
    RunnerError::task_invocation(format!(
        "cannot {action} runtime data path {}: {error}; the container's service user must be able to write it, so fix that path's permissions and re-run",
        path.display()
    ))
}

#[cfg(unix)]
pub(super) fn walk_runtime_data(root: &Path) -> Result<Vec<PathBuf>, RunnerError> {
    let mut pending = vec![root.to_path_buf()];
    let mut discovered = Vec::new();
    while let Some(dir) = pending.pop() {
        let entries =
            fs::read_dir(&dir).map_err(|error| runtime_data_error(&dir, "list", error))?;
        for entry in entries {
            let entry = entry.map_err(|error| runtime_data_error(&dir, "list", error))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| runtime_data_error(&path, "inspect", error))?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path.clone());
            }
            discovered.push(path);
        }
    }
    Ok(discovered)
}

pub(super) fn parse_bind_mount_host_path(spec: &str) -> Option<&str> {
    let host = spec.split(':').next()?;
    if host.starts_with('/') || host.starts_with('.') || host.starts_with('~') {
        Some(host)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::parse_bind_mount_host_path;

    #[test]
    fn parse_bind_mount_host_path_accepts_bind_mounts_only() {
        assert_eq!(
            parse_bind_mount_host_path("/tmp/data:/var/lib/mysql"),
            Some("/tmp/data")
        );
        assert_eq!(
            parse_bind_mount_host_path("./runtime/mysql:/var/lib/mysql"),
            Some("./runtime/mysql")
        );
        assert_eq!(
            parse_bind_mount_host_path("~/runtime/mysql:/var/lib/mysql"),
            Some("~/runtime/mysql")
        );
        assert_eq!(
            parse_bind_mount_host_path("named-volume:/var/lib/mysql"),
            None
        );
    }
}
