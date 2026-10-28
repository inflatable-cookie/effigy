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
    ContainerAction, EffectiveContainerPolicy,
};
use effigy_containers::{
    BackendId, ContainerBackendDetection, ContainerComposeInvocationPlan, ContainerManager,
};
use effigy_env::secret::SecretString;
use effigy_exec::detection::{build_capabilities_from_results, standard_probe_spec, ProbeResult};

use crate::runner::container_command::{bounded_failure_error, bounded_phase_error};
use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_ASSIGNMENT;
use crate::runner::container_runtime_prep::OWNED_EXEC_VERIFICATION_TIMEOUT;
use crate::runner::error::RunnerError;
use crate::runner::exec_command::{BoundedCommandBudget, BoundedCommandPhase};
use crate::runner::task_engine::EnvOverride;

mod colima;

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
    let runtime_root = match strategy {
        effigy_exec::ExecStrategy::RawExec { working_dir, .. } => Some(working_dir.as_str()),
        effigy_exec::ExecStrategy::Handoff { .. } => None,
    };
    append_task_exec_env(&mut args, task_env, runtime_root);
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

/// A routed task's workload as remote custody runs it: argv, environment,
/// user and working directory, with no Compose grammar and no secret in argv.
pub(super) struct RoutedExec {
    pub(super) argv: Vec<String>,
    pub(super) env: Vec<(String, EnvOverride)>,
    pub(super) user: Option<String>,
    pub(super) workdir: String,
}

/// The same workload [`build_routed_task_exec_args`] renders for Compose.
pub(super) fn routed_task_exec(
    strategy: &effigy_exec::ExecStrategy,
    task_env: Option<&BTreeMap<String, String>>,
    secret_env: Option<&[(&str, &SecretString)]>,
    workspace_identity: Option<(&str, Option<&str>)>,
    mapped_cwd: &str,
    tty: bool,
) -> RoutedExec {
    let runtime_root = match strategy {
        effigy_exec::ExecStrategy::RawExec { working_dir, .. } => Some(working_dir.as_str()),
        effigy_exec::ExecStrategy::Handoff { .. } => None,
    };
    let mut env = Vec::new();
    for (key, value) in task_env.into_iter().flatten() {
        let value = runtime_root.map_or_else(
            || value.clone(),
            |root| value.replace("{repo}", root).replace("{project}", root),
        );
        env.push((key.clone(), EnvOverride::Plain(value)));
    }
    for (key, value) in secret_env.unwrap_or(&[]) {
        env.push((
            (*key).to_owned(),
            EnvOverride::Secret(value.expose().to_owned()),
        ));
    }
    let color = CONTAINER_COLOR_ENV.iter().chain(
        tty.then_some(&CONTAINER_TTY_COLOR_ENV)
            .into_iter()
            .flatten(),
    );
    for (key, value) in color {
        env.push(((*key).to_owned(), EnvOverride::Plain((*value).to_owned())));
    }
    let user = workspace_identity.map(|(user, home)| {
        if let Some(home) = home {
            env.push(("HOME".to_owned(), EnvOverride::Plain(home.to_owned())));
        }
        user.to_owned()
    });
    match strategy {
        effigy_exec::ExecStrategy::Handoff { args } => {
            if let Some((key, value)) = CONTAINER_HANDOFF_ENV_ASSIGNMENT.split_once('=') {
                env.push((key.to_owned(), EnvOverride::Plain(value.to_owned())));
            }
            RoutedExec {
                argv: std::iter::once(CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH.to_owned())
                    .chain(args.iter().cloned())
                    .collect(),
                env,
                user,
                workdir: mapped_cwd.to_owned(),
            }
        }
        effigy_exec::ExecStrategy::RawExec {
            working_dir,
            command,
        } => RoutedExec {
            argv: command.clone(),
            env,
            user,
            workdir: working_dir.clone(),
        },
    }
}

