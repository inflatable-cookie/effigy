//! Compose-exec argv: the one owner of the arguments Effigy builds for a
//! routed exec, an owned compose exec and a verified container target, and of
//! the parser that reads them back. Transports execute these arguments; they
//! never spell them.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::Instant;

use effigy_containers::compose::{
    resolve_compose_backend_for_repo, resolve_host_cli_program, ComposeBackend,
};
use effigy_containers::{
    BackendId, ContainerBackendDetection, ContainerComposeInvocationPlan, ContainerManager,
    EffectiveContainerPolicy,
};
use effigy_env::secret::SecretString;

use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_ASSIGNMENT;
use crate::runner::container_runtime_prep::OWNED_EXEC_VERIFICATION_TIMEOUT;
use crate::runner::error::RunnerError;
use crate::runner::task_engine::EnvOverride;

const CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH: &str = "/usr/local/bin/effigy";
const CONTAINER_COLOR_ENV: [(&str, &str); 3] = [
    ("EFFIGY_COLOR", "always"),
    ("CLICOLOR_FORCE", "1"),
    ("FORCE_COLOR", "3"),
];
const CONTAINER_TTY_COLOR_ENV: [(&str, &str); 2] =
    [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

pub(in crate::runner) fn build_routed_task_exec_args(
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
pub(in crate::runner) struct RoutedExec {
    pub(in crate::runner) argv: Vec<String>,
    pub(in crate::runner) env: Vec<(String, EnvOverride)>,
    pub(in crate::runner) user: Option<String>,
    pub(in crate::runner) workdir: String,
}

/// The same workload [`build_routed_task_exec_args`] renders for Compose.
pub(in crate::runner) fn routed_task_exec(
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

pub(in crate::runner) fn verified_native_exec_invocation(
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

pub(in crate::runner) fn copy_file_into_service_invocation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    target_id: &str,
    host_source: &Path,
    container_dest: &str,
) -> Result<
    (OsString, Vec<OsString>, Option<std::path::PathBuf>),
    effigy_containers::ContainerManagerError,
> {
    let backend = resolve_compose_backend_for_repo(repo_root, policy);
    let (args, stdin_file) = match backend {
        ComposeBackend::ColimaNerdctl => (
            [
                "exec",
                "-i",
                "--user",
                "0",
                target_id,
                "sh",
                "-c",
                "umask 077; set -C; cat > \"$1\"",
                "effigy-stage",
                container_dest,
            ]
            .map(OsString::from)
            .to_vec(),
            Some(host_source.to_path_buf()),
        ),
        ComposeBackend::Docker => (
            vec![
                "cp".into(),
                host_source.as_os_str().to_owned(),
                format!("{target_id}:{container_dest}").into(),
            ],
            None,
        ),
    };
    // Colima's cp reads VM paths, not arbitrary host paths. Stream into an
    // exclusive private staging file on the already verified exact target;
    // stdin remains inside the same caller deadline and process supervision.
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(match backend {
        ComposeBackend::Docker => BackendId::docker_compose(),
        ComposeBackend::ColimaNerdctl => BackendId::colima_nerdctl(),
    });
    ContainerManager::defaults()
        .runtime_process_invocation(&detection, policy.profile.as_str(), "docker", &args)
        .map(|(program, args)| (program, args, stdin_file))
}

pub(in crate::runner) fn parse_compose_exec_args(
    args: &[OsString],
) -> Result<ParsedComposeExec, RunnerError> {
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

pub(in crate::runner) fn resolve_host_program(program: impl AsRef<OsStr>) -> OsString {
    let program = program.as_ref();
    match program.to_str() {
        Some(value) if !value.contains('/') => resolve_host_cli_program(value),
        _ => program.to_os_string(),
    }
}

pub(in crate::runner) struct ParsedComposeExec {
    pub(in crate::runner) env: Vec<OsString>,
    pub(in crate::runner) working_dir: Option<OsString>,
    pub(in crate::runner) user: Option<OsString>,
    pub(in crate::runner) tty: bool,
    pub(in crate::runner) service: String,
    pub(in crate::runner) command: Vec<OsString>,
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

pub(in crate::runner) fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
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
