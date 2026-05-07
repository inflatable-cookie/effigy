use std::path::PathBuf;

use crate::value_parsing::next_required_value;
use crate::{
    BootstrapArgs, BootstrapBackendOverride, BootstrapDbSeedInput, BootstrapDepsSyncMode,
    BootstrapSubcommand, CliParseError, Command, HelpTopic,
};

use super::unknown_argument;

pub(super) fn parse_bootstrap_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Ok(Command::Help(HelpTopic::Bootstrap));
    };
    if matches!(first.as_str(), "--help" | "-h") {
        return Ok(Command::Help(HelpTopic::Bootstrap));
    }
    if first == "deps" {
        return parse_bootstrap_deps(args);
    }
    if first == "children" {
        return parse_bootstrap_children(args);
    }
    if first == "teardown" {
        let mut yes = false;
        let mut output_json = false;
        for arg in args.by_ref() {
            match arg.as_str() {
                "--json" => output_json = true,
                "--yes" => yes = true,
                other if other.starts_with('-') => return Err(unknown_argument(other)),
                _ => return Err(unknown_argument(arg)),
            }
        }
        return Ok(Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Teardown { yes },
            output_json,
        }));
    }

    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    let mut backend = None;
    let mut db_seeds = Vec::<BootstrapDbSeedInput>::new();
    let mut fresh = false;
    let mut no_prompt = false;
    let mut reuse_path = false;
    let mut start = true;
    let mut plan = false;
    let mut output_json = false;
    let repo_url = first;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => output_json = true,
            "--fresh" => fresh = true,
            "--no-prompt" => no_prompt = true,
            "--reuse-path" => reuse_path = true,
            "--start" => start = true,
            "--no-start" => start = false,
            "--plan" => plan = true,
            "--path" => {
                path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--path".to_owned(),
                    },
                )?));
            }
            "--branch" => {
                branch = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--branch".to_owned(),
                    },
                )?);
            }
            "--backend" => {
                backend = Some(parse_bootstrap_backend_override(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--backend".to_owned(),
                    },
                )?)?);
            }
            "--db-seed" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--db-seed".to_owned(),
                    },
                )?;
                db_seeds.push(parse_bootstrap_db_seed(value)?);
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => return Err(unknown_argument(arg)),
        }
    }

    Ok(Command::Bootstrap(BootstrapArgs {
        subcommand: BootstrapSubcommand::Clone {
            repo_url,
            path,
            branch,
            backend,
            db_seeds,
            fresh,
            no_prompt,
            reuse_path,
            start,
            plan,
        },
        output_json,
    }))
}

fn parse_bootstrap_children<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcommand) = args.next() else {
        return Ok(Command::Help(HelpTopic::Bootstrap));
    };
    match subcommand.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Bootstrap)),
        "status" => parse_bootstrap_children_status(args),
        "sync" => parse_bootstrap_children_sync(args),
        other => Err(CliParseError::UnknownArgument(other.to_owned())),
    }
}

fn parse_bootstrap_children_status<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter();
    let mut output_json = false;

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Bootstrap)),
            "--json" => output_json = true,
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => return Err(unknown_argument(arg)),
        }
    }

    Ok(Command::Bootstrap(BootstrapArgs {
        subcommand: BootstrapSubcommand::ChildrenStatus,
        output_json,
    }))
}

fn parse_bootstrap_children_sync<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter();
    let mut output_json = false;
    let mut fetch_only = false;
    let mut checkout = false;

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Bootstrap)),
            "--json" => output_json = true,
            "--fetch-only" => fetch_only = true,
            "--checkout" => checkout = true,
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => return Err(unknown_argument(arg)),
        }
    }

    Ok(Command::Bootstrap(BootstrapArgs {
        subcommand: BootstrapSubcommand::ChildrenSync {
            fetch_only,
            checkout,
        },
        output_json,
    }))
}

pub(super) fn parse_bootstrap_db_seed(
    value: String,
) -> Result<BootstrapDbSeedInput, CliParseError> {
    if let Some((target, path)) = value.split_once('=') {
        if target.is_empty() || path.is_empty() {
            return Err(CliParseError::InvalidArguments(
                "--db-seed requires <file> or <target>=<file>".to_owned(),
            ));
        }
        return Ok(BootstrapDbSeedInput {
            target: Some(target.to_owned()),
            path: PathBuf::from(path),
        });
    }

    if looks_like_bare_db_target(&value) {
        return Ok(BootstrapDbSeedInput {
            target: Some(value.clone()),
            path: PathBuf::from(format!("{value}.sql")),
        });
    }

    Ok(BootstrapDbSeedInput {
        target: None,
        path: PathBuf::from(value),
    })
}

fn parse_bootstrap_backend_override(
    value: String,
) -> Result<BootstrapBackendOverride, CliParseError> {
    match value.trim() {
        "containerd" | "colima-nerdctl" => Ok(BootstrapBackendOverride::Containerd),
        "docker" | "docker-compose" => Ok(BootstrapBackendOverride::Docker),
        other => Err(CliParseError::InvalidFlagValue {
            flag: "--backend".to_owned(),
            value: other.to_owned(),
            expected: "containerd|docker".to_owned(),
        }),
    }
}

fn looks_like_bare_db_target(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && !value.starts_with('.')
        && !value.contains('.')
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn parse_bootstrap_deps<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcommand) = args.next() else {
        return Ok(Command::Help(HelpTopic::Bootstrap));
    };
    match subcommand.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Bootstrap)),
        "sync" => parse_bootstrap_deps_sync(args),
        other => Err(CliParseError::UnknownArgument(other.to_owned())),
    }
}

fn parse_bootstrap_deps_sync<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter();
    let mut output_json = false;
    let mut mode = BootstrapDepsSyncMode::Both;
    let mut paths = Vec::<String>::new();

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Bootstrap)),
            "--json" => output_json = true,
            "--js-only" => {
                if mode == BootstrapDepsSyncMode::RustOnly {
                    return Err(CliParseError::UnknownArgument(arg.to_owned()));
                }
                mode = BootstrapDepsSyncMode::JsOnly;
            }
            "--rust-only" => {
                if mode == BootstrapDepsSyncMode::JsOnly {
                    return Err(CliParseError::UnknownArgument(arg.to_owned()));
                }
                mode = BootstrapDepsSyncMode::RustOnly;
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => paths.push(arg),
        }
    }

    if paths.is_empty() {
        paths.push(".".to_owned());
    }

    Ok(Command::Bootstrap(BootstrapArgs {
        subcommand: BootstrapSubcommand::DepsSync { mode, paths },
        output_json,
    }))
}
