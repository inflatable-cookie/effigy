use std::path::PathBuf;

use crate::{
    BoundedExecLimits, Command, ContainerArgs, ContainerCacheSubcommand,
    ContainerProfileSubcommand, ContainerSubcommand, ContainerVolumeSubcommand, HelpTopic,
};

use crate::value_parsing::{next_required_value, parse_repo_path};

use super::container_data::parse_container_data;
use super::{unknown_argument, CliParseError};

const CONTAINER_EJECT_RETIRED_ERROR: &str = "`effigy container eject` is retired in v0.15 with `compose_file`: generated Compose stays internal. Run `effigy container render <NAME>` to read the exact render, and customise a service with a project catalog fragment under `infra/dev/catalog/`";

const NAMED_CONTAINER_RETIRE_ERROR: &str = "`effigy container <NAME> retire` is unsupported; use `effigy container retire` for the checkout or `effigy container retire --scope <TOKEN>` for one recorded scope";

pub(super) fn parse_container_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    const ACTIONS: [&str; 17] = [
        "up", "down", "status", "stats", "logs", "shell", "reset", "cache", "volume", "data",
        "profile", "eject", "hosts", "retire", "scope", "render", "migrate",
    ];

    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Ok(Command::Help(HelpTopic::Container));
    };

    if matches!(first.as_str(), "--help" | "-h") {
        return Ok(Command::Help(HelpTopic::Container));
    }

    let (name, action) = if ACTIONS.contains(&first.as_str()) {
        (None, first)
    } else {
        let Some(action) = args.next() else {
            return Err(unknown_argument(first));
        };
        (Some(first), action)
    };

    match action.as_str() {
        "render" => parse_container_render(name, args, |name| ContainerSubcommand::Render { name }),
        "migrate" => {
            parse_container_render(name, args, |name| ContainerSubcommand::Migrate { name })
        }
        "up" => parse_container_up(name, args),
        "down" => parse_container_down(name, args),
        "status" => parse_container_status(name, args),
        "stats" => parse_container_stats(name, args),
        "logs" => parse_container_logs(name, args),
        "shell" => parse_container_shell(name, args),
        "reset" => parse_container_reset(name, args),
        "cache" => parse_container_cache(name, args),
        "volume" => parse_container_volume(name, args),
        "profile" => parse_container_profile(name, args),
        "data" => parse_container_data(name, args),
        "eject" => Err(CliParseError::InvalidArguments(
            CONTAINER_EJECT_RETIRED_ERROR.to_owned(),
        )),
        "scope" => parse_container_scope(name, args),
        "hosts" => parse_container_hosts(name, args),
        "retire" => parse_container_retire(name, args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_container_up<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut attach = false;
    let mut detach = false;
    let mut system: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--attach" => attach = true,
            "--detach" => detach = true,
            "--system" => system = Some(parse_system_selector(&mut args, name.as_deref())?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Up {
            name,
            system,
            attach,
            detach,
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_down<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut global = false;
    let mut system: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--global" => global = true,
            "--system" => system = Some(parse_system_selector(&mut args, name.as_deref())?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if global && name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> down` does not accept `--global`; use `effigy container down --global` for cross-project shutdown".to_owned(),
        ));
    }
    if global && system.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container down --global` does not accept `--system`; it stops every project, not one system".to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Down {
            name,
            system,
            global,
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_status<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut global = false;
    let mut system: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--global" => global = true,
            "--system" => system = Some(parse_system_selector(&mut args, name.as_deref())?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if global && name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> status` does not accept `--global`; use `effigy container status --global` for cross-project discovery".to_owned(),
        ));
    }
    if global && system.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container status --global` does not accept `--system`; it discovers every project, not one system".to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Status {
            name,
            system,
            global,
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_reset<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut keep_data = false;
    let mut wipe_data = false;
    let mut yes = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--keep-data" => keep_data = true,
            "--wipe-data" => wipe_data = true,
            "--yes" => yes = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }
    if keep_data && wipe_data {
        return Err(CliParseError::InvalidArguments(
            "`effigy container reset` does not accept both `--keep-data` and `--wipe-data`"
                .to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Reset {
            name,
            keep_data,
            wipe_data,
            yes,
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_stats<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut global = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--global" => global = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> stats` is not supported yet; use `effigy container stats --global` for the bounded cross-project resource view".to_owned(),
        ));
    }
    if !global {
        return Err(CliParseError::InvalidArguments(
            "`effigy container stats` currently requires `--global`; use `effigy container stats --global` for cross-project resource discovery".to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Stats { global },
        repo_override,
        output_json,
    }))
}

/// `render` and `migrate` share one grammar: an optional name, `--repo`, `--json`.
fn parse_container_render<I>(
    mut name: Option<String>,
    args: I,
    subcommand: impl FnOnce(Option<String>) -> ContainerSubcommand,
) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other if !other.starts_with('-') && name.is_none() => name = Some(arg),
            _ => return Err(unknown_argument(arg)),
        }
    }
    Ok(Command::Container(ContainerArgs {
        subcommand: subcommand(name),
        repo_override,
        output_json,
    }))
}

fn parse_container_hosts<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    parse_named_container_simple_subcommand(name, args, |name| ContainerSubcommand::Hosts { name })
}

fn parse_container_scope<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    if name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> scope` is not supported; use `effigy container scope` for checkout identity".to_owned(),
        ));
    }

    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Scope,
        repo_override,
        output_json,
    }))
}

fn parse_container_retire<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut yes = false;
    let mut dead = false;
    let mut scope: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--yes" => yes = true,
            "--dead" => dead = true,
            "--scope" => {
                scope = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--scope".to_owned(),
                    },
                )?)
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if name.is_some() {
        return Err(CliParseError::InvalidArguments(
            NAMED_CONTAINER_RETIRE_ERROR.to_owned(),
        ));
    }

    if dead && scope.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container retire --dead` discovers scopes of deleted checkouts on the selected backend; it cannot be combined with `--scope`".to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Retire {
            name,
            scope,
            dead,
            yes,
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_cache<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Err(CliParseError::InvalidArguments(
            "`effigy container cache` requires a subcommand; use `list` or `prune`".to_owned(),
        ));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Container)),
        "list" => parse_container_cache_list(name, args),
        "prune" => parse_container_cache_prune(name, args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_container_profile<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    if name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> profile` is not supported; use `effigy container profile status` for Colima profile sizing, or `effigy container profile repair --system <NAME>` for runtime recovery".to_owned(),
        ));
    }

    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Err(CliParseError::InvalidArguments(
            "`effigy container profile` requires a subcommand; use `status`, `resize`, `purge`, `recreate`, `repair`, `reset-runtime`, or `prune-health-pid-files`"
                .to_owned(),
        ));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Container)),
        "status" => parse_container_profile_status(args),
        "resize" => parse_container_profile_resize(args),
        "purge" => parse_container_profile_purge(args),
        "recreate" => parse_container_profile_recreate(args),
        "repair" => parse_container_profile_runtime_recovery(args, false),
        "reset-runtime" => parse_container_profile_runtime_recovery(args, true),
        "prune-health-pid-files" => parse_container_profile_prune_health_pid_files(args),
        other => Err(unknown_argument(other)),
    }
}

/// `repair` and `reset-runtime` keep the repository and system selection the
/// retired `effigy system` recovery verbs accepted.
fn parse_container_profile_runtime_recovery<I>(
    args: I,
    reset_runtime: bool,
) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut system: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--system" => system = Some(parse_system_selector(&mut args, None)?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    let subcommand = if reset_runtime {
        ContainerProfileSubcommand::ResetRuntime { system }
    } else {
        ContainerProfileSubcommand::Repair { system }
    };

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Profile { subcommand },
        repo_override,
        output_json,
    }))
}

/// `prune-health-pid-files` is machine-level runtime maintenance: it takes a
/// profile and never a repository.
fn parse_container_profile_prune_health_pid_files<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut output_json = false;
    let mut profile: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => output_json = true,
            "--profile" => {
                profile = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--profile".to_owned(),
                    },
                )?)
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Profile {
            subcommand: ContainerProfileSubcommand::PruneHealthPidFiles { profile },
        },
        repo_override: None,
        output_json,
    }))
}

/// `--system <NAME>` selects an environment through `[systems.<NAME>]` instead
/// of naming its container. A directly named container cannot be combined with
/// it.
fn parse_system_selector<I>(
    args: &mut I,
    named_container: Option<&str>,
) -> Result<String, CliParseError>
where
    I: Iterator<Item = String>,
{
    if let Some(name) = named_container {
        return Err(CliParseError::InvalidArguments(format!(
            "`effigy container {name} ...` does not accept `--system`; the container is already named"
        )));
    }
    next_required_value(
        args,
        CliParseError::MissingFlagValue {
            flag: "--system".to_owned(),
        },
    )
}

fn parse_container_volume<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    if name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> volume` is not supported; use `effigy container volume list` for global Effigy-managed volume inventory".to_owned(),
        ));
    }

    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Err(CliParseError::InvalidArguments(
            "`effigy container volume` requires a subcommand; use `list`".to_owned(),
        ));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Container)),
        "list" => parse_container_volume_list(args),
        "prune" => parse_container_volume_prune(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_container_profile_status<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut profile: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--profile" => {
                profile = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--profile".to_owned(),
                    },
                )?)
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Profile {
            subcommand: ContainerProfileSubcommand::Status { profile },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_profile_recreate<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut profile: Option<String> = None;
    let mut disk_gib: Option<u64> = None;
    let mut yes = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--yes" => yes = true,
            "--disk" => disk_gib = Some(parse_positive_u64(&mut args, "--disk")?),
            "--profile" => {
                profile = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--profile".to_owned(),
                    },
                )?)
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Profile {
            subcommand: ContainerProfileSubcommand::Recreate {
                profile,
                disk_gib,
                yes,
            },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_profile_purge<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut profile: Option<String> = None;
    let mut yes = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--yes" => yes = true,
            "--profile" => {
                profile = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--profile".to_owned(),
                    },
                )?)
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Profile {
            subcommand: ContainerProfileSubcommand::Purge { profile, yes },
        },
        repo_override,
        output_json,
    }))
}

