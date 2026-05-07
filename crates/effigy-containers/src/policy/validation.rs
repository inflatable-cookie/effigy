use std::path::Path;

use effigy_manifest::ManifestContainerDriver;

use crate::compose;
use crate::policy_support::validate_media_mounts;
use crate::{driver_label, NERDCTL_MOUNTS_LABEL_BUDGET_BYTES};

use super::model::{ContainerPolicyError, EffectiveContainerPolicy};

pub fn validate_container_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    if policy.driver != ManifestContainerDriver::Colima {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{}` uses unsupported driver `{}`; v1 only supports `colima`",
            policy.name,
            driver_label(policy.driver)
        )));
    }
    for compose_file in &policy.compose_files {
        if !compose_file.is_file() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{}` compose_file not found: {}",
                policy.name,
                compose_file.display()
            )));
        }
    }
    // Host mounts are resolved + validated at intake (see `mount_spec`).
    // `policy.declared_mounts` already contains canonical absolute paths.
    validate_media_mounts(repo_root, &policy.name, &policy.declared_media_mounts)?;
    Ok(())
}

/// Verify the resolved compose backend can actually reach the repo host paths.
///
/// This is the runtime preflight that rejects Colima-nerdctl fallback runs
/// against repos living under temp directories the Colima VM may not share.
/// It is NOT part of static manifest validation — call it only from code
/// paths that will actually drive `docker compose`.
pub fn validate_compose_backend_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    validate_compose_backend_host_paths(repo_root, policy)?;
    validate_compose_backend_mount_budget(policy)
}

fn validate_compose_backend_host_paths(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    if compose::resolve_compose_backend_for_repo(repo_root, policy)
        != compose::ComposeBackend::ColimaNerdctl
    {
        return Ok(());
    }
    if std::env::var_os("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK").is_some()
        || std::env::var_os("EFFIGY_TEST_COLIMA_ARGS_FILE").is_some()
    {
        return Ok(());
    }
    if !is_colima_temp_root_path(repo_root)
        && !policy
            .compose_files
            .iter()
            .any(|compose_file| is_colima_temp_root_path(compose_file))
    {
        return Ok(());
    }
    Err(ContainerPolicyError::TaskInvocation(format!(
        "container `{}` uses the Colima nerdctl compose fallback, but repo `{}` is under a temp directory that Colima may not share into the VM; move the repo under a shared path like `/Users/...` and retry",
        policy.name,
        repo_root.display()
    )))
}

fn validate_compose_backend_mount_budget(
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    if compose::resolve_compose_backend_for_repo(Path::new("."), policy)
        != compose::ComposeBackend::ColimaNerdctl
    {
        return Ok(());
    }
    let Some(estimate) = estimate_primary_service_mount_label_size(
        &policy.compose_files[0],
        &policy.primary_service,
    )?
    else {
        return Ok(());
    };
    if estimate.total_bytes < NERDCTL_MOUNTS_LABEL_BUDGET_BYTES {
        return Ok(());
    }
    let heaviest = estimate
        .entries
        .iter()
        .rev()
        .take(6)
        .map(|entry| format!("{} ({} bytes)", entry.target, entry.raw.len()))
        .collect::<Vec<_>>()
        .join(", ");
    Err(ContainerPolicyError::TaskInvocation(format!(
        "container `{}` uses the Colima nerdctl compose fallback, but primary service `{}` has an estimated mount payload of {} bytes across {} mounts, which exceeds the nerdctl/containerd label budget of {} bytes; trim isolation or workspace mounts. Heaviest targets: {}",
        policy.name,
        policy.primary_service,
        estimate.total_bytes,
        estimate.entries.len(),
        NERDCTL_MOUNTS_LABEL_BUDGET_BYTES,
        heaviest
    )))
}

fn is_colima_temp_root_path(path: &Path) -> bool {
    let temp_root = std::env::temp_dir();
    path_is_within(path, &temp_root)
        || path_is_within(path, Path::new("/tmp"))
        || path_is_within(path, Path::new("/private/tmp"))
        || path_is_within(path, Path::new("/var/folders"))
        || path_is_within(path, Path::new("/private/var/folders"))
}

#[derive(Debug)]
struct PrimaryServiceMountEstimate {
    total_bytes: usize,
    entries: Vec<MountBudgetEntry>,
}

#[derive(Debug)]
struct MountBudgetEntry {
    target: String,
    raw: String,
}

fn estimate_primary_service_mount_label_size(
    compose_file: &Path,
    primary_service: &str,
) -> Result<Option<PrimaryServiceMountEstimate>, ContainerPolicyError> {
    let content =
        std::fs::read_to_string(compose_file).map_err(|error| ContainerPolicyError::Read {
            path: compose_file.to_path_buf(),
            error,
        })?;
    let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to parse compose file {} while estimating nerdctl mount budget: {error}",
            compose_file.display()
        ))
    })?;
    let Some(service) = parsed
        .get("services")
        .and_then(serde_yaml::Value::as_mapping)
        .and_then(|services| services.get(serde_yaml::Value::String(primary_service.to_owned())))
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return Ok(None);
    };
    let Some(volumes) = service
        .get(serde_yaml::Value::String("volumes".to_owned()))
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return Ok(None);
    };
    let mut entries = volumes
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .filter_map(parse_mount_budget_entry)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.raw.len());
    let total_bytes = entries.iter().map(|entry| entry.raw.len()).sum::<usize>()
        + entries.len().saturating_sub(1);
    Ok(Some(PrimaryServiceMountEstimate {
        total_bytes,
        entries,
    }))
}

fn parse_mount_budget_entry(raw: &str) -> Option<MountBudgetEntry> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = trimmed.splitn(3, ':');
    let source = parts.next()?.trim();
    let target = parts.next()?.trim();
    if source.is_empty() || target.is_empty() {
        return None;
    }
    Some(MountBudgetEntry {
        target: target.to_owned(),
        raw: trimmed.to_owned(),
    })
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}
