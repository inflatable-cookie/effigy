//! One runtime command on the environment's exact backend.
//!
//! Every helper here resolves the backend the policy's endpoint names, runs the
//! command, and translates a non-zero exit into a refusal that names the
//! command; the volume reads the volume commands need are the same shape.

use std::ffi::OsString;
use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{
    remove_volume_command, DockerCommand, VolumeClassification, VolumeIdentity,
};
use effigy_containers::backend::endpoint_backend;
use effigy_containers::{
    BackendId, ContainerAction, ContainerBackendDetection, ContainerInterruptPolicy,
    ContainerManager, ContainerManagerRequest, EffectiveContainerPolicy,
};
use effigy_core::shell::shell_quote;

use crate::runner::container_command::RunnerError;

/// Stop a container, then remove it (effigy#376). A forced remove of a running
/// container is a kill the runtime's restart manager undoes for the
/// `restart: unless-stopped` policy `up` asked the runtime for; an explicit stop is
/// recorded as a user stop, so the removal holds. A stop that finds nothing
/// running is not a failure.
pub(in crate::runner::container_command) fn stop_and_remove_container(
    cwd: &Path,
    profile: &str,
    name: &str,
) -> Result<(), RunnerError> {
    let stop = DockerCommand {
        program: "docker".to_owned(),
        args: vec!["stop".to_owned(), name.to_owned()],
        description: format!("stop owned container {name}"),
    };
    let _ = run_runtime_volume_capture(cwd, profile, &stop);
    let remove = DockerCommand {
        program: "docker".to_owned(),
        args: vec!["rm".to_owned(), "-f".to_owned(), name.to_owned()],
        description: format!("remove owned container {name}"),
    };
    run_runtime_volume_capture(cwd, profile, &remove).map(|_| ())
}

/// Remove only volumes whose labels prove this environment owns them, on its
/// exact backend. A same-named lookalike is reported and left alone (R3).
pub(in crate::runner::container_command) fn remove_reset_volumes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    classification: &VolumeClassification,
) -> Result<(), RunnerError> {
    let endpoint = effigy_containers::backend::runtime_endpoint(repo_root, policy)?;
    let identities = effigy_runtime::data::volume_identities(
        repo_root,
        &endpoint,
        &policy.project_name,
        &classification.remove,
        &|cwd: &Path, endpoint: &str, command: &DockerCommand| {
            run_runtime_volume_capture(cwd, endpoint, command)
                .map_err(crate::runner::container_command::runtime_error_from_runner)
        },
    )
    .map_err(RunnerError::from)?;
    for name in &classification.remove {
        match identities.get(name) {
            Some(VolumeIdentity::Owned) => {
                run_runtime_volume_capture(repo_root, &endpoint, &remove_volume_command(name))?;
            }
            Some(VolumeIdentity::Lookalike(reason)) => {
                eprintln!(
                    "[warn] reset left {reason}; it was not created for `{}`",
                    policy.name
                );
            }
            Some(VolumeIdentity::Absent) | None => {}
        }
    }
    Ok(())
}