fn append_task_exec_env(
    args: &mut Vec<OsString>,
    task_env: Option<&BTreeMap<String, String>>,
    runtime_root: Option<&str>,
) {
    let Some(task_env) = task_env else {
        return;
    };
    for (key, value) in task_env {
        let value = runtime_root.map_or_else(
            || value.clone(),
            |root| value.replace("{repo}", root).replace("{project}", root),
        );
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("{key}={value}")));
    }
}

/// Backend-specific runtime discovery belongs to this transport adapter. The
/// caller's existing bounded budget owns every capture and failure observation.
pub(in crate::runner) fn runtime_backend_is_running_bounded(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    budget: &mut BoundedCommandBudget,
) -> Result<bool, RunnerError> {
    if effigy_containers::compose::resolve_compose_backend_for_repo(repo_root, policy)
        == effigy_containers::compose::ComposeBackend::ColimaNerdctl
    {
        let command = effigy_containers::colima::colima_status_command(policy);
        let args = command
            .args
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>();
        let capture = budget
            .run_command(
                repo_root,
                std::ffi::OsStr::new(&command.program),
                &args,
                None,
                &[],
                BoundedCommandPhase::RuntimeDiscovery,
            )
            .map_err(|failure| {
                bounded_failure_error(policy, &policy.primary_service, budget, failure)
            })?;
        if !capture.status.success() {
            return Ok(false);
        }
        return Ok(effigy_containers::colima::parse_colima_running(
            &String::from_utf8_lossy(&capture.stdout),
            &String::from_utf8_lossy(&capture.stderr),
        ));
    }

    let args = [OsString::from("ps")];
    let plan = effigy_runtime::container_manager::runtime_invocation_plan(
        repo_root,
        policy,
        "docker",
        &args,
        ContainerAction::Status,
        "runtime ps",
    )
    .map_err(|_| {
        bounded_phase_error(
            policy,
            &policy.primary_service,
            budget,
            BoundedCommandPhase::RuntimeDiscovery,
            "runtime_discovery_failed",
            "bounded container command could not resolve runtime discovery",
            None,
        )
    })?;
    let capture = budget
        .run_command(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            None,
            &[],
            BoundedCommandPhase::RuntimeDiscovery,
        )
        .map_err(|failure| {
            bounded_failure_error(policy, &policy.primary_service, budget, failure)
        })?;
    Ok(capture.status.success())
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
    run_owned_compose_exec_plan_with_deadline(policy, &plan, capture, stdin_file, deadline)
}

pub(in crate::runner) fn run_compose_exec_plan_with_options(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    run_owned_compose_exec_plan_with_deadline(policy, plan, capture, stdin_file, None)
}

pub(in crate::runner) fn run_managed_compose_exec_plan(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
) -> Result<Output, RunnerError> {
    let parsed = parse_compose_exec_args(&plan.args)?;
    let binding = crate::runner::container_runtime_prep::verify_service_binding(
        &plan.repo_root,
        policy,
        &parsed.service,
        None,
    )?;
    let names = std::env::var(crate::runner::managed_secrets::NAMES)
        .ok()
        .map(|names| {
            names
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !names.is_empty() {
        // The managed engine supplied this private bootstrap. Keep all the
        // existing binding/reverification, user, cwd and command selection.
        let (program, mut args) =
            verified_native_exec_invocation(policy, plan, &binding, !capture, None)?;
        let exec = args
            .iter()
            .position(|a| a == "exec")
            .ok_or_else(|| RunnerError::task_invocation("missing managed native exec"))?;
        for name in names.iter().rev() {
            args.splice(
                exec + 1..exec + 1,
                [OsString::from("-e"), OsString::from(name)],
            );
        }
        let command = crate::runner::managed_secrets::relay(&program, &args, &names);
        let relay = ContainerComposeInvocationPlan {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), command.into()],
            label: "managed container exec".into(),
            ..plan.clone()
        };
        return run_exec_process_plan(&relay, capture, None, None);
    }
    run_verified_compose_exec_plan(policy, plan, &binding, capture, None, None)
}

