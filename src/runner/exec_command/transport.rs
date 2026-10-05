use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command as ProcessCommand, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use effigy_containers::{
    compose::{
        compose_args, compose_invocation_for_repo, resolve_compose_backend_for_repo,
        resolve_host_cli_program, ComposeBackend,
    },
    EffectiveContainerPolicy,
};
use effigy_containers::{
    BackendId, ContainerBackendDetection, ContainerComposeInvocationPlan, ContainerManager,
};
use effigy_env::secret::SecretString;
use effigy_exec::detection::{build_capabilities_from_results, standard_probe_spec, ProbeResult};

use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_ASSIGNMENT;
use crate::runner::error::RunnerError;

mod colima;

#[cfg(test)]
pub(in crate::runner) use self::colima::clear_service_container_name_cache;

const CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH: &str = "/usr/local/bin/effigy";
const CONTAINER_COLOR_ENV: [(&str, &str); 3] = [
    ("EFFIGY_COLOR", "always"),
    ("CLICOLOR_FORCE", "1"),
    ("FORCE_COLOR", "3"),
];
const CONTAINER_TTY_COLOR_ENV: [(&str, &str); 2] =
    [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

pub(super) fn build_routed_task_exec_args(
    strategy: &effigy_exec::ExecStrategy,
    task_env: Option<&BTreeMap<String, String>>,
    secret_env: Option<&[(&str, &SecretString)]>,
    workspace_identity: Option<(&str, Option<&str>)>,
    service: &str,
    mapped_cwd: &str,
) -> Vec<OsString> {
    let mut args = vec![OsString::from("exec"), OsString::from("-T")];
    append_task_exec_env(&mut args, task_env);
    append_exec_env(&mut args, secret_env);
    append_color_exec_env(&mut args, false);
    if let Some((user, home)) = workspace_identity {
        args.push(OsString::from("-u"));
        args.push(OsString::from(user));
        if let Some(home) = home {
            args.push(OsString::from("-e"));
            args.push(OsString::from(format!("HOME={home}")));
        }
    }

    match strategy {
        effigy_exec::ExecStrategy::Handoff { args: handoff_args } => {
            args.push(OsString::from("-e"));
            args.push(OsString::from(CONTAINER_HANDOFF_ENV_ASSIGNMENT));
            args.push(OsString::from("-w"));
            args.push(OsString::from(mapped_cwd));
            args.push(OsString::from(service));
            args.push(OsString::from(CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH));
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

fn append_task_exec_env(args: &mut Vec<OsString>, task_env: Option<&BTreeMap<String, String>>) {
    let Some(task_env) = task_env else {
        return;
    };
    for (key, value) in task_env {
        if crate::runner::host_scheduler::is_scheduler_owned_env(key) {
            continue;
        }
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("{key}={value}")));
    }
}

pub(in crate::runner) fn run_compose_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
) -> Result<Output, RunnerError> {
    run_compose_exec_with_deadline(repo_root, policy, args, capture, label, None)
}

pub(in crate::runner) fn run_compose_exec_with_deadline(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    run_compose_exec_with_options(repo_root, policy, args, capture, label, None, deadline)
}

pub(in crate::runner) fn run_compose_exec_with_options(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    let (program, resolved_args) = compose_invocation_for_repo(repo_root, policy, args);
    let plan = ContainerComposeInvocationPlan {
        backend_id: active_backend_id_for_policy(repo_root, policy),
        repo_root: repo_root.to_path_buf(),
        profile: policy.profile.clone(),
        action: effigy_containers::ContainerAction::Exec,
        program: OsString::from(program),
        args: resolved_args,
        label: label.to_owned(),
    };
    run_compose_exec_plan_with_deadline(policy, &plan, capture, stdin_file, deadline)
}

pub(in crate::runner) fn run_compose_exec_plan_with_options(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    run_compose_exec_plan_with_deadline(policy, plan, capture, stdin_file, None)
}

fn run_compose_exec_plan_with_deadline(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    if plan.backend_id == BackendId::colima_nerdctl() {
        return colima::run_colima_direct_exec(
            &plan.repo_root,
            policy,
            &plan.args,
            colima::ColimaDirectExecRequest {
                capture,
                label: &plan.label,
                stdin_file,
                deadline,
            },
            colima::ColimaExecAdapters {
                parse_compose_exec_args: &parse_compose_exec_args,
                run_command_capture_allow_failure: &move |root, program, args| {
                    run_command_capture_until(root, program, args, None, deadline)
                },
                run_command_capture_allow_failure_with_stdin: &move |root, program, args, stdin| {
                    run_command_capture_until(root, program, args, stdin, deadline)
                },
                format_args: &format_args,
            },
        );
    }

    if capture {
        return run_command_capture_until(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            stdin_file,
            deadline,
        );
    }

    let resolved_program = resolve_host_program(plan.program.as_os_str());
    let mut child = ProcessCommand::new(&resolved_program)
        .current_dir(&plan.repo_root)
        .args(&plan.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} ({} {})",
                plan.label,
                plan.program.to_string_lossy(),
                format_args(&plan.args)
            ),
            error,
        })?;
    let status = child
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: plan.label.clone(),
            error,
        })?;
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

