//! Read named mounts from the final immutable Compose closure and create the
//! host directories an isolated mount needs.
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use effigy_kernel::fs::{DirFd, FsError};
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
/// stack is created. Returns the host paths this call created.
///
/// Preparation is component-relative and no-follow: every component below the
/// checkout is opened from the directory already held above it, so a symlinked
/// ancestor or leaf refuses exactly like a symlink at the mount point itself.
/// An existing non-directory refuses too. Creation never replaces a file,
/// follows a link, or changes an owner or mode.
pub fn ensure_isolated_mount_host_points(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, ContainerExecError> {
    let working_dir = crate::load_container_exec_working_dir(repo_root, Some(&policy.name))
        .map_err(unavailable)?;
    ensure_points(
        repo_root,
        &isolated_mount_points(repo_root, policy, &working_dir)?,
    )
}

fn ensure_points(
    repo_root: &Path,
    points: &[IsolatedMountPoint],
) -> Result<Vec<String>, ContainerExecError> {
    let root = DirFd::open(repo_root).map_err(|error| {
        unavailable(format!(
            "cannot open checkout `{}` for isolated mount points: {error}",
            repo_root.display()
        ))
    })?;
    let mut created = Vec::new();
    for point in points {
        let relative = point.host_path.strip_prefix(repo_root).map_err(|_| {
            unavailable(format!(
                "isolated mount point `{}` is not inside the checkout `{}`",
                point.host_path.display(),
                repo_root.display()
            ))
        })?;
        let components = relative_components(point, relative)?;
        match root.create_component_path(&components, 0o777) {
            Ok((_, made)) => {
                if made {
                    created.push(point.host_path.display().to_string());
                }
            }
            Err(error) => return Err(host_point_refusal(point, error)),
        }
    }
    Ok(created)
}

/// The mount point's components below the checkout. `..`, a root or a prefix
/// is never part of a host mount point: such a path would leave the checkout.
fn relative_components<'a>(
    point: &IsolatedMountPoint,
    relative: &'a Path,
) -> Result<Vec<&'a str>, ContainerExecError> {
    relative
        .components()
        .map(|component| match component {
            Component::Normal(name) => name.to_str().ok_or_else(|| {
                unavailable(format!(
                    "isolated mount point `{}` is not valid UTF-8",
                    point.host_path.display()
                ))
            }),
            _ => Err(unavailable(format!(
                "isolated mount point `{}` leaves the checkout through `{}`",
                point.host_path.display(),
                relative.display()
            ))),
        })
        .collect()
}

