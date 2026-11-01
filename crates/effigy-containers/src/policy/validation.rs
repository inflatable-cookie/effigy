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
    if nerdctl_mounts_label_fits(estimate.guarded_bytes) {
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
        "container `{}` uses the Colima nerdctl compose fallback, but primary service `{}` has an estimated nerdctl/mounts label size of {} bytes plus a 2% reserve ({} bytes) = {} bytes across {} effective mounts, exceeding the conservative {}-byte preflight budget (containerd label limit: 4096 bytes); reduce optional mounts or workspace isolation mounts. Largest contributors (paths omitted): {}",
        policy.name,
        policy.primary_service,
        estimate.total_bytes,
        estimate.safety_margin_bytes,
        estimate.guarded_bytes,
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
    safety_margin_bytes: usize,
    guarded_bytes: usize,
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
    // is not counted as if nerdctl received both entries. Every explicit
    // mount input must be modelled; silently skipping one could undercount.
    let mut effective = std::collections::BTreeMap::<String, EffectiveMount>::new();
    let mut unmodelled_named_volumes = std::collections::BTreeMap::<String, usize>::new();
    for (file_index, compose_file) in policy.compose_files.iter().enumerate() {
        let content = policy.read_compose(compose_file)?;
        let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|_| {
            mount_budget_refusal(policy, file_index, "Compose input is not valid YAML")
        })?;
        let Some(document) = parsed.as_mapping() else {
            return Err(mount_budget_refusal(
                policy,
                file_index,
                "Compose document is not a mapping",
            ));
        };

        // Named-volume options can change the backing source path. Recognize
        // defaults and Effigy's generated aliases; defer unsupported
        // definitions until we know if the primary service mounts them.
        if let Some(volume_definitions) =
            document.get(serde_yaml::Value::String("volumes".to_owned()))
        {
            let Some(volume_definitions) = volume_definitions.as_mapping() else {
                return Err(mount_budget_refusal(
                    policy,
                    file_index,
                    "top-level `volumes` is not a mapping",
                ));
            };
            for (name, definition) in volume_definitions {
                let Some(name) = name.as_str() else {
                    return Err(mount_budget_refusal(
                        policy,
                        file_index,
                        "a top-level volume definition has an unmodelled key",
                    ));
                };
                if !is_modelled_named_volume_definition(name, definition) {
                    unmodelled_named_volumes
                        .entry(name.to_owned())
                        .or_insert(file_index);
                }
            }
        }

        let Some(services_value) = document.get(serde_yaml::Value::String("services".to_owned()))
        else {
            continue;
        };
        let Some(services) = services_value.as_mapping() else {
            return Err(mount_budget_refusal(
                policy,
                file_index,
                "the services table is not a mapping",
            ));
        };
        let Some(service_value) =
            services.get(serde_yaml::Value::String(policy.primary_service.clone()))
        else {
            continue;
        };
        let Some(service) = service_value.as_mapping() else {
            return Err(mount_budget_refusal(
                policy,
                file_index,
                "the primary service definition is not a mapping",
            ));
        };

        // These Compose fields can add mounts outside service.volumes. Their
        // nerdctl serialization is not modelled here, so refuse the Colima
        // fallback rather than counting only the explicit volume list.
        for field in ["extends", "tmpfs", "configs", "secrets", "volumes_from"] {
            if service.contains_key(serde_yaml::Value::String(field.to_owned())) {
                return Err(mount_budget_refusal(
                    policy,
                    file_index,
                    &format!("primary service has unmodelled mount input `{field}`"),
                ));
            }
        }

        let Some(volumes_value) = service.get(serde_yaml::Value::String("volumes".to_owned()))
        else {
            continue;
        };
        let Some(volumes) = volumes_value.as_sequence() else {
            return Err(mount_budget_refusal(
                policy,
                file_index,
                "primary service `volumes` is not a sequence",
            ));
        };
        for (mount_index, value) in volumes.iter().enumerate() {
            let mount = parse_effective_mount(
                value,
                compose_file.parent().unwrap_or(Path::new(".")),
                &policy.project_name,
            )
            .map_err(|reason| {
                mount_budget_refusal(
                    policy,
                    file_index,
                    &format!(
                        "primary service `volumes[{}]` has {reason}",
                        mount_index + 1
                    ),
                )
            })?;
            effective.insert(mount.target.clone(), mount);
        }
    }
    for mount in effective.values() {
        if let Some(name) = &mount.named_volume_source {
            if let Some(file_index) = unmodelled_named_volumes.get(name) {
                return Err(mount_budget_refusal(
                    policy,
                    *file_index,
                    "primary service mounts a named volume with unmodelled top-level options",
                ));
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
    let safety_margin_bytes = nerdctl_mounts_safety_margin(total_bytes);
    Ok(Some(PrimaryServiceMountEstimate {
        total_bytes,
        safety_margin_bytes,
        guarded_bytes: total_bytes.saturating_add(safety_margin_bytes),
        entries,
    }))
}

fn is_modelled_named_volume_definition(name: &str, definition: &serde_yaml::Value) -> bool {
    if definition.is_null() {
        return true;
    }
    let Some(options) = definition.as_mapping() else {
        return false;
    };
    options.iter().all(|(key, value)| match key.as_str() {
        Some("driver") => value.as_str() == Some("local"),
        Some("name") => value.as_str() == Some(name),
        Some("labels") => value.as_mapping().is_some(),
        _ => false,
    })
}

fn mount_budget_refusal(
    policy: &EffectiveContainerPolicy,
    file_index: usize,
    detail: &str,
) -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation(format!(
        "container `{}` uses the Colima nerdctl compose fallback, but primary service `{}` has an unmodelled mount input in Compose input #{}: {detail}; use only short-form bind/volume mounts or long-form `type: bind|volume` with `source`, `target`, and `read_only`, or remove the unsupported input. Mount paths and values are omitted",
        policy.name,
        policy.primary_service,
        file_index + 1,
    ))
}

#[derive(Debug)]
struct EffectiveMount {
    source: String,
    target: String,
    kind: String,
    read_only: bool,
    named_volume_source: Option<String>,
}

fn parse_effective_mount(
    value: &serde_yaml::Value,
    compose_dir: &Path,
    project_name: &str,
) -> Result<EffectiveMount, &'static str> {
    let (source, target, kind, read_only) = if let Some(raw) = value.as_str() {
        let parts = raw.split(':').collect::<Vec<_>>();
        if parts.len() > 3 {
            return Err("short-form syntax contains an ambiguous colon");
        }
        if parts.len() == 1 {
            ("", parts[0].trim(), "volume", false)
        } else {
            let source = parts[0].trim();
            let target = parts[1].trim();
            let mode = parts.get(2).copied().unwrap_or("").trim();
            let mode_parts = if mode.is_empty() {
                Vec::new()
            } else {
                mode.split(',').collect::<Vec<_>>()
            };
            if mode_parts.len() > 1 || mode_parts.iter().any(|part| !matches!(*part, "ro" | "rw")) {
                return Err("short-form mount mode has unmodelled options");
            }
            let kind = if source.is_empty() {
                "volume"
            } else if source.starts_with('/')
                || source.starts_with("./")
                || source.starts_with("../")
            {
                "bind"
            } else {
                "volume"
            };
            (source, target, kind, mode_parts.contains(&"ro"))
        }
    } else if let Some(mapping) = value.as_mapping() {
        for key in mapping.keys() {
            let Some(name) = key.as_str() else {
                return Err("long-form mount has an unmodelled field");
            };
            if !matches!(name, "type" | "source" | "target" | "read_only") {
                return Err("long-form mount has unmodelled fields or options");
            }
        }
        let get = |name: &str| mapping.get(serde_yaml::Value::String(name.to_owned()));
        let kind = match get("type") {
            None => "volume",
            Some(value) => value
                .as_str()
                .ok_or("long-form mount type is not a string")?,
        };
        if !matches!(kind, "bind" | "volume") {
            return Err("long-form mount type is not a supported bind or volume");
        }
        let source = match get("source") {
            None => "",
            Some(value) => value
                .as_str()
                .ok_or("long-form mount source is not a string")?,
        };
        let target = get("target")
            .and_then(serde_yaml::Value::as_str)
            .ok_or("long-form mount has no string target")?;
        let read_only = match get("read_only") {
            None => false,
            Some(value) => value
                .as_bool()
                .ok_or("long-form mount `read_only` is not a boolean")?,
        };
        (source, target, kind, read_only)
    } else {
        return Err("entry is neither short-form text nor a long-form mapping");
    };
    if !target.starts_with('/') {
        return Err("mount target is not an absolute guest path");
    }
    if source.contains('$') || target.contains('$') {
        return Err("mount path contains unresolved Compose interpolation");
    }
    if source.starts_with("~/") {
        return Err("tilde-prefixed bind source is not modelled");
    }
    if kind == "bind" && source.is_empty() {
        return Err("bind mount source is empty");
    }
    let named_volume_source = (kind == "volume" && !source.is_empty()).then(|| source.to_owned());
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
    Ok(EffectiveMount {
        source: resolved,
        target: target.to_owned(),
        kind: resolved_kind.to_owned(),
        read_only,
        named_volume_source,
    })
}