fn active_backend_id_for_policy(repo_root: &Path, policy: &EffectiveContainerPolicy) -> BackendId {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => BackendId::docker_compose(),
        ComposeBackend::ColimaNerdctl => BackendId::colima_nerdctl(),
    }
}

pub(in crate::runner) fn copy_file_into_service(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    host_source: &Path,
    container_dest: &str,
) -> Result<(), RunnerError> {
    let container_id = colima::resolve_compose_service_container_id(
        repo_root,
        policy,
        service,
        None,
        &run_command_capture_allow_failure,
        &format_args,
    )?;
    let mut args = vec![OsString::from("cp")];
    args.push(OsString::from(host_source));
    args.push(OsString::from(format!(
        "{}:{}",
        container_id.to_string_lossy(),
        container_dest
    )));

    let (program, resolved_args) = copy_file_into_service_invocation(repo_root, policy, &args)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let output = run_command_capture_allow_failure(repo_root, program.as_os_str(), &resolved_args)?;
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: format!(
                "{} {}",
                program.to_string_lossy(),
                format_args(&resolved_args)
            ),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

pub(super) fn copy_file_into_service_invocation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
) -> Result<(OsString, Vec<OsString>), effigy_containers::ContainerManagerError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => BackendId::docker_compose(),
        ComposeBackend::ColimaNerdctl => BackendId::colima_nerdctl(),
    });
    ContainerManager::defaults().runtime_process_invocation(
        &detection,
        policy.profile.as_str(),
        "docker",
        args,
    )
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
    program: &OsStr,
    args: &[OsString],
) -> Result<Output, RunnerError> {
    run_command_capture_allow_failure_with_stdin(repo_root, program, args, None)
}

pub(super) fn run_command_capture_allow_failure_with_stdin(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    run_command_capture_until(repo_root, program, args, stdin_file, None)
}