/// The refusal that names the object which blocked preparation. The observation
/// below only classifies the refusal after the no-follow walk already refused:
/// it never grants creation authority and never repairs anything.
fn host_point_refusal(point: &IsolatedMountPoint, error: FsError) -> ContainerExecError {
    let offending = match &error {
        FsError::Untrusted { path, .. } | FsError::UnsupportedFilesystem { path, .. } => {
            path.clone()
        }
        _ => point.host_path.clone(),
    };
    let follow_advice = "A host output link sits on a container-isolated path: adopt the project's output links with `oxide outputs adopt` so the path stays a real directory, or drop it from `isolated_dirs`.";
    match std::fs::symlink_metadata(&offending) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = std::fs::read_link(&offending)
                .map(|target| target.display().to_string())
                .unwrap_or_else(|error| format!("<unreadable: {error}>"));
            if offending == point.host_path {
                unavailable(format!(
                    "isolated mount point `{}` is a symlink to `{target}`; refusing to follow it. {follow_advice}",
                    point.host_path.display()
                ))
            } else {
                unavailable(format!(
                    "isolated mount point `{}` is reached through a symlink at `{}` to `{target}`; refusing to follow it. {follow_advice}",
                    point.host_path.display(),
                    offending.display()
                ))
            }
        }
        Ok(_) => unavailable(format!(
            "isolated mount point `{}` is not a directory: `{}` is not one",
            point.host_path.display(),
            offending.display()
        )),
        Err(_) => unavailable(format!(
            "cannot prepare isolated mount point `{}`: {error}",
            point.host_path.display()
        )),
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
            ensure_points(fixture.path(), &points).unwrap(),
            vec![target.display().to_string()]
        );
        assert!(target.is_dir());
        assert!(
            ensure_points(fixture.path(), &points).unwrap().is_empty(),
            "idempotent"
        );

        let file = fixture.path().join("apps/farmyard/node_modules");
        std::fs::write(&file, "not a directory").unwrap();
        let error = ensure_points(
            fixture.path(),
            &[IsolatedMountPoint {
                host_path: file.clone(),
            }],
        )
        .expect_err("a file mount point refuses");
        assert!(error.to_string().contains("is not a directory"), "{error}");

        #[cfg(unix)]
        {
            let link = fixture.path().join("linked");
            std::os::unix::fs::symlink(fixture.path(), &link).unwrap();
            let error = ensure_points(fixture.path(), &[IsolatedMountPoint { host_path: link }])
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

    /// A genuine nested mount point below the checkout is accepted: the
    /// no-follow walk never refuses a real directory chain.
    #[test]
    fn plain_nested_ancestors_are_created() {
        let fixture = tempfile::tempdir().unwrap();
        let point = IsolatedMountPoint {
            host_path: fixture.path().join("apps/farmyard/target"),
        };
        assert_eq!(
            ensure_points(fixture.path(), std::slice::from_ref(&point)).unwrap(),
            vec![point.host_path.display().to_string()]
        );
        assert!(point.host_path.is_dir());
        assert!(fixture.path().join("apps/farmyard").is_dir());
        assert!(
            ensure_points(fixture.path(), std::slice::from_ref(&point))
                .unwrap()
                .is_empty(),
            "a present nested mount point is reused, never reported as created"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_ancestor_is_refused_before_anything_is_created() {
        let fixture = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(fixture.path().join("apps")).unwrap();
        let link = fixture.path().join("apps/farmyard");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        let point = IsolatedMountPoint {
            host_path: link.join("target"),
        };
        let error = ensure_points(fixture.path(), &[point])
            .expect_err("a symlinked ancestor refuses the mount point");
        let error = error.to_string();
        assert!(error.contains("reached through a symlink"), "{error}");
        assert!(
            error.contains(&format!(
                "at `{}` to `{}`",
                link.display(),
                outside.path().display()
            )),
            "the refusal names the ancestor link and its target: {error}"
        );
        assert!(error.contains("oxide outputs adopt"), "{error}");
        assert!(
            !outside.path().join("target").exists(),
            "nothing is created through the ancestor link"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_ancestor_with_a_present_leaf_still_refuses() {
        let fixture = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let present = outside.path().join("target");
        std::fs::create_dir_all(&present).unwrap();
        std::fs::create_dir_all(fixture.path().join("apps")).unwrap();
        let link = fixture.path().join("apps/farmyard");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        let error = ensure_points(
            fixture.path(),
            &[IsolatedMountPoint {
                host_path: link.join("target"),
            }],
        )
        .expect_err("an existing leaf behind a link is no authority either");
        assert!(
            error.to_string().contains("reached through a symlink"),
            "{error}"
        );
        assert_eq!(
            std::fs::read_dir(&present).unwrap().count(),
            0,
            "the link target is observed, never written"
        );
    }

    /// A component verified through a held directory stays authoritative: a
    /// link swapped in at the old path cannot redirect the next creation.
    #[cfg(unix)]
    #[test]
    fn a_replaced_intermediate_never_redirects_creation() {
        let fixture = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(fixture.path().join("apps")).unwrap();
        let root = DirFd::open(fixture.path()).unwrap();
        let (held, created) = root.create_component_path(&["apps"], 0o777).unwrap();
        assert!(!created, "the intermediate already existed");

        let moved = fixture.path().join("moved-apps");
        std::fs::rename(fixture.path().join("apps"), &moved).unwrap();
        std::os::unix::fs::symlink(outside.path(), fixture.path().join("apps")).unwrap();

        let (_, created) = held
            .create_component_path(&["farmyard", "target"], 0o777)
            .unwrap();
        assert!(
            created,
            "the missing leaf below the held directory is created"
        );
        assert!(moved.join("farmyard/target").is_dir());
        assert!(
            !outside.path().join("farmyard").exists(),
            "the replaced path never receives the creation"
        );
    }
}
