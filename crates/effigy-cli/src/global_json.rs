use std::path::PathBuf;

use crate::{unknown_argument, CliParseError, Command};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GlobalCliOptions {
    pub json_mode: bool,
    pub repo_override: Option<PathBuf>,
    pub task_verbose_root: bool,
    pub task_env_schema: Option<PathBuf>,
}

pub fn strip_global_cli_flags(
    args: Vec<String>,
) -> Result<(Vec<String>, GlobalCliOptions), CliParseError> {
    let mut stripped = Vec::with_capacity(args.len());
    let mut options = GlobalCliOptions::default();
    let mut args = args.into_iter();
    let mut parsing_leading_globals = true;

    while let Some(arg) = args.next() {
        if !parsing_leading_globals {
            stripped.push(arg);
            stripped.extend(args);
            break;
        }

        match arg.as_str() {
            "--json" => options.json_mode = true,
            "--repo" => {
                options.repo_override = Some(PathBuf::from(args.next().ok_or(
                    CliParseError::MissingFlagValue {
                        flag: "--repo".to_owned(),
                    },
                )?));
            }
            "--env-schema" => {
                options.task_env_schema = Some(PathBuf::from(args.next().ok_or(
                    CliParseError::MissingFlagValue {
                        flag: "--env-schema".to_owned(),
                    },
                )?));
            }
            "--verbose-root" => options.task_verbose_root = true,
            "--help" | "-h" => {
                parsing_leading_globals = false;
                stripped.push(arg);
            }
            "--version" => {
                parsing_leading_globals = false;
                stripped.push(arg);
            }
            "--" => {
                parsing_leading_globals = false;
                stripped.push(arg);
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => {
                parsing_leading_globals = false;
                stripped.push(arg);
            }
        }
    }

    Ok((stripped, options))
}

pub(super) fn strip_global_json_flags(args: Vec<String>) -> (Vec<String>, bool) {
    let mut stripped = Vec::with_capacity(args.len());
    let mut json_mode = false;
    let mut passthrough_mode = false;
    for arg in args {
        if arg == "--" {
            passthrough_mode = true;
            stripped.push(arg);
            continue;
        }
        if !passthrough_mode && arg == "--json" {
            json_mode = true;
            continue;
        }
        stripped.push(arg);
    }
    (stripped, json_mode)
}

pub fn apply_global_cli_options(
    mut cmd: Command,
    options: &GlobalCliOptions,
) -> Result<Command, CliParseError> {
    if let Some(repo_override) = options.repo_override.as_ref() {
        match &mut cmd {
            Command::Bundle(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Changelog(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Deploy(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Secrets(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Defer(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Exec(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::State(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::System(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Workspace(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Service(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Demo(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Graph(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Rhai(_) => return Err(unknown_argument("--repo")),
            Command::Docs(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Contracts(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Artifact(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Container(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Release(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Doctor(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Tasks(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Task(task) => {
                if !task.args.iter().any(|arg| arg == "--repo") {
                    task.args.insert(0, repo_override.display().to_string());
                    task.args.insert(0, "--repo".to_owned());
                }
            }
            Command::Version
            | Command::Bootstrap(_)
            | Command::Uninstall(_)
            | Command::Gateway(_)
            | Command::InternalGateway(_)
            | Command::InternalScriptRun(_)
            | Command::InternalContainerLeaseReaper(_)
            | Command::InternalHostProcessSupervise(_)
            | Command::InternalHostProcessStop(_) => return Err(unknown_argument("--repo")),
            Command::Help(_) => {}
        }
    }

    if options.task_verbose_root {
        match &mut cmd {
            Command::Task(task) => {
                if !task.args.iter().any(|arg| arg == "--verbose-root") {
                    task.args.insert(0, "--verbose-root".to_owned());
                }
            }
            _ => return Err(unknown_argument("--verbose-root")),
        }
    }

    if let Some(env_schema) = options.task_env_schema.as_ref() {
        match &mut cmd {
            Command::Task(task) => {
                if !task.args.iter().any(|arg| arg == "--env-schema") {
                    task.args.insert(0, env_schema.display().to_string());
                    task.args.insert(0, "--env-schema".to_owned());
                }
            }
            _ => return Err(unknown_argument("--env-schema")),
        }
    }

    if options.json_mode {
        cmd = apply_global_json_flag(cmd, true);
    }

    Ok(cmd)
}

pub(super) fn apply_global_json_flag(mut cmd: Command, json_mode: bool) -> Command {
    if !json_mode {
        return cmd;
    }

    match &mut cmd {
        Command::Version => {}
        Command::Bundle(args) => args.output_json = true,
        Command::Deploy(args) => args.output_json = true,
        Command::Secrets(args) => args.output_json = true,
        Command::Defer(args) => args.output_json = true,
        Command::Exec(args) => args.output_json = true,
        Command::State(args) => args.output_json = true,
        Command::System(args) => args.output_json = true,
        Command::Workspace(args) => args.output_json = true,
        Command::Gateway(args) => args.output_json = true,
        Command::Service(args) => args.output_json = true,
        Command::Task(task) => {
            if !task.args.iter().any(|arg| arg == "--json") {
                let insert_at =
                    if task.args.first().map(String::as_str).is_some_and(|arg| {
                        matches!(arg, "migrate" | "unlock" | "cache" | "completion")
                    }) {
                        1
                    } else {
                        0
                    };
                task.args.insert(insert_at, "--json".to_owned());
            }
        }
        Command::Changelog(args) => args.output_json = true,
        Command::Demo(args) => args.output_json = true,
        Command::Graph(args) => args.output_json = true,
        Command::Rhai(args) => args.output_json = true,
        Command::Docs(args) => args.output_json = true,
        Command::Contracts(args) => args.output_json = true,
        Command::Artifact(args) => args.output_json = true,
        Command::Container(args) => args.output_json = true,
        Command::Bootstrap(args) => args.output_json = true,
        Command::Uninstall(args) => args.output_json = true,
        Command::Release(args) => args.output_json = true,
        Command::Tasks(args) => args.output_json = true,
        Command::Doctor(args) => args.output_json = true,
        Command::InternalGateway(_) => {}
        Command::InternalScriptRun(_) => {}
        Command::InternalContainerLeaseReaper(_) => {}
        Command::InternalHostProcessSupervise(_) => {}
        Command::InternalHostProcessStop(_) => {}
        Command::Help(_) => {}
    }
    cmd
}

pub(super) fn command_requests_json(cmd: &Command, global_json_mode: bool) -> bool {
    if global_json_mode {
        return true;
    }
    match cmd {
        Command::Version => false,
        Command::Bundle(args) => args.output_json,
        Command::Deploy(args) => args.output_json,
        Command::Secrets(args) => args.output_json,
        Command::Defer(args) => args.output_json,
        Command::Exec(args) => args.output_json,
        Command::State(args) => args.output_json,
        Command::System(args) => args.output_json,
        Command::Workspace(args) => args.output_json,
        Command::Gateway(args) => args.output_json,
        Command::Service(args) => args.output_json,
        Command::Changelog(args) => args.output_json,
        Command::Demo(args) => args.output_json,
        Command::Graph(args) => args.output_json,
        Command::Rhai(args) => args.output_json,
        Command::Docs(args) => args.output_json,
        Command::Contracts(args) => args.output_json,
        Command::Artifact(args) => args.output_json,
        Command::Container(args) => args.output_json,
        Command::Bootstrap(args) => args.output_json,
        Command::Uninstall(args) => args.output_json,
        Command::Release(args) => args.output_json,
        Command::Tasks(args) => args.output_json,
        Command::Doctor(args) => args.output_json,
        Command::Task(task) => task.args.iter().any(|arg| arg == "--json"),
        Command::InternalGateway(_) => false,
        Command::InternalScriptRun(_) => false,
        Command::InternalContainerLeaseReaper(_) => false,
        Command::InternalHostProcessSupervise(_) => false,
        Command::InternalHostProcessStop(_) => false,
        Command::Help(_) => false,
    }
}
