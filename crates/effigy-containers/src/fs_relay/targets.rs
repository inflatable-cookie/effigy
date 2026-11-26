//! Select writable bind mounts and safely translate host paths.
use crate::{ContainerPolicyError, EffectiveContainerPolicy};
use std::path::{Component, Path, PathBuf};

/// An observed bind mount of a running service. Container IDs address runtime exec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceMount {
    pub service: String,
    pub container: String,
    pub host: PathBuf,
    pub guest: PathBuf,
    pub writable: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayTarget {
    pub service: String,
    pub container: String,
    pub host: PathBuf,
    pub guest: PathBuf,
}

fn invalid(message: impl Into<String>) -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation(message.into())
}
fn safe_path(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|s| !s.contains(['\n', '\r', '\0']))
        && !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
}
fn no_symlinks(path: &Path) -> bool {
    let mut p = PathBuf::new();
    for c in path.components() {
        p.push(c);
        match std::fs::symlink_metadata(&p) {
            Ok(m) if m.file_type().is_symlink() => return false,
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return false,
            _ => {}
        }
    }
    true
}
impl RelayTarget {
    /// Normalize only the mount boundary; descendants remain subject to symlink checks.
    pub(super) fn canonicalize_root(&mut self) -> std::io::Result<()> {
        if !self.host.is_absolute() || !safe_path(&self.host) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid relay mount",
            ));
        }
        let root = self.host.canonicalize()?;
        if !safe_path(&root) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid canonical relay mount",
            ));
        }
        self.host = root;
        Ok(())
    }

    pub fn guest_path(&self, path: &Path) -> Option<PathBuf> {
        if !safe_path(path) || !no_symlinks(path) {
            return None;
        }
        let rel = path.strip_prefix(&self.host).ok()?;
        let result = self.guest.join(rel);
        safe_path(&result).then_some(result)
    }
}

