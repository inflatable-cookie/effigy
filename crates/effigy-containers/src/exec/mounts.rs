//! Compare live named mounts with the final immutable Compose closure.
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
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
    let mut external = BTreeMap::new();
    for file in &policy.compose_files {
        let text = policy.read_compose(file).map_err(unavailable)?;
        let document: Value = serde_yaml::from_str(&text).map_err(unavailable)?;
        if let Some(volumes) = document.get("volumes").and_then(Value::as_mapping) {
            for (key, volume) in volumes {
                if let Some(key) = key.as_str() {
                    if let Some(value) = volume.get("external").and_then(Value::as_bool) {
                        external.insert(key.to_owned(), value);
                    }
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
                .unwrap_or_else(|| {
                    if external.get(&source) == Some(&true) {
                        source
                    } else {
                        format!("{}_{source}", policy.project_name)
                    }
                });
            Some(NamedMount {
                service,
                target,
                name,
                read_only,
            })
        })
        .collect())
}

/// One isolated mount: a named volume whose container target sits inside the
/// workspace checkout, so the checkout subtree is its host mount point.
#[derive(Debug, Clone, PartialEq, Eq)]
struct IsolatedMountPoint {
    host_path: PathBuf,
}

/// Named volumes nested under the primary service's workspace bind overlay a
/// host checkout subtree. Deleting that host directory (a host `target` or
/// `node_modules` cleanup) leaves a running container's nested mount without a
/// mount point, so `up` must restore the directory before create or reuse.
/// Only the primary service is mapped: its bind source is the checkout root.
fn isolated_mount_points(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
) -> Result<Vec<IsolatedMountPoint>, ContainerExecError> {
    let working_dir = working_dir
        .to_str()
        .ok_or_else(|| unavailable("workspace working directory is not UTF-8"))?
        .trim_end_matches('/');
    let mut points = named_mounts(policy)?
        .into_iter()
        .filter(|mount| mount.service == policy.primary_service)
        .filter_map(|mount| {
            let target = mount.target.trim_end_matches('/');
            let relative = target
                .strip_prefix(working_dir)
                .and_then(|rest| rest.strip_prefix('/'))
                .filter(|rest| !rest.is_empty())?;
            Some(IsolatedMountPoint {
                host_path: repo_root.join(relative),
            })
        })
        .collect::<Vec<_>>();
    points.sort_by(|left, right| left.host_path.cmp(&right.host_path));
    points.dedup();
    Ok(points)
}

