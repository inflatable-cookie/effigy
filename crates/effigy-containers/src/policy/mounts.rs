//! Bind-source creation belongs to Prepare, never policy resolution.
use std::collections::BTreeSet;

use crate::{ContainerPolicyError, EffectiveContainerPolicy};

/// Prepare the directories Effigy owns for declared media and Composer cache.
/// Other host mounts remain user-owned and must already exist.
pub fn prepare_container_mount_sources(
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    let mut directories = BTreeSet::new();
    for mount in &policy.declared_media_mounts {
        let (source, _) =
            crate::policy_support::parse_media_mount(&policy.repo_root, &policy.name, mount)?;
        directories.insert(source);
    }
    if let Some(home) = crate::policy_support::effigy_home_dir() {
        let cache = home.join("shared/composer-cache");
        let expected = format!("{}:/home/dev/.cache/composer", cache.display());
        for file in &policy.compose_files {
            let content = policy.read_compose(file)?;
            let compose: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
                ContainerPolicyError::TaskInvocation(format!(
                    "cannot read mount sources from {}: {error}",
                    file.display()
                ))
            })?;
            let Some(services) = compose
                .get("services")
                .and_then(serde_yaml::Value::as_mapping)
            else {
                continue;
            };
            if services.values().any(|service| {
                service
                    .get("volumes")
                    .and_then(serde_yaml::Value::as_sequence)
                    .is_some_and(|volumes| {
                        volumes
                            .iter()
                            .any(|volume| volume.as_str() == Some(expected.as_str()))
                    })
            }) {
                directories.insert(cache.clone());
            }
        }
    }
    for directory in directories {
        std::fs::create_dir_all(&directory).map_err(|error| ContainerPolicyError::Read {
            path: directory,
            error,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::model::test_policy;

    #[test]
    fn mount_prepare_creates_media_and_is_idempotent() {
        let root = tempfile::tempdir().expect("private fixture");
        let home = tempfile::tempdir().expect("private state");
        crate::policy_support::with_test_effigy_home(home.path(), || {
            let mut policy = test_policy(root.path());
            policy.declared_media_mounts = vec!["storage/uploads:/uploads".into()];
            crate::policy_support::validate_media_mounts(
                root.path(),
                &policy.name,
                &policy.declared_media_mounts,
            )
            .expect("pure validation");
            assert!(!root.path().join("storage").exists());
            prepare_container_mount_sources(&policy).expect("prepare");
            let marker = root.path().join("storage/uploads/existing");
            std::fs::write(&marker, b"keep").expect("existing data");
            prepare_container_mount_sources(&policy).expect("repeat prepare");
            assert_eq!(std::fs::read(marker).expect("data retained"), b"keep");
        });
    }

    #[cfg(unix)]
    #[test]
    fn mount_prepare_revalidates_media_after_symlink_replacement() {
        let fixture = tempfile::tempdir().expect("private fixture");
        let root = fixture.path().join("checkout");
        let other = fixture.path().join("other");
        std::fs::create_dir(&root).expect("checkout");
        std::fs::create_dir(&other).expect("other private directory");
        let mut policy = test_policy(&root);
        policy.declared_media_mounts = vec!["storage/uploads:/uploads".into()];
        crate::policy_support::validate_media_mounts(
            &root,
            &policy.name,
            &policy.declared_media_mounts,
        )
        .expect("validate absent source");
        std::os::unix::fs::symlink(&other, root.join("storage")).expect("private symlink");
        assert!(prepare_container_mount_sources(&policy).is_err());
        assert!(!other.join("uploads").exists());
    }
}