fn parse_positive_u64<I>(args: &mut I, flag: &str) -> Result<u64, CliParseError>
where
    I: Iterator<Item = String>,
{
    let raw = next_required_value(
        args,
        CliParseError::MissingFlagValue {
            flag: flag.to_owned(),
        },
    )?;
    let value = raw.parse::<u64>().map_err(|_| {
        CliParseError::InvalidArguments(format!("`{flag}` must be a positive integer, got `{raw}`"))
    })?;
    if value == 0 {
        return Err(CliParseError::InvalidArguments(format!(
            "`{flag}` must be a positive integer, got `0`"
        )));
    }
    Ok(value)
}

fn parse_container_profile_resize<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut profile: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--profile" => {
                profile = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--profile".to_owned(),
                    },
                )?)
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Profile {
            subcommand: ContainerProfileSubcommand::Resize { profile },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_volume_list<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut global = false;
    let mut orphans = false;
    let mut dormant = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--global" => global = true,
            "--orphans" => orphans = true,
            "--dormant" => dormant = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if orphans && !global {
        return Err(CliParseError::InvalidArguments(
            "`effigy container volume list --orphans` requires `--global`; use `--dormant` for repo-scoped stale volumes"
                .to_owned(),
        ));
    }
    if dormant && global {
        return Err(CliParseError::InvalidArguments(
            "`effigy container volume list --dormant` does not accept `--global`; use `--orphans` for global ownerless volumes"
                .to_owned(),
        ));
    }
    if dormant && orphans {
        return Err(CliParseError::InvalidArguments(
            "`effigy container volume list` does not accept both `--orphans` and `--dormant`"
                .to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Volume {
            subcommand: ContainerVolumeSubcommand::List {
                global,
                orphans,
                dormant,
            },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_volume_prune<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut global = false;
    let mut yes = false;
    let mut orphans = false;
    let mut dormant = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--global" => global = true,
            "--yes" => yes = true,
            "--orphans" => orphans = true,
            "--dormant" => dormant = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if orphans && !global {
        return Err(CliParseError::InvalidArguments(
            "`effigy container volume prune --orphans` requires `--global`; use `--dormant` for repo-scoped stale volumes"
                .to_owned(),
        ));
    }
    if dormant && global {
        return Err(CliParseError::InvalidArguments(
            "`effigy container volume prune --dormant` does not accept `--global`; use `--orphans` for global ownerless volumes"
                .to_owned(),
        ));
    }
    if dormant && orphans {
        return Err(CliParseError::InvalidArguments(
            "`effigy container volume prune` does not accept both `--orphans` and `--dormant`"
                .to_owned(),
        ));
    }
    if !orphans && !dormant {
        return Err(CliParseError::InvalidArguments(
            "`effigy container volume prune` requires either `--dormant` or `--global --orphans`"
                .to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Volume {
            subcommand: ContainerVolumeSubcommand::Prune {
                global,
                yes,
                orphans,
                dormant,
            },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_cache_list<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut global = false;
    let mut project: Option<String> = None;
    let mut kind: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--global" => global = true,
            "--project" => {
                project = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--project".to_owned(),
                    },
                )?)
            }
            "--kind" => {
                kind = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--kind".to_owned(),
                    },
                )?)
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if project.is_some() || kind.is_some() {
        global = true;
    }

    if global && name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> cache list` does not accept `--global`; use `effigy container cache list --global` for cross-project cache discovery".to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Cache {
            name,
            subcommand: ContainerCacheSubcommand::List {
                global,
                project,
                kind,
            },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_cache_prune<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut global = false;
    let mut yes = false;
    let mut project: Option<String> = None;
    let mut kind: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--global" => global = true,
            "--yes" => yes = true,
            "--project" => {
                project = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--project".to_owned(),
                    },
                )?)
            }
            "--kind" => {
                kind = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--kind".to_owned(),
                    },
                )?)
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if project.is_some() || kind.is_some() {
        global = true;
    }

    if global && name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> cache prune` does not accept `--global`; use `effigy container cache prune --global` for profile-wide cache cleanup".to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Cache {
            name,
            subcommand: ContainerCacheSubcommand::Prune {
                global,
                yes,
                project,
                kind,
            },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_logs<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut service: Option<String> = None;
    let mut system: Option<String> = None;
    let mut follow = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--system" => system = Some(parse_system_selector(&mut args, name.as_deref())?),
            "--service" => {
                service = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--service".to_owned(),
                    },
                )?);
            }
            "--follow" => follow = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Logs {
            name,
            system,
            service,
            follow,
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_shell<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut service: Option<String> = None;
    let mut command: Option<String> = None;
    let mut bounded = false;
    let mut deadline_ms: Option<u64> = None;
    let mut stdout_limit_bytes: Option<u64> = None;
    let mut stderr_limit_bytes: Option<u64> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--bounded" => bounded = true,
            "--service" => {
                service = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--service".to_owned(),
                    },
                )?);
            }
            "--command" => {
                command = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--command".to_owned(),
                    },
                )?);
            }
            "--deadline-ms" => deadline_ms = Some(parse_bounded_u64(&mut args, "--deadline-ms")?),
            "--stdout-limit-bytes" => {
                stdout_limit_bytes = Some(parse_bounded_u64(&mut args, "--stdout-limit-bytes")?);
            }
            "--stderr-limit-bytes" => {
                stderr_limit_bytes = Some(parse_bounded_u64(&mut args, "--stderr-limit-bytes")?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    let limits = if bounded {
        if command.is_none() {
            return Err(CliParseError::InvalidArguments(
                "`effigy container <NAME> shell --bounded` requires `--command <CMD>`".to_owned(),
            ));
        }
        let limits = BoundedExecLimits {
            deadline_ms: deadline_ms.ok_or_else(|| {
                CliParseError::InvalidArguments(
                    "`--bounded` requires `--deadline-ms <MS>`".to_owned(),
                )
            })?,
            stdout_limit_bytes: stdout_limit_bytes.ok_or_else(|| {
                CliParseError::InvalidArguments(
                    "`--bounded` requires `--stdout-limit-bytes <N>`".to_owned(),
                )
            })?,
            stderr_limit_bytes: stderr_limit_bytes.ok_or_else(|| {
                CliParseError::InvalidArguments(
                    "`--bounded` requires `--stderr-limit-bytes <N>`".to_owned(),
                )
            })?,
        };
        limits.validate().map_err(CliParseError::InvalidArguments)?;
        Some(limits)
    } else {
        if deadline_ms.is_some() || stdout_limit_bytes.is_some() || stderr_limit_bytes.is_some() {
            return Err(CliParseError::InvalidArguments(
                "`--deadline-ms`, `--stdout-limit-bytes` and `--stderr-limit-bytes` require `--bounded`"
                    .to_owned(),
            ));
        }
        None
    };

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Shell {
            name,
            service,
            command,
            bounded: limits,
        },
        repo_override,
        output_json,
    }))
}