fn nerdctl_mount_json_bytes(mount: &EffectiveMount) -> usize {
    // nerdctl v2.3.1's `nerdctl/mounts` metadata serializes Type, Source,
    // Destination, Mode, RW and Propagation. Go's encoder also escapes
    // HTML-sensitive characters; account for that on both paths.
    let mode = if mount.read_only { "ro" } else { "" };
    let rw = !mount.read_only;
    let encoded = format!(
        "{{\"Type\":{},\"Source\":{},\"Destination\":{},\"Mode\":{},\"RW\":{rw},\"Propagation\":\"\"}}",
        go_json_string(&mount.kind),
        go_json_string(&mount.source),
        go_json_string(&mount.target),
        go_json_string(mode),
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

fn nerdctl_mounts_safety_margin(size: usize) -> usize {
    size.div_ceil(50)
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
            named_volume_source: None,
        };
        let bytes = nerdctl_mount_json_bytes(&mount);
        let expected = format!(
            "{{\"Type\":\"bind\",\"Source\":{},\"Destination\":\"/workspace\",\"Mode\":\"ro\",\"RW\":false,\"Propagation\":\"\"}}",
            go_json_string(&mount.source),
        );
        assert_eq!(bytes, expected.len());
        assert!(go_json_string(&mount.source).contains("\\u003c"));
        assert!(go_json_string(&mount.source).contains("\\u2028"));
        assert_eq!(nerdctl_mounts_label_size(&[]), "nerdctl/mounts".len() + 2);
    }

    #[test]
    fn observed_nerdctl_shape_and_two_percent_reserve_match_r13_fixture() {
        let source = format!("/{}", "s".repeat(61));
        let target = format!("/{}", "d".repeat(27));
        let writable = EffectiveMount {
            source: source.clone(),
            target: target.clone(),
            kind: "bind".to_owned(),
            read_only: false,
            named_volume_source: None,
        };
        let read_only = EffectiveMount {
            source,
            target,
            kind: "bind".to_owned(),
            read_only: true,
            named_volume_source: None,
        };
        assert_eq!(nerdctl_mount_json_bytes(&writable), 171);
        assert_eq!(nerdctl_mount_json_bytes(&read_only), 174);

        let entries = (0..20)
            .map(|index| MountBudgetEntry {
                ordinal: index + 1,
                bytes: nerdctl_mount_json_bytes(&writable),
            })
            .chain(std::iter::once(MountBudgetEntry {
                ordinal: 21,
                bytes: nerdctl_mount_json_bytes(&read_only),
            }))
            .collect::<Vec<_>>();
        let observed_fixture_bytes = nerdctl_mounts_label_size(&entries);
        assert_eq!(observed_fixture_bytes, 3630);
        let margin = nerdctl_mounts_safety_margin(observed_fixture_bytes);
        assert_eq!(margin, 73);
        assert_eq!(observed_fixture_bytes + margin, 3703);
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