/// Recreate missing host mount-point directories for isolated mounts before a
/// stack is created or reused. Returns the host paths this call created, so the
/// bring-up can treat a deleted mount point as mount drift and reconcile the
/// running generation. An existing non-directory or symlink refuses: this is
/// never permission to write through a link or replace a file.
pub fn ensure_isolated_mount_host_points(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, ContainerExecError> {
    let working_dir = crate::load_container_exec_working_dir(repo_root, Some(&policy.name))
        .map_err(unavailable)?;
    ensure_points(&isolated_mount_points(repo_root, policy, &working_dir)?)
}

fn ensure_points(points: &[IsolatedMountPoint]) -> Result<Vec<String>, ContainerExecError> {
    let mut created = Vec::new();
    for point in points {
        match host_point_presence(point)? {
            HostPoint::Directory => continue,
            HostPoint::Missing => {
                std::fs::create_dir_all(&point.host_path).map_err(|error| {
                    unavailable(format!(
                        "cannot recreate isolated mount point `{}`: {error}",
                        point.host_path.display()
                    ))
                })?;
                created.push(point.host_path.display().to_string());
            }
        }
    }
    Ok(created)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostPoint {
    Directory,
    Missing,
}

/// Read-only host observation for one isolated mount point. A non-directory or
/// symlink is unavailable, never drift and never recreation authority.
fn host_point_presence(point: &IsolatedMountPoint) -> Result<HostPoint, ContainerExecError> {
    match std::fs::symlink_metadata(&point.host_path) {
        Ok(metadata) if metadata.is_dir() => Ok(HostPoint::Directory),
        Ok(metadata) if metadata.file_type().is_symlink() => Err(unavailable(format!(
            "isolated mount point `{}` is a symlink; refusing to follow it",
            point.host_path.display()
        ))),
        Ok(_) => Err(unavailable(format!(
            "isolated mount point `{}` is not a directory",
            point.host_path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HostPoint::Missing),
        Err(error) => Err(unavailable(format!(
            "cannot inspect isolated mount point `{}`: {error}",
            point.host_path.display()
        ))),
    }
}

/// A host mount point that no longer exists is mount drift even while the
/// running container still reports the named volume: the directory beneath
/// that nested mount is gone. Observation only; the caller decides whether it
/// may create the directory and reconcile.
fn observe_host_mount_points(
    points: &[IsolatedMountPoint],
) -> Result<Vec<String>, ContainerExecError> {
    let mut drift = Vec::new();
    for point in points {
        if host_point_presence(point)? == HostPoint::Missing {
            drift.push(format!(
                "isolated mount point `{}` is missing on the host; the running stack's nested mount has no directory. Run `effigy container up --detach` to recreate it and reconcile before ownership repair",
                point.host_path.display()
            ));
        }
    }
    Ok(drift)
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
    let working_dir = crate::load_container_exec_working_dir(repo_root, Some(&policy.name))
        .map_err(unavailable)?;
    let host_point_drift =
        observe_host_mount_points(&isolated_mount_points(repo_root, policy, &working_dir)?)?;
    let mut drift = observe_mounts(&mounts, &rows, |row| {
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
        if let Err(error) = ownership.verify_inspected(repo_root, policy, row, &output.stdout) {
            // Use the same positive checkout-bind proof as reactivation. This
            // does not relax immutable identity or admit an unlabelled scope.
            if !ownership.admits_legacy_unlabelled(policy, &working_dir, row, &output.stdout) {
                return Err(error);
            }
        }
        serde_json::from_slice(&output.stdout).map_err(unavailable)
    })?;
    drift.extend(host_point_drift);
    Ok(drift)
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
            return Err(unavailable(format!(
                "service `{service}` has no running container whose mounts can be observed"
            )));
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
        assert!(observe_mounts(&expected(), &[], |_| panic!("no container to inspect")).is_err());
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
    fn isolated_dirs_external_volumes_use_bare_keys_or_explicit_names() {
        let fixture = tempfile::tempdir().unwrap();
        let mut policy = crate::policy::model::test_policy(fixture.path());
        let first = fixture.path().join("first.yml");
        let second = fixture.path().join("override.yml");
        std::fs::write(&first, "services:\n  workspace:\n    volumes: ['cache:/a', 'alias:/b', 'local:/c']\nvolumes:\n  cache: {external: true}\n  alias: {external: true, name: physical}\n  local: {}\n").unwrap();
        std::fs::write(
            &second,
            "volumes:\n  cache: {labels: {example: preserved}}\n",
        )
        .unwrap();
        policy.compose_files = vec![first, second];
        let required = named_mounts(&policy).unwrap();
        assert_eq!(required[0].name, "cache");
        assert_eq!(required[1].name, "physical");
        assert_eq!(required[2].name, format!("{}_local", policy.project_name));
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

    fn isolated_policy(fixture: &Path) -> EffectiveContainerPolicy {
        let mut policy = crate::policy::model::test_policy(fixture);
        let compose = fixture.join("compose.yml");
        std::fs::write(
            &compose,
            "services:\n  app:\n    volumes:\n      - node_modules:/workspace-root/repo/node_modules\n      - farmyard-target:/workspace-root/repo/apps/farmyard/target\n      - registry:/usr/local/cargo/registry\n  db:\n    volumes:\n      - pgdata:/workspace-root/repo/pgdata\nvolumes:\n  node_modules:\n  farmyard-target:\n  registry:\n  pgdata:\n",
        )
        .unwrap();
        policy.compose_files = vec![compose];
        policy
    }

    #[test]
    fn isolated_mount_points_map_only_primary_workspace_volumes_to_the_checkout() {
        let fixture = tempfile::tempdir().unwrap();
        let policy = isolated_policy(fixture.path());
        let points =
            isolated_mount_points(fixture.path(), &policy, Path::new("/workspace-root/repo"))
                .unwrap();
        assert_eq!(
            points,
            vec![
                IsolatedMountPoint {
                    host_path: fixture.path().join("apps/farmyard/target")
                },
                IsolatedMountPoint {
                    host_path: fixture.path().join("node_modules")
                },
            ]
        );
    }

    #[test]
    fn isolated_mount_points_are_recreated_and_a_file_or_link_refuses() {
        let fixture = tempfile::tempdir().unwrap();
        let target = fixture.path().join("apps/farmyard/target");
        let present = fixture.path().join("node_modules");
        std::fs::create_dir_all(&present).unwrap();
        let points = vec![
            IsolatedMountPoint {
                host_path: target.clone(),
            },
            IsolatedMountPoint {
                host_path: present.clone(),
            },
        ];
        assert_eq!(
            ensure_points(&points).unwrap(),
            vec![target.display().to_string()]
        );
        assert!(target.is_dir());
        assert!(ensure_points(&points).unwrap().is_empty(), "idempotent");

        let file = fixture.path().join("apps/farmyard/node_modules");
        std::fs::write(&file, "not a directory").unwrap();
        let error = ensure_points(&[IsolatedMountPoint {
            host_path: file.clone(),
        }])
        .expect_err("a file mount point refuses");
        assert!(error.to_string().contains("is not a directory"), "{error}");

        #[cfg(unix)]
        {
            let link = fixture.path().join("linked");
            std::os::unix::fs::symlink(fixture.path(), &link).unwrap();
            let error = ensure_points(&[IsolatedMountPoint { host_path: link }])
                .expect_err("a symlink mount point refuses");
            assert!(error.to_string().contains("symlink"), "{error}");
        }
    }

    #[test]
    fn a_missing_host_mount_point_is_observed_as_drift_without_creating_it() {
        let fixture = tempfile::tempdir().unwrap();
        let policy = isolated_policy(fixture.path());
        let working_dir = Path::new("/workspace-root/repo");
        let points = isolated_mount_points(fixture.path(), &policy, working_dir).unwrap();
        let drift = observe_host_mount_points(&points).unwrap();
        assert_eq!(drift.len(), 2, "{drift:?}");
        assert!(drift
            .iter()
            .all(|detail| detail.contains("is missing on the host")));
        assert!(
            !fixture.path().join("node_modules").exists(),
            "observation only"
        );
        assert!(!fixture.path().join("apps/farmyard/target").exists());

        std::fs::create_dir_all(fixture.path().join("node_modules")).unwrap();
        let points = isolated_mount_points(fixture.path(), &policy, working_dir).unwrap();
        let drift = observe_host_mount_points(&points).unwrap();
        assert_eq!(drift.len(), 1, "{drift:?}");

        ensure_points(&points).unwrap();
        let points = isolated_mount_points(fixture.path(), &policy, working_dir).unwrap();
        assert!(observe_host_mount_points(&points).unwrap().is_empty());
    }
}
