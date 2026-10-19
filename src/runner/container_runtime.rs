use std::path::Path;
use std::process::{Command, Output, Stdio};

use super::error::RunnerError;

pub(in crate::runner) const CONTAINER_HANDOFF_ENV_NAME: &str = "EFFIGY_INTERNAL_CONTAINER_HANDOFF";
pub(in crate::runner) const CONTAINER_HANDOFF_ENV_ASSIGNMENT: &str =
    "EFFIGY_INTERNAL_CONTAINER_HANDOFF=1";

pub(in crate::runner) fn inside_container_handoff() -> bool {
    std::env::var_os(CONTAINER_HANDOFF_ENV_NAME).is_some()
}

pub(in crate::runner) use effigy_containers::ContainerExecTarget as HandoffLocalExecution;

/// Resolve handoff-local execution when this process already runs inside its
/// own workspace container (`EFFIGY_INTERNAL_CONTAINER_HANDOFF=1`).
///
/// Returns `None` on the host so host flows keep their runtime checks.
/// Resolution reads only manifest execution coordinates (container, service and
/// working directory): it never spawns `colima`, `nerdctl`, `docker` or
/// `compose`, and never refreshes the workspace effigy install (the running
/// binary proves it is present).
///
/// By construction the handoff marker is only set by Effigy's own compose
/// exec into the target service, so the local container is the requested
/// one; a manually exported marker runs the command where the process is.
pub(in crate::runner) fn resolve_handoff_local_execution(
    repo_root: &Path,
    container_name: Option<&str>,
    service: Option<&str>,
) -> Option<Result<HandoffLocalExecution, RunnerError>> {
    if !inside_container_handoff() {
        return None;
    }
    Some(
        effigy_containers::load_container_exec_target(repo_root, container_name, service)
            .map_err(|error| RunnerError::task_invocation(error.to_string())),
    )
}

fn check_handoff_working_dir(execution: &HandoffLocalExecution) -> Result<(), RunnerError> {
    if execution.working_dir.is_dir() {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "container `{}` handoff working directory `{}` is not available inside the workspace container",
        execution.container,
        execution.working_dir.display()
    )))
}

/// Run `command` as a local shell inside the workspace container.
///
/// `command` runs through `sh -lc` in `working_dir` with `env` applied; the
/// handoff environment is inherited. No host runtime is contacted.
/// `capture` pipes stdout/stderr for callers that render results, otherwise
/// stdio is inherited like a streamed compose exec.
pub(in crate::runner) fn run_handoff_local_shell_command(
    execution: &HandoffLocalExecution,
    command: &str,
    env: &[(String, String)],
    capture: bool,
) -> Result<Output, RunnerError> {
    check_handoff_working_dir(execution)?;
    let mut process = Command::new("sh");
    process
        .arg("-lc")
        .arg(command)
        .current_dir(&execution.working_dir);
    for (key, value) in env {
        process.env(key, value);
    }
    if capture {
        process.stdin(Stdio::null());
        process.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        process.stdin(Stdio::inherit());
        process.stdout(Stdio::inherit());
        process.stderr(Stdio::inherit());
    }
    super::owned_children::configure_owned_process_group(&mut process);
    process
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "container `{}` handoff shell command `{command}`",
                execution.container
            ),
            error,
        })
}

/// Run an interactive shell where the caller already is.
///
/// Used for bare `container shell` inside the workspace container: the
/// login shell runs with inherited stdio instead of a second compose exec
/// hop that could never reach a host runtime from inside.
pub(in crate::runner) fn run_handoff_local_interactive_shell(
    execution: &HandoffLocalExecution,
) -> Result<Output, RunnerError> {
    check_handoff_working_dir(execution)?;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
    let mut process = Command::new(&shell);
    process.arg("-i").current_dir(&execution.working_dir);
    process.stdin(Stdio::inherit());
    process.stdout(Stdio::inherit());
    process.stderr(Stdio::inherit());
    super::owned_children::configure_owned_process_group(&mut process);
    let status = process
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "container `{}` handoff interactive shell `{shell}`",
                execution.container
            ),
            error,
        })?
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "container `{}` handoff interactive shell `{shell}`",
                execution.container
            ),
            error,
        })?;
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

/// Run `argv` directly where the caller already is (the `effigy exec`
/// shape: no intermediate shell). Mirrors streamed versus captured compose
/// exec stdio handling. `working_dir` overrides the resolved container
/// working directory when the caller already resolved one; `stdin_file`
/// feeds standard input for file-backed captures.
pub(in crate::runner) fn run_handoff_local_direct_command(
    execution: &HandoffLocalExecution,
    argv: &[String],
    capture: bool,
) -> Result<Output, RunnerError> {
    run_handoff_local_direct_command_with_options(execution, argv, None, None, &[], capture)
}

/// Flexible form of [`run_handoff_local_direct_command`] for container exec
/// operations that carry their own working directory or stdin file.
pub(in crate::runner) fn run_handoff_local_direct_command_with_options(
    execution: &HandoffLocalExecution,
    argv: &[String],
    working_dir: Option<&Path>,
    stdin_file: Option<&Path>,
    env: &[(String, String)],
    capture: bool,
) -> Result<Output, RunnerError> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| RunnerError::task_invocation("`effigy exec` requires a command to run"))?;
    let working_dir = working_dir.unwrap_or(execution.working_dir.as_path());
    if !working_dir.is_dir() {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` handoff working directory `{}` is not available inside the workspace container",
            execution.container,
            working_dir.display()
        )));
    }
    let mut process = Command::new(program);
    process.args(args).current_dir(working_dir);
    for (key, value) in env {
        process.env(key, value);
    }
    let stdin = match stdin_file {
        Some(path) => Stdio::from(std::fs::File::open(path).map_err(|error| {
            RunnerError::task_invocation(format!(
                "container `{}` handoff stdin file `{}` is not readable: {error}",
                execution.container,
                path.display()
            ))
        })?),
        None if capture => Stdio::null(),
        None => Stdio::inherit(),
    };
    process.stdin(stdin);
    if capture {
        process.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        process.stdout(Stdio::inherit());
        process.stderr(Stdio::inherit());
    }
    super::owned_children::configure_owned_process_group(&mut process);
    if capture {
        return process
            .output()
            .map_err(|error| RunnerError::TaskCommandLaunch {
                command: argv.join(" "),
                error,
            });
    }
    let status = process
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: argv.join(" "),
            error,
        })?
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: argv.join(" "),
            error,
        })?;
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}
