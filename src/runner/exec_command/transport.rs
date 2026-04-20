use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command as ProcessCommand, Output, Stdio};
use std::thread;

use effigy_containers::{
    compose::{compose_args, compose_invocation, resolve_compose_backend, ComposeBackend},
    EffectiveContainerPolicy,
};
use effigy_env::secret::SecretString;
use effigy_exec::detection::{build_capabilities_from_results, standard_probe_spec, ProbeResult};

use crate::runner::error::RunnerError;

const CONTAINER_HANDOFF_ENV: &str = "EFFIGY_INTERNAL_CONTAINER_HANDOFF";
const CONTAINER_COLOR_ENV: [(&str, &str); 3] = [
    ("EFFIGY_COLOR", "always"),
    ("CLICOLOR_FORCE", "1"),
    ("FORCE_COLOR", "3"),
];
const CONTAINER_TTY_COLOR_ENV: [(&str, &str); 2] =
    [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

pub(super) fn build_routed_task_exec_args(
    strategy: &effigy_exec::ExecStrategy,
    secret_env: Option<&[(&str, &SecretString)]>,
    service: &str,
    mapped_cwd: &str,
) -> Vec<OsString> {
    let mut args = vec![OsString::from("exec"), OsString::from("-T")];
    append_exec_env(&mut args, secret_env);
    append_color_exec_env(&mut args, false);

    match strategy {
        effigy_exec::ExecStrategy::Handoff { args: handoff_args } => {
            args.push(OsString::from("-e"));
            args.push(OsString::from(format!("{CONTAINER_HANDOFF_ENV}=1")));
            args.push(OsString::from("-w"));
            args.push(OsString::from(mapped_cwd));
            args.push(OsString::from(service));
            args.push(OsString::from("effigy"));
            args.extend(handoff_args.iter().cloned().map(OsString::from));
        }
        effigy_exec::ExecStrategy::RawExec {
            working_dir,
            command,
        } => {
            args.push(OsString::from("-w"));
            args.push(OsString::from(working_dir));
            args.push(OsString::from(service));
            args.extend(command.iter().cloned().map(OsString::from));
        }
    }

    args
}

pub(in crate::runner) fn run_compose_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
) -> Result<Output, RunnerError> {
    if resolve_compose_backend() == ComposeBackend::ColimaNerdctl {
        return run_colima_direct_exec(repo_root, policy, args, capture, label);
    }

    let (program, resolved_args) = compose_invocation(policy, args);
    if capture {
        return run_command_capture_allow_failure(repo_root, program, &resolved_args);
    }

    let mut child = ProcessCommand::new(program)
        .current_dir(repo_root)
        .args(&resolved_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{label} ({program} {})", format_args(&resolved_args)),
            error,
        })?;
    let status = child
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: label.to_owned(),
            error,
        })?;
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

pub(in crate::runner) fn copy_file_into_service(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    host_source: &Path,
    container_dest: &str,
) -> Result<(), RunnerError> {
    let container_id = resolve_compose_service_container_id(repo_root, policy, service)?;
    let mut args = match resolve_compose_backend() {
        ComposeBackend::Docker => vec![OsString::from("cp")],
        ComposeBackend::ColimaNerdctl => vec![
            OsString::from("nerdctl"),
            OsString::from("--profile"),
            OsString::from(policy.profile.as_str()),
            OsString::from("--"),
            OsString::from("cp"),
        ],
    };
    args.push(OsString::from(host_source));
    args.push(OsString::from(format!(
        "{}:{}",
        container_id.to_string_lossy(),
        container_dest
    )));

    let (program, resolved_args) = match resolve_compose_backend() {
        ComposeBackend::Docker => ("docker", args),
        ComposeBackend::ColimaNerdctl => ("colima", args),
    };
    let output = run_command_capture_allow_failure(repo_root, program, &resolved_args)?;
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: format!("{program} {}", format_args(&resolved_args)),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

pub(super) fn parse_compose_exec_args(args: &[OsString]) -> Result<ParsedComposeExec, RunnerError> {
    let exec_index = args
        .iter()
        .position(|value| value.to_string_lossy() == "exec")
        .ok_or_else(|| RunnerError::task_invocation("missing compose exec command"))?;
    let mut iter = args[exec_index..].iter();
    let _exec = iter.next();

    let mut env = Vec::new();
    let mut working_dir: Option<OsString> = None;
    let mut user: Option<OsString> = None;
    let mut tty = true;
    let mut service: Option<String> = None;
    let mut command = Vec::new();
    while let Some(arg) = iter.next() {
        let value = arg.to_string_lossy();
        if service.is_none() {
            match value.as_ref() {
                "-T" => {
                    tty = false;
                    continue;
                }
                "-w" => {
                    working_dir = Some(iter.next().cloned().ok_or_else(|| {
                        RunnerError::task_invocation("missing exec working directory")
                    })?);
                    continue;
                }
                "-u" => {
                    user = Some(
                        iter.next()
                            .cloned()
                            .ok_or_else(|| RunnerError::task_invocation("missing exec user"))?,
                    );
                    continue;
                }
                "-e" => {
                    env.push(
                        iter.next().cloned().ok_or_else(|| {
                            RunnerError::task_invocation("missing exec env value")
                        })?,
                    );
                    continue;
                }
                _ if value.starts_with('-') => {
                    continue;
                }
                _ => {
                    service = Some(value.into_owned());
                    continue;
                }
            }
        }
        command.push(arg.clone());
        command.extend(iter.cloned());
        break;
    }

    Ok(ParsedComposeExec {
        env,
        working_dir,
        user,
        tty,
        service: service
            .ok_or_else(|| RunnerError::task_invocation("missing exec target service"))?,
        command,
    })
}

pub(super) fn run_command_capture_allow_failure(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
) -> Result<Output, RunnerError> {
    ProcessCommand::new(program)
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{program} {}", format_args(args)),
            error,
        })
}

