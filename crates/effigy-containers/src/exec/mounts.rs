//! Compare live named mounts with the final immutable Compose closure.
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::Instant;

use serde_yaml::Value;

use super::colima_runtime::run_runtime_command_capture_for_policy_with_absolute_deadline;
use super::{ContainerExecError, OwnershipContext, RunningComposeContainer};
use crate::EffectiveContainerPolicy;

#[derive(Debug)]
struct NamedMount {
    service: String,
    target: String,
    name: String,
    read_only: bool,
}

fn unavailable(reason: impl std::fmt::Display) -> ContainerExecError {
    ContainerExecError::Failure {
        command: "inspect rendered mounts".into(),
        code: None,
        stdout: String::new(),
        stderr: format!("mount observation unavailable: {reason}"),
    }
}

// Later Compose files replace mounts by destination. Use the final aliases'
// explicit names, including workspace compaction and data_from borrowing.
fn named_mounts(policy: &EffectiveContainerPolicy) -> Result<Vec<NamedMount>, ContainerExecError> {
    let mut mounts = BTreeMap::new();
    let mut names = BTreeMap::new();
    for file in &policy.compose_files {
        let text = policy.read_compose(file).map_err(unavailable)?;
        let document: Value = serde_yaml::from_str(&text).map_err(unavailable)?;
        if let Some(volumes) = document.get("volumes").and_then(Value::as_mapping) {
            for (key, volume) in volumes {
                if let Some(key) = key.as_str() {
                    let name = volume
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    // An override adding labels/driver options does not erase
                    // an earlier explicit physical name.
                    if name.is_some() || !names.contains_key(key) {
                        names.insert(key.to_owned(), name);
                    }
                }
            }
        }
        let Some(services) = document.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (service, value) in services {
            let Some(service) = service.as_str() else {
                continue;
            };
            let Some(volumes) = value.get("volumes").and_then(Value::as_sequence) else {
                continue;
            };
            for volume in volumes {
                let (source, target, named, read_only) = if let Some(raw) = volume.as_str() {
                    let parts = raw.splitn(3, ':').collect::<Vec<_>>();
                    let [source, target, ..] = parts.as_slice() else {
                        mounts.remove(&(service.to_owned(), raw.to_owned()));
                        continue;
                    };
                    (
                        *source,
                        *target,
                        !source.starts_with(['/', '.', '~', '$']),
                        parts
                            .get(2)
                            .is_some_and(|options| options.split(',').any(|option| option == "ro")),
                    )
                } else {
                    let Some(target) = volume.get("target").and_then(Value::as_str) else {
                        continue;
                    };
                    (
                        volume
                            .get("source")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        target,
                        volume.get("type").and_then(Value::as_str) == Some("volume"),
                        volume
                            .get("read_only")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    )
                };
                mounts.insert(
                    (service.to_owned(), target.to_owned()),
                    (source.to_owned(), named, read_only),
                );
            }
        }
    }
    Ok(mounts
        .into_iter()
        .filter_map(|((service, target), (source, named, read_only))| {
            if !named || source.is_empty() {
                return None;
            }
            let name = names
                .get(&source)
                .and_then(Clone::clone)
                .unwrap_or_else(|| format!("{}_{source}", policy.project_name));
            Some(NamedMount {
                service,
                target,
                name,
                read_only,
            })
        })
        .collect())
}

/// Observe only. A failed inventory, ownership check or inspect is an error,
/// never a clean mount set or permission to recreate anything.
pub fn probe_mount_drift(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Instant,
) -> Result<Vec<String>, ContainerExecError> {
    let mounts = named_mounts(policy)?;
    if mounts.is_empty() {
        return Ok(Vec::new());
    }
    let rows = super::list_running_compose_containers_for_policy_with_deadline(
        repo_root,
        policy,
        Some(deadline),
    )?;
    let ownership = OwnershipContext::resolve(repo_root, policy)?;
    observe_mounts(&mounts, &rows, |row| {
        let remaining =
            super::process::remaining_until_deadline(deadline, "inspect rendered mounts")?;
        let output = run_runtime_command_capture_for_policy_with_absolute_deadline(
            repo_root,
            policy,
            &[
                OsString::from("inspect"),
                OsString::from(&row.container_name),
            ],
            "inspect rendered mounts",
            deadline,
            remaining,
        )?;
        ownership.verify_inspected(repo_root, policy, row, &output.stdout)?;
        serde_json::from_slice(&output.stdout).map_err(unavailable)
    })
}