pub(in crate::runner::container_command) fn run_runtime_volume_capture(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<Output, RunnerError> {
    if command.program == "__effigy_volume_usage" {
        return run_runtime_volume_usage_capture(repo_root, profile, command);
    }
    if command.program == "__effigy_volume_usage_batch" {
        return run_runtime_volume_usage_batch_capture(repo_root, profile, command);
    }
    if command.program == "__effigy_volume_cache_kind_batch" {
        return run_runtime_volume_cache_kind_batch_capture(repo_root, profile, command);
    }
    let plan = runtime_volume_plan(repo_root, profile, command)?;
    capture_runtime(repo_root, &command.description, &plan.program, &plan.args)
}

/// Resolve one exact backend command without executing it, for kernel custody.
pub(in crate::runner::container_command) fn runtime_volume_plan(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<effigy_containers::ContainerRuntimeInvocationPlan, RunnerError> {
    let docker_args = runtime_args(&command.args);
    let request =
        ContainerManagerRequest::new(repo_root).interrupt_policy(ContainerInterruptPolicy::Forward);
    // The endpoint carries the exact backend; see `endpoint_backend`.
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(endpoint_backend(profile));
    ContainerManager::defaults()
        .runtime_invocation_plan(
            &request,
            &detection,
            effigy_containers::RuntimeInvocation {
                profile,
                docker_program: command.program.as_str().into(),
                docker_args: &docker_args,
                action: ContainerAction::Shutdown,
                label: command.description.clone(),
            },
        )
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn run_runtime_volume_usage_capture(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<Output, RunnerError> {
    let Some(mount_point) = command.args.first() else {
        return Err(RunnerError::task_invocation(
            "runtime volume usage command requires one mount-point argument",
        ));
    };
    let backend_id = endpoint_backend(profile);
    let (program, args) = if backend_id == BackendId::docker_compose() {
        (
            OsString::from("du"),
            vec![OsString::from("-sk"), OsString::from(mount_point)],
        )
    } else {
        (
            effigy_containers::compose::resolve_host_cli_program("colima"),
            vec![
                OsString::from("ssh"),
                OsString::from("--profile"),
                OsString::from(profile),
                OsString::from("--"),
                OsString::from("sudo"),
                OsString::from("du"),
                OsString::from("-sk"),
                OsString::from(mount_point),
            ],
        )
    };

    capture_runtime(repo_root, &command.description, &program, &args)
}

fn run_runtime_volume_usage_batch_capture(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<Output, RunnerError> {
    if command.args.is_empty() {
        return Err(RunnerError::task_invocation(
            "runtime volume usage batch command requires one or more mount-point arguments",
        ));
    }
    let backend_id = endpoint_backend(profile);
    let (program, args) = if backend_id == BackendId::docker_compose() {
        let mut args = vec![OsString::from("-sk")];
        args.extend(command.args.iter().map(OsString::from));
        (OsString::from("du"), args)
    } else {
        let mut args = vec![
            OsString::from("ssh"),
            OsString::from("--profile"),
            OsString::from(profile),
            OsString::from("--"),
            OsString::from("sudo"),
            OsString::from("du"),
            OsString::from("-sk"),
        ];
        args.extend(command.args.iter().map(OsString::from));
        (
            effigy_containers::compose::resolve_host_cli_program("colima"),
            args,
        )
    };

    capture_runtime(repo_root, &command.description, &program, &args)
}

fn run_runtime_volume_cache_kind_batch_capture(
    repo_root: &Path,
    profile: &str,
    command: &DockerCommand,
) -> Result<Output, RunnerError> {
    if command.args.is_empty() {
        return Err(RunnerError::task_invocation(
            "runtime volume cache-kind batch command requires one or more mount-point arguments",
        ));
    }
    let script = render_volume_cache_kind_script(&command.args);
    let backend_id = endpoint_backend(profile);
    let (program, args) = if backend_id == BackendId::docker_compose() {
        (
            OsString::from("sh"),
            vec![OsString::from("-lc"), OsString::from(script)],
        )
    } else {
        (
            effigy_containers::compose::resolve_host_cli_program("colima"),
            vec![
                OsString::from("ssh"),
                OsString::from("--profile"),
                OsString::from(profile),
                OsString::from("--"),
                OsString::from("sudo"),
                OsString::from("sh"),
                OsString::from("-lc"),
                OsString::from(script),
            ],
        )
    };

    capture_runtime(repo_root, &command.description, &program, &args)
}

fn render_volume_cache_kind_script(mount_points: &[String]) -> String {
    let mut script = String::from(
        r#"set -eu
classify_cache_volume() {
  path="$1"
  kind=""
  if [ -d "$path/debug/deps" ] || [ -d "$path/release/deps" ] || [ -f "$path/.rustc_info.json" ]; then
    kind="rust-target"
  fi
  if [ -n "$kind" ]; then
    printf '%s\t%s\n' "$path" "$kind"
  fi
}
"#,
    );
    for mount_point in mount_points {
        script.push_str("classify_cache_volume ");
        script.push_str(&shell_quote(mount_point));
        script.push('\n');
    }
    script
}

/// Run one resolved program and translate its own result.
///
/// A launch failure names the command that never started; a non-zero exit is
/// the refusal a person reads, with both streams it wrote. Every runtime
/// command this module runs goes through here, so "one command, one refusal"
/// has one owner (effigy#933).
fn capture_runtime(
    repo_root: &Path,
    description: &str,
    program: &OsString,
    args: &[OsString],
) -> Result<Output, RunnerError> {
    std::process::Command::new(program)
        .current_dir(repo_root)
        // Run tokens stay out of privileged helper commands.
        .args(args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{description} ({} {})",
                program.to_string_lossy(),
                format_args(args)
            ),
            error,
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(RunnerError::task_invocation(format!(
                    "{description} failed ({})\nstdout:\n{}\nstderr:\n{}",
                    effigy_ui::human::exit_code(output.status.code()),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        })
}

fn runtime_args(args: &[String]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

pub(super) fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}
