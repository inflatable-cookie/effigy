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
        if policy.read_compose(compose_file).is_err() {
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
    let Some(estimate) = estimate_primary_service_mount_label_size(policy)? else {
        return Ok(());
    };
    if nerdctl_mounts_label_fits(estimate.total_bytes) {
        return Ok(());
    }
    let heaviest = estimate
        .entries
        .iter()
        .rev()
        .take(6)
        .map(|entry| format!("effective mount #{} ({} bytes)", entry.ordinal, entry.bytes))
        .collect::<Vec<_>>()
        .join(", ");
    Err(ContainerPolicyError::TaskInvocation(format!(
        "container `{}` uses the Colima nerdctl compose fallback, but primary service `{}` has an estimated nerdctl/mounts label size of {} bytes across {} effective mounts, exceeding the conservative {}-byte preflight budget (containerd label limit: 4096 bytes); reduce optional mounts or workspace isolation mounts. Largest contributors (paths omitted): {}",
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
    ordinal: usize,
    bytes: usize,
}

fn estimate_primary_service_mount_label_size(
    policy: &EffectiveContainerPolicy,
) -> Result<Option<PrimaryServiceMountEstimate>, ContainerPolicyError> {
    // Compose applies -f overlays in order and merges service volumes by
    // container target. Match that identity here so a replaced earlier mount
    // is not counted as if nerdctl received both entries.
    let mut effective = std::collections::BTreeMap::<String, EffectiveMount>::new();
    for compose_file in &policy.compose_files {
        let content = policy.read_compose(compose_file)?;
        let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to parse compose file {} while estimating nerdctl mount budget: {error}",
                compose_file.display()
            ))
        })?;
        let Some(volumes) = parsed
            .get("services")
            .and_then(serde_yaml::Value::as_mapping)
            .and_then(|services| {
                services.get(serde_yaml::Value::String(policy.primary_service.clone()))
            })
            .and_then(serde_yaml::Value::as_mapping)
            .and_then(|service| service.get(serde_yaml::Value::String("volumes".to_owned())))
            .and_then(serde_yaml::Value::as_sequence)
        else {
            continue;
        };
        for value in volumes {
            if let Some(mount) = parse_effective_mount(
                value,
                compose_file.parent().unwrap_or(Path::new(".")),
                &policy.project_name,
            ) {
                effective.insert(mount.target.clone(), mount);
            }
        }
    }
    if effective.is_empty() {
        return Ok(None);
    }
    let mut entries = effective
        .into_values()
        .enumerate()
        .map(|(index, mount)| MountBudgetEntry {
            ordinal: index + 1,
            bytes: nerdctl_mount_json_bytes(&mount),
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.bytes);
    // nerdctl v2.3.1 stores compact JSON for []mount.Mount under
    // `nerdctl/mounts`; include the key and the JSON array punctuation.
    let total_bytes = nerdctl_mounts_label_size(&entries);
    Ok(Some(PrimaryServiceMountEstimate {
        total_bytes,
        entries,
    }))
}

#[derive(Debug)]
struct EffectiveMount {
    source: String,
    target: String,
    kind: String,
    read_only: bool,
}

fn parse_effective_mount(
    value: &serde_yaml::Value,
    compose_dir: &Path,
    project_name: &str,
) -> Option<EffectiveMount> {
    let (source, target, kind, read_only) = if let Some(raw) = value.as_str() {
        let mut parts = raw.splitn(3, ':');
        let first = parts.next()?.trim();
        let second = parts.next().map(str::trim);
        let mode = parts.next().unwrap_or("");
        let (source, target) = match second {
            Some(target) => (first, target),
            None => ("", first),
        };
        let kind =
            if source.starts_with('/') || source.starts_with("./") || source.starts_with("../") {
                "bind"
            } else {
                "volume"
            };
        (
            source,
            target,
            kind,
            mode.split(',').any(|part| part == "ro"),
        )
    } else {
        let mapping = value.as_mapping()?;
        let get = |name: &str| mapping.get(serde_yaml::Value::String(name.to_owned()));
        let source = get("source")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("");
        let target = get("target")?.as_str()?;
        let kind = get("type")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("volume");
        let read_only = get("read_only")
            .and_then(serde_yaml::Value::as_bool)
            .unwrap_or(false);
        (source, target, kind, read_only)
    };
    if target.is_empty() {
        return None;
    }
    // Compose/nerdctl materialize named and anonymous volumes as host binds
    // under containerd's volume store. Reserve the maximum documented source
    // identity width here; exact VM-local paths are unavailable pre-runtime.
    let (resolved, resolved_kind) = if kind == "bind" {
        let path = if source.is_empty() {
            String::new()
        } else if Path::new(source).is_absolute() {
            source.to_owned()
        } else {
            compose_dir.join(source).to_string_lossy().into_owned()
        };
        (path, "bind")
    } else {
        let volume_name = if source.is_empty() {
            "anonymous".to_owned()
        } else {
            format!("{project_name}-{source}")
        };
        (
            format!(
                "/var/lib/nerdctl/{}/volumes/default/{volume_name}/_data",
                "0".repeat(64)
            ),
            "bind",
        )
    };
    Some(EffectiveMount {
        source: resolved,
        target: target.to_owned(),
        kind: resolved_kind.to_owned(),
        read_only,
    })
}

