//! Checkout paths, preflight and private host-process records.

use super::*;

/// Reject listener configurations and checkout paths before container or
/// child startup can create runtime effects.
pub(in crate::runner) fn validate_host_process_preflight(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if policy.host_processes.is_empty() {
        return Ok(());
    }
    let checkout_root = repo_root.canonicalize().map_err(|error| {
        RunnerError::task_invocation(format!("failed to resolve checkout: {error}"))
    })?;
    let ordered = order_host_processes(&policy.host_processes)?;
    for process in &ordered {
        resolve_host_process_cwd(&checkout_root, process)?;
    }
    listener_gateway_root(policy)?;
    Ok(())
}

pub(super) fn resolve_host_process_cwd(
    checkout_root: &Path,
    process: &EffectiveHostProcess,
) -> Result<PathBuf, RunnerError> {
    if process.cwd.as_os_str().is_empty() {
        return Ok(checkout_root.to_path_buf());
    }
    let requested = checkout_root.join(&process.cwd);
    let cwd = requested.canonicalize().map_err(|error| {
        RunnerError::task_invocation(format!(
            "managed host process `{}` working directory {} is unavailable: {error}",
            process.name,
            requested.display()
        ))
    })?;
    if !cwd.starts_with(checkout_root) {
        return Err(RunnerError::task_invocation(format!(
            "managed host process `{}` working directory escapes its checkout",
            process.name
        )));
    }
    Ok(cwd)
}

pub(super) fn host_process_dir(repo_root: &Path, container_name: &str) -> PathBuf {
    repo_root
        .join(HOST_PROCESS_DIR)
        .join(sanitize(container_name))
}

pub(super) fn managed_host_process_dir(
    repo_root: &Path,
    container_name: &str,
    profile: &str,
) -> PathBuf {
    host_process_dir(repo_root, container_name).join(sanitize(profile))
}

pub(super) fn host_process_dir_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> PathBuf {
    if policy
        .host_processes
        .iter()
        .any(|process| process.listener.is_some())
    {
        managed_host_process_dir(repo_root, &policy.name, &policy.profile)
    } else {
        host_process_dir(repo_root, &policy.name)
    }
}

pub(super) fn create_host_process_dir(repo_root: &Path, dir: &Path) -> Result<(), RunnerError> {
    let relative = dir.strip_prefix(repo_root).map_err(|_| {
        RunnerError::task_invocation(
            "managed host process runtime directory is outside its canonical checkout",
        )
    })?;
    let mut current = repo_root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(RunnerError::task_invocation(
                "managed host process runtime directory contains an unsafe path component",
            ));
        }
        current.push(component.as_os_str());
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => {
                        return Err(RunnerError::task_invocation_failed_write(&current, error))
                    }
                }
                fs::symlink_metadata(&current)
                    .map_err(|error| RunnerError::task_invocation_failed_read(&current, error))?
            }
            Err(error) => return Err(RunnerError::task_invocation_failed_read(&current, error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(RunnerError::task_invocation(format!(
                "managed host process runtime path {} is not a real directory",
                current.display()
            )));
        }
    }
    if fs::canonicalize(dir).ok().as_deref() != Some(dir) {
        return Err(RunnerError::task_invocation(format!(
            "managed host process runtime directory {} is not canonical",
            dir.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| RunnerError::task_invocation_failed_write(dir, error))?;
    }
    Ok(())
}

pub(super) fn host_process_state_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.listener.json", sanitize(name)))
}

/// Gateway state root that receives managed listener routes: the caller's
/// validated private root when `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT` is set,
/// otherwise the shared operator gateway.
pub(super) fn listener_gateway_root(
    policy: &EffectiveContainerPolicy,
) -> Result<Option<PathBuf>, RunnerError> {
    policy
        .host_processes
        .iter()
        .any(|process| process.listener.is_some())
        .then(crate::runner::gateway_command::gateway_dir)
        .transpose()
}

/// Re-check a recorded listener root before use. Private roots keep their
/// ownership and permission checks; the shared gateway directory is the
/// operator's own state and is not a caller-owned private root.
pub(super) fn validate_listener_gateway_root(root: &Path) -> Result<(), RunnerError> {
    if root == crate::runner::gateway_command::standard_gateway_dir()? {
        return Ok(());
    }
    effigy_gateway::private_state::validate_root(root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

pub(super) fn managed_host_process_owner(
    project_path: &str,
    policy: &EffectiveContainerPolicy,
    process: &EffectiveHostProcess,
) -> String {
    format!("{project_path}|{}|{}", policy.name, process.name)
}

pub(in crate::runner) fn write_private_json(
    path: &Path,
    value: &impl Serialize,
) -> Result<(), RunnerError> {
    let rendered = serde_json::to_vec_pretty(value)
        .map_err(|error| RunnerError::task_invocation_failed_render(path, error))?;
    let temp = path.with_extension(format!(
        "json.tmp-{}-{}",
        std::process::id(),
        random_token()?
    ));
    fs::write(&temp, rendered)
        .map_err(|error| RunnerError::task_invocation_failed_write(&temp, error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))
            .map_err(|error| RunnerError::task_invocation_failed_write(&temp, error))?;
    }
    fs::rename(&temp, path).map_err(|error| RunnerError::task_invocation_failed_write(path, error))
}

pub(super) fn random_token() -> Result<String, RunnerError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot obtain a managed listener generation token: {error}"
        ))
    })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(super) fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

pub(super) fn read_private_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<T, RunnerError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| RunnerError::task_invocation_failed_read(path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 1_048_576 {
        return Err(RunnerError::task_invocation(format!(
            "managed host process record {} is not a bounded regular file",
            path.display()
        )));
    }
    let raw =
        fs::read(path).map_err(|error| RunnerError::task_invocation_failed_read(path, error))?;
    serde_json::from_slice(&raw)
        .map_err(|error| RunnerError::task_invocation_failed_parse(path, error))
}