pub(in crate::runner) fn run_command_capture_until(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    // Test-only deterministic seam. Production runs contain no scripted
    // runtime, so this compiles to a no-op branch and keeps the real capture,
    // argv and deadline semantics below unchanged.
    #[cfg(test)]
    if let Some(scripted) =
        crate::runner::scripted_doctor::intercept_capture(program, args, deadline)
    {
        return scripted;
    }
    let resolved_program = resolve_host_program(program);
    let command_label = format!(
        "{} {}",
        resolved_program.to_string_lossy(),
        format_args(args)
    );
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(RunnerError::task_invocation(format!(
            "{command_label} timed out"
        )));
    }
    let mut command = ProcessCommand::new(&resolved_program);
    command.current_dir(repo_root).args(args);
    if let Some(stdin_file) = stdin_file {
        let file =
            std::fs::File::open(stdin_file).map_err(|error| RunnerError::TaskCommandLaunch {
                command: command_label.clone(),
                error,
            })?;
        command.stdin(Stdio::from(file));
    } else {
        command.stdin(Stdio::null());
    }
    let Some(deadline) = deadline else {
        return command
            .output()
            .map_err(|error| RunnerError::TaskCommandLaunch {
                command: command_label,
                error,
            });
    };
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    // SAFETY: `pre_exec` runs this closure in the forked child before `exec`,
    // where only async-signal-safe work is allowed. `setpgid` is
    // async-signal-safe and runs before any image is loaded, placing the child
    // in a new process group whose id is the child's own pid; the error path
    // uses the allocation-free `io::Error::from` conversion. On deadline the
    // caller calls `effigy_process::terminate_process_tree(child.id())`, which
    // signals the negative of that same process-group id, so the TERM/KILL
    // contract covers the started child and its descendants.
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))
                .map_err(std::io::Error::from)
        });
    }
    let mut child = command
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: command_label.clone(),
            error,
        })?;
    let mut stdout = child.stdout.take().ok_or_else(|| {
        RunnerError::task_invocation(format!("{command_label} stdout unavailable"))
    })?;
    let mut stderr = child.stderr.take().ok_or_else(|| {
        RunnerError::task_invocation(format!("{command_label} stderr unavailable"))
    })?;
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| RunnerError::TaskCommandLaunch {
                command: command_label.clone(),
                error,
            })?
        {
            let stdout = stdout_reader.join().unwrap_or_default();
            let stderr = stderr_reader.join().unwrap_or_default();
            return Ok(Output {
                status,
                stdout,
                stderr,
            });
        }
        if Instant::now() >= deadline {
            effigy_process::terminate_process_tree(child.id(), false);
            let grace_deadline = Instant::now() + Duration::from_millis(500);
            loop {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                if Instant::now() >= grace_deadline {
                    effigy_process::terminate_process_tree(child.id(), true);
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(RunnerError::task_invocation(format!(
                "{command_label} timed out"
            )));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn resolve_host_program(program: impl AsRef<OsStr>) -> OsString {
    let program = program.as_ref();
    match program.to_str() {
        Some(value) if !value.contains('/') => resolve_host_cli_program(value),
        _ => program.to_os_string(),
    }
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
        if crate::runner::host_scheduler::is_scheduler_owned_env(key) {
            continue;
        }
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

fn run_command_capture_allow_failure_with_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
) -> Result<Output, RunnerError> {
    let (program, resolved_args) = compose_invocation_for_repo(repo_root, policy, args);
    run_command_capture_allow_failure(repo_root, OsStr::new(program), &resolved_args)
}

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod capture_deadline_tests {
    use super::*;

    #[test]
    fn run_command_capture_until_expired_deadline_does_not_spawn() {
        let temp = tempfile::tempdir().expect("tempdir");
        let deadline = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("deadline");
        let error = run_command_capture_until(
            temp.path(),
            OsStr::new("/definitely-not-a-binary-effigy-deadline-probe"),
            &[],
            None,
            Some(deadline),
        )
        .expect_err("expired deadline must fail closed");
        assert!(
            error.to_string().contains("timed out"),
            "expected timeout, got {error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_command_capture_until_kills_hung_child() {
        let temp = tempfile::tempdir().expect("tempdir");
        let started = Instant::now();
        let error = run_command_capture_until(
            temp.path(),
            OsStr::new("/bin/sleep"),
            &[OsString::from("30")],
            None,
            Some(Instant::now() + Duration::from_millis(250)),
        )
        .expect_err("hung child must time out");
        assert!(
            error.to_string().contains("timed out"),
            "expected timeout, got {error}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "deadline must kill the child instead of waiting out sleep 30"
        );
    }
}