fn nerdctl_mount_json_bytes(mount: &EffectiveMount) -> usize {
    // containerd mount.Mount's JSON shape as used by nerdctl v2.3.1. Go's
    // encoder also escapes HTML-sensitive characters; account for that so
    // this Rust-side estimate cannot undercount those paths.
    let options = if mount.read_only {
        "[\"rbind\",\"rprivate\",\"ro\"]"
    } else {
        "[\"rbind\",\"rprivate\",\"rw\"]"
    };
    let encoded = format!(
        "{{\"Type\":{},\"Source\":{},\"Target\":{},\"Options\":{options}}}",
        go_json_string(&mount.kind),
        go_json_string(&mount.source),
        go_json_string(&mount.target),
    );
    encoded.len()
}

fn nerdctl_mounts_label_size(entries: &[MountBudgetEntry]) -> usize {
    "nerdctl/mounts".len()
        + 2
        + entries.iter().map(|entry| entry.bytes).sum::<usize>()
        + entries.len().saturating_sub(1)
}

fn nerdctl_mounts_label_fits(size: usize) -> bool {
    size < NERDCTL_MOUNTS_LABEL_BUDGET_BYTES
}

fn go_json_string(value: &str) -> String {
    let json = serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned());
    json.replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_json_estimate_counts_nerdctl_key_and_json_escaping() {
        let mount = EffectiveMount {
            source: "/workspace/secret<&>\u{2028}".to_owned(),
            target: "/workspace".to_owned(),
            kind: "bind".to_owned(),
            read_only: true,
        };
        let bytes = nerdctl_mount_json_bytes(&mount);
        let expected = format!(
            "{{\"Type\":\"bind\",\"Source\":{},\"Target\":\"/workspace\",\"Options\":[\"rbind\",\"rprivate\",\"ro\"]}}",
            go_json_string(&mount.source)
        );
        assert_eq!(bytes, expected.len());
        assert!(go_json_string(&mount.source).contains("\\u003c"));
        assert!(go_json_string(&mount.source).contains("\\u2028"));
        assert_eq!(nerdctl_mounts_label_size(&[]), "nerdctl/mounts".len() + 2);
    }

    #[test]
    fn short_long_and_anonymous_mounts_resolve_into_backend_shapes() {
        let dir = Path::new("/long/queue/path/project/compose");
        let short: serde_yaml::Value = serde_yaml::from_str("'./src:/workspace:ro'").unwrap();
        let named: serde_yaml::Value = serde_yaml::from_str(
            "{type: volume, source: cache, target: /root/.cache, read_only: true}",
        )
        .unwrap();
        let anonymous: serde_yaml::Value = serde_yaml::from_str("/tmp").unwrap();

        let short = parse_effective_mount(&short, dir, "example").unwrap();
        assert_eq!(short.source, "/long/queue/path/project/compose/./src");
        assert!(short.read_only);
        let named = parse_effective_mount(&named, dir, "example").unwrap();
        assert!(named.source.contains("/var/lib/nerdctl/"));
        assert!(named.source.ends_with("/example-cache/_data"));
        assert!(named.read_only);
        let anonymous = parse_effective_mount(&anonymous, dir, "example").unwrap();
        assert_eq!(anonymous.target, "/tmp");
        assert!(anonymous.source.contains("/anonymous/_data"));
    }

    #[test]
    fn threshold_refuses_at_budget_and_allows_one_byte_below() {
        let entry = MountBudgetEntry {
            ordinal: 1,
            bytes: NERDCTL_MOUNTS_LABEL_BUDGET_BYTES - "nerdctl/mounts".len() - 2,
        };
        assert_eq!(
            nerdctl_mounts_label_size(std::slice::from_ref(&entry)),
            3900
        );
        assert!(!nerdctl_mounts_label_fits(nerdctl_mounts_label_size(
            std::slice::from_ref(&entry)
        )));
        let below = MountBudgetEntry {
            bytes: entry.bytes - 1,
            ..entry
        };
        assert!(nerdctl_mounts_label_fits(nerdctl_mounts_label_size(
            std::slice::from_ref(&below)
        )));
    }
}