fn parse_bounded_u64<I>(args: &mut I, flag: &str) -> Result<u64, CliParseError>
where
    I: Iterator<Item = String>,
{
    let raw = next_required_value(
        args,
        CliParseError::MissingFlagValue {
            flag: flag.to_owned(),
        },
    )?;
    raw.parse::<u64>()
        .map_err(|_| CliParseError::InvalidFlagValue {
            flag: flag.to_owned(),
            value: raw,
            expected: "a positive integer".to_owned(),
        })
}

fn parse_named_container_simple_subcommand<I, F>(
    name: Option<String>,
    args: I,
    build: F,
) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
    F: FnOnce(Option<String>) -> ContainerSubcommand,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: build(name),
        repo_override,
        output_json,
    }))
}

#[cfg(test)]
mod tests {
    use crate::{parse_command, Command, ContainerSubcommand};

    fn shell(args: &[&str]) -> Result<ContainerSubcommand, crate::CliParseError> {
        let mut argv = vec!["container", "web", "shell"];
        argv.extend_from_slice(args);
        match parse_command(argv.into_iter().map(str::to_owned))? {
            Command::Container(container) => Ok(container.subcommand),
            other => panic!("unexpected command {other:?}"),
        }
    }

    fn bounded(args: &[&str]) -> Result<crate::BoundedExecLimits, crate::CliParseError> {
        match shell(args)? {
            ContainerSubcommand::Shell {
                bounded: Some(limits),
                ..
            } => Ok(limits),
            other => panic!("expected bounded shell, got {other:?}"),
        }
    }

