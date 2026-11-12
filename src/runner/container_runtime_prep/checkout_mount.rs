//! Proving that the container's working directory is the selected checkout,
//! before a task lease is refreshed or any user command runs.

use std::path::Path;

use effigy_containers::compose::compose_args;
use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;

use super::cargo_inputs::run_verified_prep_capture;

/// A running Compose service can be healthy while its bind mount still points
/// at an earlier checkout. Prove the selected checkout is visible before a
/// task lease is refreshed or any user command runs.
pub(super) fn verify_live_checkout_mount(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    verify_live_checkout_mount_using(
        repo_root,
        working_dir,
        |probe_path| {
            let workdir = working_dir.to_string_lossy();
            let args = compose_args(
                policy,
                [
                    "exec",
                    "-T",
                    "-w",
                    workdir.as_ref(),
                    policy.primary_service.as_str(),
                    "cat",
                    probe_path,
                ],
            );
            let output = run_verified_prep_capture(
                repo_root,
                policy,
                binding,
                &args,
                "checkout mount read probe",
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            Ok(output.stdout)
        },
        |probe_path| {
            let workdir = working_dir.to_string_lossy();
            let args = compose_args(
                policy,
                [
                    "exec",
                    "-T",
                    "-w",
                    workdir.as_ref(),
                    policy.primary_service.as_str(),
                    "touch",
                    "--",
                    probe_path,
                ],
            );
            run_verified_prep_capture(
                repo_root,
                policy,
                binding,
                &args,
                "checkout mount write probe",
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            Ok(())
        },
    )
}

pub(super) fn verify_live_checkout_mount_using(
    repo_root: &Path,
    working_dir: &Path,
    read_in_container: impl FnOnce(&str) -> Result<Vec<u8>, RunnerError>,
    write_in_container: impl FnOnce(&str) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    use std::io::Write;
    // The probe must live in the checkout, at the same relative location as
    // the container working directory. A fresh name also defeats stale copies.
    let name = format!(
        ".effigy-checkout-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
            .as_nanos()
    );
    let host_path = repo_root.join(&name);
    let write_name = format!("{name}-write");
    let host_write_path = repo_root.join(&write_name);
    let payload = format!("{}:{name}", repo_root.display());
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&host_path)
        .map_err(|error| {
            RunnerError::task_invocation(format!("cannot create checkout mount probe: {error}"))
        })?;
    let result = (|| {
        file.write_all(payload.as_bytes()).map_err(|error| {
            RunnerError::task_invocation(format!("cannot write checkout mount probe: {error}"))
        })?;
        file.sync_all().map_err(|error| {
            RunnerError::task_invocation(format!("cannot sync checkout mount probe: {error}"))
        })?;
        let container_path = working_dir.join(&name);
        let actual = read_in_container(&container_path.to_string_lossy())?;
        if actual != payload.as_bytes() {
            return Err(RunnerError::task_invocation(format!(
                "container checkout mount at {} does not match selected checkout {}; refusing task execution",
                working_dir.display(), repo_root.display()
            )));
        }
        write_in_container(&working_dir.join(&write_name).to_string_lossy())?;
        if !host_write_path.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "container checkout mount at {} did not write to selected checkout {}; refusing task execution",
                working_dir.display(), repo_root.display()
            )));
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&host_path);
    let _ = std::fs::remove_file(&host_write_path);
    result.map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot verify container checkout mount for {}: {error}",
            repo_root.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::super::tests::temp_test_dir;
    use super::verify_live_checkout_mount_using;

    #[test]
    fn checkout_mount_probe_accepts_selected_checkout_and_rejects_sibling() {
        let first = temp_test_dir("checkout-first");
        let sibling = temp_test_dir("checkout-sibling");
        let working_dir = Path::new("/workspace/root");
        verify_live_checkout_mount_using(
            &first,
            working_dir,
            |path| {
                let name = Path::new(path).file_name().unwrap();
                Ok(fs::read(first.join(name)).unwrap())
            },
            |path| {
                let name = Path::new(path).file_name().unwrap();
                fs::write(first.join(name), "generated").unwrap();
                Ok(())
            },
        )
        .expect("selected checkout must be visible");
        let error = verify_live_checkout_mount_using(
            &first,
            working_dir,
            |path| {
                let name = Path::new(path).file_name().unwrap();
                Ok(fs::read(sibling.join(name)).unwrap_or_default())
            },
            |_| Ok(()),
        )
        .expect_err("sibling mount must fail closed");
        assert!(error
            .to_string()
            .contains("does not match selected checkout"));
        assert_eq!(fs::read_dir(&first).unwrap().count(), 0);
        assert_eq!(fs::read_dir(&sibling).unwrap().count(), 0);
        fs::remove_dir_all(first).unwrap();
        fs::remove_dir_all(sibling).unwrap();
    }
}