fn run_owned_compose_exec_plan_with_deadline(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    let parsed = parse_compose_exec_args(&plan.args)?;
    let binding = crate::runner::container_runtime_prep::verify_service_binding(
        &plan.repo_root,
        policy,
        &parsed.service,
        deadline,
    )?;
    run_verified_compose_exec_plan(policy, plan, &binding, capture, stdin_file, deadline)
}

/// Readiness and the subsequent effect share an inspected full ID. Native exec
/// targets that ID, never a mutable service/name or the Colima name cache.
pub(in crate::runner) fn run_verified_compose_exec_plan(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
    capture: bool,
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    let parsed = parse_compose_exec_args(&plan.args)?;
    if binding.service() != Some(parsed.service.as_str()) {
        return Err(RunnerError::task_invocation(
            "verified exec service changed",
        ));
    }
    if plan.backend_id == BackendId::colima_nerdctl() {
        binding
            .reverify(
                &plan.repo_root,
                policy,
                deadline.unwrap_or_else(|| Instant::now() + OWNED_EXEC_VERIFICATION_TIMEOUT),
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        return colima::run_colima_direct_exec(
            &plan.repo_root,
            policy,
            &plan.args,
            colima::ColimaDirectExecRequest {
                capture,
                label: &plan.label,
                stdin_file,
            },
            OsStr::new(binding.id()),
            colima::ColimaExecAdapters {
                parse_compose_exec_args: &parse_compose_exec_args,
                run_command_capture_allow_failure_with_stdin: &move |root, program, args, stdin| {
                    run_command_capture_until(root, program, args, stdin, deadline)
                },
                format_args: &format_args,
            },
        );
    }
    let (program, args) = verified_native_exec_invocation(
        policy,
        plan,
        binding,
        stdin_file.is_some() || !capture,
        deadline,
    )?;
    let plan = ContainerComposeInvocationPlan {
        program,
        args,
        ..plan.clone()
    };

    run_exec_process_plan(&plan, capture, stdin_file, deadline)
}

/// Existing deadline-aware diagnosis/permission transport. It does not acquire
/// workload readiness or restart authority. Workload entry points above use the
/// verified binding path; diagnosis keeps its original bounded observation cost.
pub(in crate::runner) fn run_compose_exec_plan_with_deadline(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    if plan.backend_id == BackendId::colima_nerdctl() {
        let parsed = parse_compose_exec_args(&plan.args)?;
        let container_id = colima::resolve_compose_service_container_id(
            &plan.repo_root,
            policy,
            &parsed.service,
            deadline,
            &move |root, program, args| {
                run_command_capture_until(root, program, args, None, deadline)
            },
            &format_args,
        )?;
        return colima::run_colima_direct_exec(
            &plan.repo_root,
            policy,
            &plan.args,
            colima::ColimaDirectExecRequest {
                capture,
                label: &plan.label,
                stdin_file,
            },
            &container_id,
            colima::ColimaExecAdapters {
                parse_compose_exec_args: &parse_compose_exec_args,
                run_command_capture_allow_failure_with_stdin: &move |root, program, args, stdin| {
                    run_command_capture_until(root, program, args, stdin, deadline)
                },
                format_args: &format_args,
            },
        );
    }
    run_exec_process_plan(plan, capture, stdin_file, deadline)
}

fn run_exec_process_plan(
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
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

pub(super) fn verified_native_exec_invocation(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
    attach_stdin: bool,
    deadline: Option<Instant>,
) -> Result<(OsString, Vec<OsString>), RunnerError> {
    let parsed = parse_compose_exec_args(&plan.args)?;
    if binding.service() != Some(parsed.service.as_str()) {
        return Err(RunnerError::task_invocation(
            "verified exec service changed",
        ));
    }
    binding.reverify(
        &plan.repo_root,
        policy,
        deadline.unwrap_or_else(|| Instant::now() + OWNED_EXEC_VERIFICATION_TIMEOUT),
    )?;
    let mut args = vec![OsString::from("exec")];
    if attach_stdin {
        args.push(OsString::from("-i"));
    }
    if parsed.tty {
        args.push(OsString::from("-t"));
    }
    if let Some(dir) = parsed.working_dir {
        args.extend([OsString::from("-w"), dir]);
    }
    if let Some(user) = parsed.user {
        args.extend([OsString::from("-u"), user]);
    }
    for env in parsed.env {
        args.extend([OsString::from("-e"), env]);
    }
    args.push(OsString::from(binding.id()));
    args.extend(parsed.command);
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(plan.backend_id.clone());
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, &plan.profile, "docker", &args)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok((program, args))
}

/// Runtime exec arguments that target an already-verified container ID.
///
/// Bounded workspace preparation builds Compose `exec` tail arguments with the
/// service alias (its own discovery already proved which container owns the
/// service). This converts them for the runtime: the alias becomes
/// `target_id`, Compose's `-T` is dropped and the remaining options and
/// command are preserved, so every effect targets the verified container
/// instead of a mutable service name.
pub(in crate::runner) fn verified_target_exec_args(
    plan: &ContainerComposeInvocationPlan,
    target_id: &str,
) -> Result<Vec<OsString>, RunnerError> {
    let parsed = parse_compose_exec_args(&plan.args)?;
    let mut args = vec![OsString::from("exec")];
    if parsed.tty {
        args.push(OsString::from("-t"));
    }
    if let Some(dir) = parsed.working_dir {
        args.extend([OsString::from("-w"), dir]);
    }
    if let Some(user) = parsed.user {
        args.extend([OsString::from("-u"), user]);
    }
    for env in parsed.env {
        args.extend([OsString::from("-e"), env]);
    }
    args.push(OsString::from(target_id));
    args.extend(parsed.command);
    Ok(args)
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

/// Copy one host file into the verified container `target_id` inside the
/// bounded budget. The caller proved ownership of `target_id`; resolving the
/// service again here could select a different container.
pub(in crate::runner) fn copy_file_into_service_bounded(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    target_id: &str,
    host_source: &Path,
    container_dest: &str,
    budget: &mut BoundedCommandBudget,
) -> Result<(), RunnerError> {
    let container_id = target_id;
    let copy_args = [
        OsString::from("cp"),
        host_source.as_os_str().to_os_string(),
        OsString::from(format!("{container_id}:{container_dest}")),
    ];
    let (program, args) = copy_file_into_service_invocation(repo_root, policy, &copy_args)
        .map_err(|_| {
            bounded_phase_error(
                policy,
                service,
                budget,
                BoundedCommandPhase::WorkspacePreparation,
                "workspace_preparation_failed",
                "bounded container command could not resolve workspace artifact copy",
                None,
            )
        })?;
    let copy_capture = budget
        .run_command(
            repo_root,
            program.as_os_str(),
            &args,
            None,
            &[],
            BoundedCommandPhase::WorkspacePreparation,
        )
        .map_err(|failure| bounded_failure_error(policy, service, budget, failure))?;
    if !copy_capture.status.success() {
        return Err(bounded_phase_error(
            policy,
            service,
            budget,
            BoundedCommandPhase::WorkspacePreparation,
            "workspace_preparation_failed",
            "bounded container command could not copy the workspace artifact",
            copy_capture.status.code(),
        ));
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
                "-T" | "--tty=false" => {
                    tty = false;
                    continue;
                }
                "-w" | "--workdir" => {
                    working_dir = Some(iter.next().cloned().ok_or_else(|| {
                        RunnerError::task_invocation("missing exec working directory")
                    })?);
                    continue;
                }
                "-u" | "--user" => {
                    user = Some(
                        iter.next()
                            .cloned()
                            .ok_or_else(|| RunnerError::task_invocation("missing exec user"))?,
                    );
                    continue;
                }
                "-e" | "--env" => {
                    env.push(
                        iter.next().cloned().ok_or_else(|| {
                            RunnerError::task_invocation("missing exec env value")
                        })?,
                    );
                    continue;
                }
                _ if value.starts_with('-') => {
                    return Err(RunnerError::task_invocation(
                        "unsupported verified exec option",
                    ));
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
    capture_command_until(repo_root, program, args, stdin_file, deadline).map_err(|error| {
        match error {
            CommandCaptureError::Deadline { command } => {
                RunnerError::task_invocation(format!("{command} timed out"))
            }
            CommandCaptureError::Runner(error) => error,
        }
    })
}

pub(in crate::runner) enum CommandCaptureError {
    Deadline { command: String },
    Runner(RunnerError),
}

impl From<RunnerError> for CommandCaptureError {
    fn from(error: RunnerError) -> Self {
        Self::Runner(error)
    }
}

pub(in crate::runner) fn capture_command_until(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, CommandCaptureError> {
    // Test-only deterministic seam. Production runs contain no scripted
    // runtime, so this compiles to a no-op branch and keeps the real capture,
    // argv and deadline semantics below unchanged.
    #[cfg(test)]
    if let Some(scripted) =
        crate::runner::scripted_doctor::intercept_capture(program, args, deadline)
    {
        return scripted.map_err(CommandCaptureError::Runner);
    }
    let resolved_program = resolve_host_program(program);
    let command_label = format!(
        "{} {}",
        resolved_program.to_string_lossy(),
        format_args(args)
    );
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(CommandCaptureError::Deadline {
            command: command_label,
        });
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
        return command.output().map_err(|error| {
            CommandCaptureError::Runner(RunnerError::TaskCommandLaunch {
                command: command_label,
                error,
            })
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
    let Some(mut stdout) = child.stdout.take() else {
        super::bounded::terminate_owned_group(&mut child);
        return Err(
            RunnerError::task_invocation(format!("{command_label} stdout unavailable")).into(),
        );
    };
    let Some(mut stderr) = child.stderr.take() else {
        super::bounded::terminate_owned_group(&mut child);
        return Err(
            RunnerError::task_invocation(format!("{command_label} stderr unavailable")).into(),
        );
    };
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    loop {
        if Instant::now() >= deadline {
            super::bounded::terminate_owned_group(&mut child);
            // An escaped, unowned writer must not defeat the caller deadline.
            // Drop unfinished handles; never join a reader without observing EOF.
            return Err(CommandCaptureError::Deadline {
                command: command_label,
            });
        }
        if stdout_reader.is_finished() && stderr_reader.is_finished() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let stdout = stdout_reader.join().ok().and_then(Result::ok);
                    let stderr = stderr_reader.join().ok().and_then(Result::ok);
                    let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
                        return Err(RunnerError::task_invocation(format!(
                            "{command_label} output unavailable"
                        ))
                        .into());
                    };
                    return Ok(Output {
                        status,
                        stdout,
                        stderr,
                    });
                }
                Ok(None) => {}
                Err(error) => {
                    super::bounded::terminate_owned_group(&mut child);
                    return Err(RunnerError::TaskCommandLaunch {
                        command: command_label,
                        error,
                    }
                    .into());
                }
            }
        }
        // Retain the leader until both readers finish. A reaped leader cannot
        // anchor cancellation of an inherited writer's original process group.
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
        let output =
            run_compose_exec(repo_root, policy, &args, true, "container capability probe")?;
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

    #[cfg(unix)]
    #[test]
    fn legacy_deadline_bounds_inherited_writer_final_drain() {
        let temp = tempfile::tempdir().expect("owned legacy drain fixture");
        let marker = temp.path().join("writer-survived");
        let script = format!(
            "(/bin/sleep 2; printf survived > '{}') & exit 0",
            marker.display()
        );
        let started = Instant::now();
        let error = run_command_capture_until(
            temp.path(),
            OsStr::new("/bin/sh"),
            &[OsString::from("-c"), OsString::from(script)],
            None,
            Some(Instant::now() + Duration::from_millis(300)),
        )
        .expect_err("deadline covers final drain after leader exit");
        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
        thread::sleep(Duration::from_secs(2));
        assert!(!marker.exists(), "legacy writer survived cancellation");
    }
}
