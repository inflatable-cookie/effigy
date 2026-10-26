//! Read named mounts from the final immutable Compose closure and create the
//! host directories an isolated mount needs.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_yaml::Value;

use super::ContainerExecError;
use crate::EffectiveContainerPolicy;

#[derive(Debug)]
struct NamedMount {
    service: String,
    target: String,
}

fn unavailable(reason: impl std::fmt::Display) -> ContainerExecError {
    ContainerExecError::Failure {
        command: "read rendered mounts".into(),
        code: None,
        stdout: String::new(),
        stderr: format!("mount preparation unavailable: {reason}"),
    }
}

// Later Compose files replace mounts by destination; only a named volume whose
// target sits in the checkout needs a host mount point.
fn named_mounts(policy: &EffectiveContainerPolicy) -> Result<Vec<NamedMount>, ContainerExecError> {
    let mut mounts = BTreeMap::new();
    for file in &policy.compose_files {
        let text = policy.read_compose(file).map_err(unavailable)?;
        let document: Value = serde_yaml::from_str(&text).map_err(unavailable)?;
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
                let (source, target, named) = if let Some(raw) = volume.as_str() {
                    let parts = raw.splitn(3, ':').collect::<Vec<_>>();
                    let [source, target, ..] = parts.as_slice() else {
                        mounts.remove(&(service.to_owned(), raw.to_owned()));
                        continue;
                    };
                    (*source, *target, !source.starts_with(['/', '.', '~', '$']))
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
                    )
                };
                mounts.insert(
                    (service.to_owned(), target.to_owned()),
                    (source.to_owned(), named),
                );
            }
        }
    }
    Ok(mounts
        .into_iter()
        .filter_map(|((service, target), (source, named))| {
            (named && !source.is_empty()).then_some(NamedMount { service, target })
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
/// stack is created. Returns the host paths this call created. An existing
/// non-directory or symlink refuses: this is never permission to write through
/// a link or replace a file.
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
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = std::fs::read_link(&point.host_path)
                .map(|target| target.display().to_string())
                .unwrap_or_else(|error| format!("<unreadable: {error}>"));
            Err(unavailable(format!(
                "isolated mount point `{}` is a symlink to `{target}`; refusing to follow it. A host output link sits on a container-isolated path: adopt the project's output links with `oxide outputs adopt` so the path stays a real directory, or drop it from `isolated_dirs`.",
                point.host_path.display()
            )))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
            let error = error.to_string();
            assert!(error.contains("symlink"), "{error}");
            assert!(
                error.contains(&format!("is a symlink to `{}`", fixture.path().display())),
                "the refusal names the link target: {error}"
            );
            assert!(error.contains("container-isolated path"), "{error}");
            assert!(error.contains("oxide outputs adopt"), "{error}");
            assert!(error.contains("drop it from `isolated_dirs`"), "{error}");
        }
    }
}