/// Resolve policy-owned roots against runtime-observed mounts of running services.
/// `working_dir` is the guest workspace mount destination resolved by the caller.
pub fn resolve_targets(
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    observed: &[ServiceMount],
    shared_kernel: bool,
) -> Result<Vec<RelayTarget>, ContainerPolicyError> {
    let mut allowed = vec![(policy.repo_root.clone(), working_dir.to_path_buf())];
    for spec in &policy.declared_mounts {
        let Some((source, dest, options)) = crate::parse_mount_parts(spec) else {
            return Err(invalid(format!("invalid mount: {spec}")));
        };
        if options.is_some_and(|o| o.split(',').any(|s| s == "ro")) {
            continue;
        }
        allowed.push((PathBuf::from(source), PathBuf::from(dest)));
    }
    for spec in &policy.declared_media_mounts {
        let (source, dest) =
            crate::policy_support::parse_media_mount(&policy.repo_root, &policy.name, spec)?;
        allowed.push((source, PathBuf::from(dest)));
    }
    // Resolve a watched subpath against a declared mount boundary, never by
    // canonicalizing the subpath itself (which would follow in-tree symlinks).
    let repo_root = policy
        .repo_root
        .canonicalize()
        .map_err(|e| invalid(format!("invalid workspace root: {e}")))?;
    let mut candidates = Vec::new();
    let mut matched_paths = vec![false; policy.fs_events.paths.len()];
    for m in observed.iter().filter(|m| {
        m.writable
            && (policy.fs_events.services.is_empty()
                || policy.fs_events.services.contains(&m.service))
    }) {
        if !safe_path(&m.host)
            || !safe_path(&m.guest)
            || !m.host.is_absolute()
            || !m.guest.is_absolute()
        {
            return Err(invalid("invalid relay mount"));
        }
        let host = match m.host.canonicalize() {
            Ok(host) => host,
            // Runtime inventories can include VM-only sources such as named
            // volumes; they do not prevent relaying host-visible mounts.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(invalid(format!("invalid relay mount: {e}"))),
        };
        if !safe_path(&host) {
            return Err(invalid("invalid canonical relay mount"));
        }
        if !allowed
            .iter()
            .any(|(h, g)| g == &m.guest && h.canonicalize().ok().as_ref() == Some(&host))
        {
            continue;
        }
        let roots: Vec<(PathBuf, Option<usize>)> = if policy.fs_events.paths.is_empty() {
            vec![(host.clone(), None)]
        } else {
            policy
                .fs_events
                .paths
                .iter()
                .enumerate()
                .filter_map(|(index, p)| {
                    let p = Path::new(p);
                    if !safe_path(p) {
                        return None;
                    }
                    let p = if p.is_absolute() {
                        p.to_path_buf()
                    } else {
                        policy.repo_root.join(p)
                    };
                    let p = if let Ok(rel) = p.strip_prefix(&m.host) {
                        host.join(rel)
                    } else if let Ok(rel) = p.strip_prefix(&policy.repo_root) {
                        repo_root.join(rel)
                    } else {
                        p
                    };
                    p.starts_with(&host).then_some((p, Some(index)))
                })
                .collect()
        };
        for (root, index) in roots {
            if !safe_path(&root) || !no_symlinks(&root) {
                // A nested explicit mount can supply a safe mapping for this
                // path even when the enclosing workspace sees a symlink.
                continue;
            }
            if let Some(index) = index {
                matched_paths[index] = true;
            }
            // Collecting components drops the trailing separator that
            // joining an empty relative path adds.
            let guest = m
                .guest
                .join(
                    root.strip_prefix(&host)
                        .map_err(|_| invalid("relay path outside mount"))?,
                )
                .components()
                .collect::<PathBuf>();
            candidates.push(RelayTarget {
                service: m.service.clone(),
                container: m.container.clone(),
                host: root,
                guest,
            });
        }
    }
    for (path, matched) in policy.fs_events.paths.iter().zip(matched_paths) {
        if !matched {
            return Err(invalid(format!(
                "relay path outside running writable mounts: {path}"
            )));
        }
    }
    candidates.sort_by(|a, b| {
        (
            a.host.components().count(),
            a.service != policy.primary_service,
            &a.host,
            &a.service,
        )
            .cmp(&(
                b.host.components().count(),
                b.service != policy.primary_service,
                &b.host,
                &b.service,
            ))
    });
    let mut result: Vec<RelayTarget> = Vec::new();
    for target in candidates {
        if result.iter().any(|t| {
            target.host.starts_with(&t.host) && (shared_kernel || t.container == target.container)
        }) {
            continue;
        }
        result.push(target);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_vm_deduplicates_and_microvm_fans_out() {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().canonicalize().expect("root");
        let mut policy = crate::policy::model::test_policy(&root);
        policy.primary_service = "app".into();
        let mounts: Vec<_> = [("worker", "two"), ("app", "one")]
            .into_iter()
            .map(|(service, container)| ServiceMount {
                service: service.into(),
                container: container.into(),
                host: root.clone(),
                guest: "/workspace".into(),
                writable: true,
            })
            .collect();
        let shared =
            resolve_targets(&policy, Path::new("/workspace"), &mounts, true).expect("resolve");
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].service, "app");
        assert_eq!(
            resolve_targets(&policy, Path::new("/workspace"), &mounts, false)
                .expect("resolve")
                .len(),
            2
        );
        assert_eq!(
            shared[0].guest_path(&root.join("a")),
            Some(PathBuf::from("/workspace/a"))
        );
        assert_eq!(shared[0].guest_path(&root.join("../escape")), None);
        policy.fs_events.services = vec!["worker".into()];
        assert_eq!(
            resolve_targets(&policy, Path::new("/workspace"), &mounts, true).expect("resolve")[0]
                .service,
            "worker"
        );
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_and_selects_only_writable_paths() {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().canonicalize().expect("root");
        let nested = root.join("nested");
        std::fs::create_dir(&nested).expect("nested");
        std::os::unix::fs::symlink(&nested, root.join("link")).expect("link");
        let mut policy = crate::policy::model::test_policy(&root);
        policy.primary_service = "app".into();
        let mounts = vec![ServiceMount {
            service: "app".into(),
            container: "one".into(),
            host: root.clone(),
            guest: "/workspace".into(),
            writable: true,
        }];
        let targets =
            resolve_targets(&policy, Path::new("/workspace"), &mounts, true).expect("target");
        assert!(targets[0].guest_path(&root.join("link/file")).is_none());
        assert!(targets[0].guest_path(&root.join("bad\nname")).is_none());
        policy.fs_events.paths = vec!["link".into()];
        assert!(resolve_targets(&policy, Path::new("/workspace"), &mounts, true).is_err());
        policy.fs_events.paths.clear();
        let mut readonly = mounts;
        readonly[0].writable = false;
        assert!(
            resolve_targets(&policy, Path::new("/workspace"), &readonly, true)
                .expect("readonly")
                .is_empty()
        );
    }
    #[cfg(unix)]
    #[test]
    fn resolves_symlinked_mount_boundary_without_following_descendant_symlinks() {
        let dir = tempfile::tempdir().expect("fixture");
        let real = dir.path().join("real");
        std::fs::create_dir(&real).expect("real root");
        let real = real.canonicalize().expect("canonical root");
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).expect("alias");
        std::fs::create_dir(real.join("src")).expect("source");
        std::os::unix::fs::symlink(real.join("src"), real.join("link")).expect("nested link");
        let mut policy = crate::policy::model::test_policy(&alias);
        policy.primary_service = "app".into();
        let mount = ServiceMount {
            service: "app".into(),
            container: "one".into(),
            host: real.clone(),
            guest: "/workspace".into(),
            writable: true,
        };
        let targets = resolve_targets(
            &policy,
            Path::new("/workspace"),
            std::slice::from_ref(&mount),
            true,
        )
        .expect("resolve alias");
        assert_eq!(targets[0].host, real);
        assert_eq!(
            targets[0].guest_path(&real.join("src/deleted")),
            Some(PathBuf::from("/workspace/src/deleted"))
        );
        assert_eq!(targets[0].guest_path(&real.join("link/file")), None);
        policy.fs_events.paths = vec!["src".into()];
        let targets = resolve_targets(
            &policy,
            Path::new("/workspace"),
            std::slice::from_ref(&mount),
            true,
        )
        .expect("relative subpath");
        assert_eq!(targets[0].host, real.join("src"));
        policy.fs_events.paths = vec![alias.join("src").to_str().expect("utf8").into()];
        assert_eq!(
            resolve_targets(
                &policy,
                Path::new("/workspace"),
                std::slice::from_ref(&mount),
                true
            )
            .expect("absolute subpath")[0]
                .host,
            real.join("src")
        );
        policy.fs_events.paths = vec!["link/file".into()];
        assert!(resolve_targets(&policy, Path::new("/workspace"), &[mount], true).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn relative_path_can_use_an_explicit_symlinked_mount_boundary() {
        let fixture = tempfile::tempdir().expect("fixture");
        let repo = fixture.path().canonicalize().expect("root");
        let real = repo.join("real");
        std::fs::create_dir_all(real.join("src")).expect("source");
        let alias = repo.join("alias");
        std::os::unix::fs::symlink(&real, &alias).expect("mount alias");
        let mut policy = crate::policy::model::test_policy(&repo);
        policy.declared_mounts = vec![format!("{}:/data", alias.display())];
        policy.fs_events.paths = vec!["alias/src".into()];
        let mounts = [
            ServiceMount {
                service: "app".into(),
                container: "one".into(),
                host: repo,
                guest: "/workspace".into(),
                writable: true,
            },
            ServiceMount {
                service: "app".into(),
                container: "one".into(),
                host: alias,
                guest: "/data".into(),
                writable: true,
            },
        ];
        let targets = resolve_targets(&policy, Path::new("/workspace"), &mounts, true)
            .expect("resolve mount alias");
        assert_eq!(targets[0].host, real.join("src"));
        assert_eq!(targets[0].guest, PathBuf::from("/data/src"));
    }
    #[test]
    fn missing_observed_mount_does_not_block_valid_mount() {
        let fixture = tempfile::tempdir().expect("fixture");
        let repo = fixture.path().join("repo");
        std::fs::create_dir(&repo).expect("workspace");
        let repo = repo.canonicalize().expect("canonical workspace");
        let missing = fixture.path().join("vm-only");
        let mut policy = crate::policy::model::test_policy(&repo);
        let mounts = [
            ServiceMount {
                service: "app".into(),
                container: "one".into(),
                host: missing.clone(),
                guest: "/data".into(),
                writable: true,
            },
            ServiceMount {
                service: "app".into(),
                container: "one".into(),
                host: repo.clone(),
                guest: "/workspace".into(),
                writable: true,
            },
        ];
        let targets = resolve_targets(&policy, Path::new("/workspace"), &mounts, true)
            .expect("valid target remains available");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].host, repo);
        assert_eq!(targets[0].guest, PathBuf::from("/workspace"));

        policy.declared_mounts = vec![format!("{}:/data", missing.display())];
        policy.fs_events.paths = vec![missing.to_str().expect("utf8 path").into()];
        let error = resolve_targets(&policy, Path::new("/workspace"), &mounts, true)
            .expect_err("missing explicit mount is refused");
        assert!(error
            .to_string()
            .contains("relay path outside running writable mounts"));
    }
}
