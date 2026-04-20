use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command as ProcessCommand, Output, Stdio};
use std::thread;

use effigy_containers::{
    compose::{compose_args, compose_invocation},
    EffectiveContainerPolicy,
};

use crate::runner::error::RunnerError;

use super::ParsedComposeExec;

pub(super) fn run_colima_direct_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    compose_exec_args: &[OsString],
    capture: bool,
    label: &str,
    parse_compose_exec_args: &dyn Fn(&[OsString]) -> Result<ParsedComposeExec, RunnerError>,
    run_command_capture_allow_failure: &dyn Fn(
        &Path,
        &str,
        &[OsString],
    ) -> Result<Output, RunnerError>,
    format_args: &dyn Fn(&[OsString]) -> String,
) -> Result<Output, RunnerError> {
    let parsed = parse_compose_exec_args(compose_exec_args)?;
    let resolved = resolve_colima_direct_exec_invocation(
        repo_root,
        policy,
        &parsed,
        run_command_capture_allow_failure,
        format_args,
    )?;
    if capture {
        return run_command_capture_allow_failure(repo_root, "colima", &resolved);
    }

    let suppress_exit_noise = looks_like_interactive_shell_exec(&parsed);
    let mut command = ProcessCommand::new("colima");
    command
        .current_dir(repo_root)
        .args(&resolved)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(if suppress_exit_noise {
            Stdio::piped()
        } else {
            Stdio::inherit()
        });
    let mut child = command
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{label} (colima {})", format_args(&resolved)),
            error,
        })?;
    let stderr_thread = if suppress_exit_noise {
        child
            .stderr
            .take()
            .map(|stderr| thread::spawn(move || forward_colima_exec_stderr(stderr)))
    } else {
        None
    };
    let status = child
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: label.to_owned(),
            error,
        })?;
    if let Some(handle) = stderr_thread {
        let _ = handle.join();
    }
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

fn resolve_colima_direct_exec_invocation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    parsed: &ParsedComposeExec,
    run_command_capture_allow_failure: &dyn Fn(
        &Path,
        &str,
        &[OsString],
    ) -> Result<Output, RunnerError>,
    format_args: &dyn Fn(&[OsString]) -> String,
) -> Result<Vec<OsString>, RunnerError> {
    let container_id = resolve_compose_service_container_id(
        repo_root,
        policy,
        &parsed.service,
        run_command_capture_allow_failure,
        format_args,
    )?;

    let mut args = vec![
        OsString::from("nerdctl"),
        OsString::from("--profile"),
        OsString::from(policy.profile.as_str()),
        OsString::from("--"),
        OsString::from("exec"),
    ];
    if parsed.tty {
        args.push(OsString::from("-i"));
        args.push(OsString::from("-t"));
    }
    if let Some(working_dir) = parsed.working_dir.as_ref() {
        args.push(OsString::from("-w"));
        args.push(working_dir.clone());
    }
    if let Some(user) = parsed.user.as_ref() {
        args.push(OsString::from("-u"));
        args.push(user.clone());
    }
    for env in &parsed.env {
        args.push(OsString::from("-e"));
        args.push(env.clone());
    }
    args.push(container_id);
    args.extend(parsed.command.iter().cloned());
    Ok(args)
}

pub(super) fn resolve_compose_service_container_id(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    run_command_capture_allow_failure: &dyn Fn(
        &Path,
        &str,
        &[OsString],
    ) -> Result<Output, RunnerError>,
    format_args: &dyn Fn(&[OsString]) -> String,
) -> Result<OsString, RunnerError> {
    let mut args = compose_args(policy, ["ps", "-q"]);
    args.push(OsString::from(service));
    let (program, resolved_args) = compose_invocation(policy, &args);
    let output = run_command_capture_allow_failure(repo_root, program, &resolved_args)?;
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: format!("{program} {}", format_args(&resolved_args)),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if container_id.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "container service `{service}` is not running"
        )));
    }
    Ok(OsString::from(container_id))
}

fn looks_like_interactive_shell_exec(parsed: &ParsedComposeExec) -> bool {
    if !parsed.tty || parsed.command.is_empty() {
        return false;
    }
    let program = parsed.command[0].to_string_lossy();
    if !program.contains("sh") && !program.contains("bash") && !program.contains("zsh") {
        return false;
    }
    parsed
        .command
        .get(1)
        .is_some_and(|value| matches!(value.to_string_lossy().as_ref(), "-i" | "-lc"))
}

fn forward_colima_exec_stderr(stderr: impl std::io::Read) {
    let mut reader = BufReader::new(stderr);
    let mut line = Vec::new();
    let mut sink = std::io::stderr().lock();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {
                if should_suppress_colima_exec_stderr_line(&line) {
                    continue;
                }
                let _ = sink.write_all(&line);
                let _ = sink.flush();
            }
            Err(_) => break,
        }
    }
}

fn should_suppress_colima_exec_stderr_line(line: &[u8]) -> bool {
    let text = String::from_utf8_lossy(line);
    let trimmed = text.trim();
    trimmed.starts_with("FATA[")
        && matches!(
            trimmed.split_once("] ").map(|(_, message)| message),
            Some("exec failed with exit code 1") | Some("exit status 1")
        )
}