    #[test]
    fn bounded_shell_parses_explicit_limits() {
        let limits = bounded(&[
            "--command",
            "true",
            "--bounded",
            "--deadline-ms",
            "1500",
            "--stdout-limit-bytes",
            "2048",
            "--stderr-limit-bytes",
            "4096",
        ])
        .expect("bounded limits parse");
        assert_eq!(limits.deadline_ms, 1500);
        assert_eq!(limits.stdout_limit_bytes, 2048);
        assert_eq!(limits.stderr_limit_bytes, 4096);
    }

    #[test]
    fn bounded_shell_requires_a_command_and_every_bound() {
        assert!(shell(&[
            "--bounded",
            "--deadline-ms",
            "1",
            "--stdout-limit-bytes",
            "1",
            "--stderr-limit-bytes",
            "1"
        ])
        .is_err());
        assert!(shell(&[
            "--command",
            "true",
            "--bounded",
            "--stdout-limit-bytes",
            "1",
            "--stderr-limit-bytes",
            "1"
        ])
        .is_err());
        assert!(shell(&[
            "--command",
            "true",
            "--bounded",
            "--deadline-ms",
            "1",
            "--stderr-limit-bytes",
            "1"
        ])
        .is_err());
        assert!(shell(&[
            "--command",
            "true",
            "--bounded",
            "--deadline-ms",
            "1",
            "--stdout-limit-bytes",
            "1"
        ])
        .is_err());
    }