fn observe_mounts(
    expected: &[NamedMount],
    rows: &[RunningComposeContainer],
    mut inspect: impl FnMut(&RunningComposeContainer) -> Result<serde_json::Value, ContainerExecError>,
) -> Result<Vec<String>, ContainerExecError> {
    let mut drift = Vec::new();
    for row in rows
        .iter()
        .filter(|row| !row.oneoff && super::compose_status_is_running(&row.status))
    {
        let required = expected
            .iter()
            .filter(|mount| row.service.as_deref() == Some(&mount.service))
            .collect::<Vec<_>>();
        if required.is_empty() {
            continue;
        }
        let document = inspect(row)?;
        let mounts = document
            .as_array()
            .filter(|records| records.len() == 1)
            .and_then(|records| records.first())
            .and_then(|record| record.get("Mounts"))
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| unavailable("inspect has no mount inventory"))?;
        for mount in mounts {
            if mount
                .get("Destination")
                .and_then(serde_json::Value::as_str)
                .is_none()
                || mount
                    .get("Type")
                    .and_then(serde_json::Value::as_str)
                    .is_none()
                || mount
                    .get("RW")
                    .and_then(serde_json::Value::as_bool)
                    .is_none()
                || (mount.get("Type").and_then(serde_json::Value::as_str) == Some("volume")
                    && mount
                        .get("Name")
                        .and_then(serde_json::Value::as_str)
                        .is_none())
            {
                return Err(unavailable("inspect has incomplete mount fields"));
            }
        }
        for required in required {
            let matching = mounts
                .iter()
                .filter(|mount| {
                    mount.get("Destination").and_then(serde_json::Value::as_str)
                        == Some(&required.target)
                })
                .collect::<Vec<_>>();
            let matches = match matching.as_slice() {
                [mount] => {
                    mount.get("Type").and_then(serde_json::Value::as_str) == Some("volume")
                        && mount.get("Name").and_then(serde_json::Value::as_str)
                            == Some(&required.name)
                        && mount.get("RW").and_then(serde_json::Value::as_bool)
                            == Some(!required.read_only)
                }
                [] => false,
                _ => return Err(unavailable("inspect has duplicate mount destinations")),
            };
            if !matches {
                drift.push(format!("service `{}` requires named volume `{}` at `{}`; live mounts differ. Run `effigy container up --detach` to reconcile before ownership repair", required.service, required.name, required.target));
            }
        }
    }
    for service in expected
        .iter()
        .map(|mount| mount.service.as_str())
        .collect::<std::collections::BTreeSet<_>>()
    {
        if !rows.iter().any(|row| {
            !row.oneoff
                && super::compose_status_is_running(&row.status)
                && row.service.as_deref() == Some(service)
        }) {
            drift.push(format!("service `{service}` has no running container for its rendered named mounts. Run `effigy container up --detach` to reconcile"));
        }
    }
    Ok(drift)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> RunningComposeContainer {
        RunningComposeContainer {
            container_name: "workspace-id".into(),
            status: "Up".into(),
            ports: vec![],
            project_name: Some("acowtancy-dev-wt-fixture".into()),
            working_dir: None,
            service: Some("workspace".into()),
            oneoff: false,
        }
    }

    fn expected() -> Vec<NamedMount> {
        [
            "node_modules",
            "packages/froyo/node_modules",
            "apps/farmyard/target",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, dir)| NamedMount {
            service: "workspace".into(),
            target: format!("/workspace-root/acowtancy/{dir}"),
            name: format!("effigy-owned-{index}"),
            read_only: false,
        })
        .collect()
    }

    #[test]
    fn isolated_dirs_drift_detects_stale_bind_then_accepts_reconciled_mounts() {
        let required = expected();
        let stale = serde_json::json!([{"Mounts": [{"Type":"bind", "Source":"/checkout", "Destination":"/workspace-root/acowtancy", "RW":true}]}]);
        assert_eq!(
            observe_mounts(&required, &[row()], |_| Ok(stale.clone()))
                .unwrap()
                .len(),
            3
        );
        let repaired = serde_json::json!([{"Mounts":required.iter().map(|mount| serde_json::json!({"Type":"volume", "Name":mount.name, "Destination":mount.target, "RW":true})).collect::<Vec<_>>()}]);
        assert!(
            observe_mounts(&required, &[row()], |_| Ok(repaired.clone()))
                .unwrap()
                .is_empty()
        );
        for replacement in [
            serde_json::json!({"Type":"bind", "Name":"effigy-owned-0", "Destination":required[0].target, "RW":true}),
            serde_json::json!({"Type":"volume", "Name":"foreign", "Destination":required[0].target, "RW":true}),
            serde_json::json!({"Type":"volume", "Name":"effigy-owned-0", "Destination":required[0].target, "RW":false}),
        ] {
            let mut changed = repaired.clone();
            changed[0]["Mounts"][0] = replacement;
            assert_eq!(
                observe_mounts(&required, &[row()], |_| Ok(changed.clone()))
                    .unwrap()
                    .len(),
                1
            );
        }
    }

    #[test]
    fn isolated_dirs_unavailable_inventory_never_becomes_clean_or_drift() {
        for invalid in [
            serde_json::json!([]),
            serde_json::json!([{}]),
            serde_json::json!([{"Mounts":null}]),
            serde_json::json!([{"Mounts":[{"Destination":"/workspace-root/acowtancy/node_modules", "Type":"volume", "Name":"effigy-owned-0"}]}]),
        ] {
            assert!(observe_mounts(&expected(), &[row()], |_| Ok(invalid.clone())).is_err());
        }
        assert!(observe_mounts(&expected(), &[row()], |_| Err(unavailable("deadline"))).is_err());
    }

    #[test]
    fn isolated_dirs_expectations_use_final_compose_names_and_overrides() {
        let fixture = tempfile::tempdir().unwrap();
        let mut policy = crate::policy::model::test_policy(fixture.path());
        let first = fixture.path().join("first.yml");
        let second = fixture.path().join("override.yml");
        std::fs::write(&first, "services:\n  workspace:\n    volumes: ['alias:/node_modules', 'cache:/target', 'old:/replaced']\nvolumes:\n  alias: {name: efv-compacted}\n  cache: {name: owner-data-from}\n").unwrap();
        std::fs::write(&second, "services:\n  workspace:\n    volumes: [{type: bind, source: /host, target: /replaced}]\nvolumes:\n  alias: {labels: {example: preserved}}\n").unwrap();
        policy.compose_files = vec![first, second];
        let required = named_mounts(&policy).unwrap();
        assert_eq!(required.len(), 2);
        assert_eq!(required[0].name, "efv-compacted");
        assert_eq!(required[1].name, "owner-data-from");
    }
}
