//! Refreshing the container's view of the Cargo inputs the host changed, so an
//! incremental build never reuses an object file for edited source.

use std::path::{Path, PathBuf};

use effigy_containers::compose::compose_args;
use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;

pub(super) fn run_verified_prep_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
    args: &[std::ffi::OsString],
    label: &str,
) -> Result<std::process::Output, RunnerError> {
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        args.to_vec(),
        effigy_containers::ContainerAction::Exec,
        label,
    )
    .map_err(RunnerError::from)?;
    crate::runner::exec_command::run_verified_compose_exec_plan(
        policy, &plan, binding, true, None, None,
    )
}

pub(super) fn refresh_changed_cargo_inputs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    refresh_changed_cargo_inputs_using(repo_root, |changed| {
        for batch in changed.chunks(128) {
            let workdir = working_dir.to_string_lossy();
            let mut args = vec![
                "exec".to_owned(),
                "-T".to_owned(),
                "-w".to_owned(),
                workdir.into_owned(),
                policy.primary_service.clone(),
                "touch".to_owned(),
                "-c".to_owned(),
                "--".to_owned(),
            ];
            args.extend(batch.iter().map(|path| path.to_string_lossy().into_owned()));
            run_verified_prep_capture(
                repo_root,
                policy,
                binding,
                &compose_args(policy, args.iter().map(String::as_str)),
                "refresh changed Cargo inputs",
            )
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "cannot refresh changed Cargo inputs in selected checkout: {error}"
                ))
            })?;
        }
        Ok(())
    })
}

pub(super) fn refresh_changed_cargo_inputs_using(
    repo_root: &Path,
    touch_in_container: impl FnOnce(&[PathBuf]) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "-z", "HEAD", "--"])
        .current_dir(repo_root)
        .output()
        .map_err(|error| {
            RunnerError::task_invocation(format!("cannot list changed Cargo inputs: {error}"))
        })?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "cannot list changed Cargo inputs in {}: {}",
            repo_root.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let changed = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| std::path::PathBuf::from(String::from_utf8_lossy(bytes).as_ref()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "rs")
                || matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some("Cargo.toml" | "Cargo.lock")
                )
        })
        .filter(|path| repo_root.join(path).is_file())
        .collect::<Vec<_>>();
    if changed.is_empty() {
        return Ok(());
    }
    touch_in_container(&changed)
}