pub(in crate::runner) fn probe_container_capabilities(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<effigy_exec::detection::ContainerCapabilities, RunnerError> {
    let mut results = HashMap::new();
    for check in standard_probe_spec().checks {
        let mut args = compose_args(policy, ["exec", "-T", service]);
        args.extend(check.command.iter().cloned().map(OsString::from));
        let output = run_command_capture_allow_failure_with_policy(repo_root, policy, &args)?;
        results.insert(
            check.description,
            ProbeResult {
                success: output.status.success(),
                output: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            },
        );
    }
    Ok(build_capabilities_from_results(&results))
}

pub(super) struct ParsedComposeExec {
    pub(super) env: Vec<OsString>,
    pub(super) working_dir: Option<OsString>,
    pub(super) user: Option<OsString>,
    pub(super) tty: bool,
    pub(super) service: String,
    pub(super) command: Vec<OsString>,
}

fn append_exec_env(args: &mut Vec<OsString>, secret_env: Option<&[(&str, &SecretString)]>) {
    for (key, value) in secret_env.unwrap_or(&[]) {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("{key}={}", value.expose())));
    }
}

pub(in crate::runner) fn append_color_exec_env(args: &mut Vec<OsString>, tty: bool) {
    for (key, value) in CONTAINER_COLOR_ENV {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("{key}={value}")));
    }
    if tty {
        for (key, value) in CONTAINER_TTY_COLOR_ENV {
            args.push(OsString::from("-e"));
            args.push(OsString::from(format!("{key}={value}")));
        }
    }
}

fn run_colima_direct_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    compose_exec_args: &[OsString],
    capture: bool,
    label: &str,
) -> Result<Output, RunnerError> {
    let parsed = parse_compose_exec_args(compose_exec_args)?;
    let resolved = resolve_colima_direct_exec_invocation(repo_root, policy, compose_exec_args)?;
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

fn resolve_colima_direct_exec_invocation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    compose_exec_args: &[OsString],
) -> Result<Vec<OsString>, RunnerError> {
    let parsed = parse_compose_exec_args(compose_exec_args)?;
    let container_id = resolve_compose_service_container_id(repo_root, policy, &parsed.service)?;

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
    if let Some(working_dir) = parsed.working_dir {
        args.push(OsString::from("-w"));
        args.push(working_dir);
    }
    if let Some(user) = parsed.user {
        args.push(OsString::from("-u"));
        args.push(user);
    }
    for env in parsed.env {
        args.push(OsString::from("-e"));
        args.push(env);
    }
    args.push(container_id);
    args.extend(parsed.command);
    Ok(args)
}

fn resolve_compose_service_container_id(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
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

fn run_command_capture_allow_failure_with_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
) -> Result<Output, RunnerError> {
    let (program, resolved_args) = compose_invocation(policy, args);
    run_command_capture_allow_failure(repo_root, program, &resolved_args)
}

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}