    #[test]
    fn bounded_flags_require_explicit_opt_in() {
        assert!(shell(&["--command", "true", "--deadline-ms", "1"]).is_err());
        assert!(shell(&["--command", "true", "--stdout-limit-bytes", "1"]).is_err());
        assert!(shell(&["--command", "true", "--stderr-limit-bytes", "1"]).is_err());
    }

    #[test]
    fn bounded_shell_rejects_zero_negative_and_garbage_bounds() {
        for value in ["0", "-1", "1_000", "abc", ""] {
            assert!(
                shell(&[
                    "--command",
                    "true",
                    "--bounded",
                    "--deadline-ms",
                    value,
                    "--stdout-limit-bytes",
                    "1",
                    "--stderr-limit-bytes",
                    "1",
                ])
                .is_err(),
                "deadline {value:?} must be refused"
            );
            assert!(
                shell(&[
                    "--command",
                    "true",
                    "--bounded",
                    "--deadline-ms",
                    "1",
                    "--stdout-limit-bytes",
                    value,
                    "--stderr-limit-bytes",
                    "1",
                ])
                .is_err(),
                "stdout limit {value:?} must be refused"
            );
        }
    }

    #[test]
    fn bounded_shell_rejects_out_of_range_bounds() {
        assert!(shell(&[
            "--command",
            "true",
            "--bounded",
            "--deadline-ms",
            "3600001",
            "--stdout-limit-bytes",
            "1",
            "--stderr-limit-bytes",
            "1",
        ])
        .is_err());
        assert!(shell(&[
            "--command",
            "true",
            "--bounded",
            "--deadline-ms",
            "1",
            "--stdout-limit-bytes",
            "67108865",
            "--stderr-limit-bytes",
            "1",
        ])
        .is_err());
    }

    #[test]
    fn shell_without_bounded_flags_stays_unbounded() {
        match shell(&["--command", "true"]).expect("plain command shell parses") {
            ContainerSubcommand::Shell {
                bounded: None,
                command: Some(command),
                ..
            } => assert_eq!(command, "true"),
            other => panic!("unexpected shell shape {other:?}"),
        }
    }
}
