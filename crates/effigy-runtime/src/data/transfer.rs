use std::path::Path;

use effigy_catalog::volumes::ManagedVolume;
use effigy_containers::{ContainerDataTransferAction, EffectiveContainerPolicy};

use crate::EffigyRuntimeError;

pub(super) fn resolve_managed_volume(
    policy: &EffectiveContainerPolicy,
    volume_name: &str,
) -> Result<ManagedVolume, EffigyRuntimeError> {
    let Some(volume) = policy
        .managed_volumes
        .iter()
        .find(|volume| volume.name == volume_name)
        .cloned()
    else {
        let available = policy
            .managed_volumes
            .iter()
            .map(|volume| format!("`{}`", volume.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(EffigyRuntimeError::task_invocation(format!(
            "managed volume `{volume_name}` is not owned by container `{}` (available: {available})",
            policy.name
        )));
    };
    Ok(volume)
}

pub(super) fn validate_transfer_path(
    archive_path: &Path,
    action: ContainerDataTransferAction,
) -> Result<(), EffigyRuntimeError> {
    match action {
        ContainerDataTransferAction::Export => {
            let parent = archive_path.parent().unwrap_or(Path::new("."));
            if !parent.is_dir() {
                return Err(EffigyRuntimeError::task_invocation(format!(
                    "export path parent directory does not exist: {}",
                    parent.display()
                )));
            }
        }
        ContainerDataTransferAction::Import => {
            if !archive_path.is_file() {
                return Err(EffigyRuntimeError::task_invocation(format!(
                    "import archive not found: {}",
                    archive_path.display()
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Output;
    use std::sync::{Arc, Mutex};

    use super::{resolve_managed_volume, run_volume_transfer, validate_transfer_path};
    use crate::test_support::generated_policy;
    use effigy_catalog::volumes::{DockerCommand, ManagedVolume};
    use effigy_containers::ContainerDataTransferAction;

    fn command_output(stdout: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn resolve_managed_volume_refuses_volumes_the_container_does_not_own() {
        let mut policy = generated_policy("web");
        policy.managed_volumes = vec![ManagedVolume {
            name: "web-project-demo-data".to_owned(),
            service: "db".to_owned(),
            persist: true,
            size_bytes: None,
            mount_point: None,
            mount_target: None,
        }];

        let owned = resolve_managed_volume(&policy, "web-project-demo-data").expect("owned volume");
        assert_eq!(owned.service, "db");
        assert!(owned.persist);

        // A volume the container does not declare is refused, never operated on.
        let error =
            resolve_managed_volume(&policy, "someone-elses-data").expect_err("foreign volume");
        let message = error.to_string();
        assert!(
            message.contains("not owned by container `web`"),
            "{message}"
        );
        assert!(message.contains("`web-project-demo-data`"), "{message}");
    }

    #[test]
    fn transfer_paths_must_exist_for_import_and_have_a_parent_for_export() {
        let base = tempfile::tempdir().expect("tempdir");
        let parent = base.path().join("exports");
        std::fs::create_dir(&parent).expect("mkdir exports");
        let archive = parent.join("demo.tar.gz");

        validate_transfer_path(&archive, ContainerDataTransferAction::Export)
            .expect("export parent exists");
        std::fs::write(&archive, b"payload").expect("write archive");
        validate_transfer_path(&archive, ContainerDataTransferAction::Import)
            .expect("import archive exists");

        let missing_parent = base.path().join("absent").join("demo.tar.gz");
        let error = validate_transfer_path(&missing_parent, ContainerDataTransferAction::Export)
            .expect_err("parent directory is missing");
        assert!(error
            .to_string()
            .contains("parent directory does not exist"));

        let error = validate_transfer_path(
            &base.path().join("absent.tar.gz"),
            ContainerDataTransferAction::Import,
        )
        .expect_err("archive is missing");
        assert!(error.to_string().contains("import archive not found"));
    }

    /// Identity is checked on the runtime labels before any transfer command
    /// runs; a lookalike volume is reported, never claimed (contract 052 R3).
    #[test]
    fn run_volume_transfer_refuses_lookalike_volumes_without_running_commands() {
        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let calls_for_capture = Arc::clone(&calls);
        let capture = move |_cwd: &Path, _endpoint: &str, command: &DockerCommand| {
            let rendered = command.args.join(" ");
            let output = if rendered.contains("volume ls") {
                "web-project-demo-data\tdriver\t\n".to_owned()
            } else if rendered.contains("volume inspect") {
                // Labels name a different project: a lookalike, not ours.
                "[{\"Name\":\"web-project-demo-data\",\"Mountpoint\":\"/var/lib/demo\",\"Labels\":{\"com.docker.compose.project\":\"someone-else\"}}]"
                    .to_owned()
            } else {
                String::new()
            };
            calls_for_capture.lock().expect("calls lock").push(rendered);
            Ok(command_output(&output))
        };
        let policy = generated_policy("web");

        let error = run_volume_transfer(
            Path::new("/tmp/repo"),
            &policy,
            "web-project-demo-data",
            Path::new("/tmp/demo.tar.gz"),
            ContainerDataTransferAction::Export,
            &capture,
        )
        .expect_err("lookalike volume must not transfer");

        assert!(error.to_string().contains("refuses to transfer data"));
        let calls = calls.lock().expect("calls lock");
        assert_eq!(calls.len(), 2, "only the listing and inspect may run");
        assert!(calls.iter().all(|call| call.contains("volume")));
    }
}
