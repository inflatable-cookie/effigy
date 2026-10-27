use std::path::PathBuf;

use crate::value_parsing::{next_required_value, parse_repo_path};
use crate::{CliParseError, Command, HelpTopic, ReleaseArgs, ReleaseSubcommand};

use super::unknown_argument;

pub(super) fn parse_release_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Release));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Release)),
        "status" => parse_release_status(args),
        "gates" => parse_release_gates(args),
        "run" => parse_release_run(args),
        "resume" => parse_release_resume(args),
        "settle" => parse_release_settle(args),
        "approve" => parse_release_approve(args),
        "decide" | "decision" => parse_release_decide(args),
        "receipt" => parse_release_receipt(args),
        "validate" => Err(CliParseError::InvalidArguments(
            "`effigy release validate` is retired in v0.15: Effigy's own release metadata check is the repo task `effigy release:metadata`, which the `metadata` certify gate runs. A project keeps its own checks in `[tasks]` wired as `[release.gates]` or a target's `verify` (contract 054 §4.4). See the v0.15.0 Removed entry in CHANGELOG.md".to_owned(),
        )),
        "preflight" => Err(CliParseError::InvalidArguments(
            "`effigy release preflight` is retired in v0.15: Effigy's own preflight is the repo task `effigy release:preflight`, which the `preflight` pre gate runs in the `pre-gates` stage before `prepare` writes a version. A project keeps its own checks in `[tasks]` wired as `[release.gates]` or a target's `verify` (contract 054 §4.4). See the v0.15.0 Removed entry in CHANGELOG.md".to_owned(),
        )),
        "proof" => Err(CliParseError::InvalidArguments(
            "`effigy release proof` is retired in v0.15: Effigy's own artifact proof is the repo task `effigy release:proof`; `effigy release run` certifies the artifacts and `effigy release receipt <run>` records their digests (contract 054 §3, §6, §5). See the v0.15.0 Removed entry in CHANGELOG.md".to_owned(),
        )),
        "check-binary" => Err(CliParseError::InvalidArguments(
            "`effigy release check-binary` is retired in v0.15: Effigy's own GLIBC floor check is the repo task `effigy release:check-binary -- <bin> --glibc-floor <VER>`. See the v0.15.0 Removed entry in CHANGELOG.md".to_owned(),
        )),
        "verify-install" => Err(CliParseError::InvalidArguments(
            "`effigy release verify-install` is retired in v0.15: Effigy's own tagged-install proof is the repo task `effigy release:verify:install --tag <TAG>`, wired as a release target's `verify` (contract 054 §4.4). See the v0.15.0 Removed entry in CHANGELOG.md".to_owned(),
        )),
        "evidence" => Err(CliParseError::InvalidArguments(
            "`effigy release evidence` is retired in v0.15: `effigy release receipt <run> [--json]` prints what a Run recorded, and `effigy release:evidence:validate` / `effigy release:evidence:closeout` are Effigy repo tasks for artifact evidence (contract 054 §5). See the v0.15.0 Removed entry in CHANGELOG.md".to_owned(),
        )),
        "--plan" | "--dry-run" => parse_release_plan(args),
        "simulate" => Err(CliParseError::InvalidArguments(
            "`effigy release simulate` is retired in v0.15: use `effigy release --plan`, which shows the whole pipeline (next versions, files, gates by stage, artifacts, targets and approval checkpoints) and still mutates nothing. See the v0.15.0 Removed entry in CHANGELOG.md".to_owned(),
        )),
        "prepare" | "execute" => Err(CliParseError::InvalidArguments(format!(
            "`effigy release {subcmd}` is retired in v0.15: `effigy release run` prepares, certifies and publishes the release as one durable Run with a checkpoint at every stage boundary, and `effigy release resume <run>` continues an interrupted one (contract 054 §5). See the v0.15.0 Removed entry in CHANGELOG.md"
        ))),
        other => Err(unknown_argument(other)),
    }
}

fn parse_release_status<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut check_gates = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--check-gates" => check_gates = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Status { check_gates },
        repo_override,
        output_json,
    }))
}

fn parse_release_gates<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Gates,
        repo_override,
        output_json,
    }))
}

fn parse_release_run<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut to: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--to" => {
                to = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--to".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Run { to },
        repo_override,
        output_json,
    }))
}

/// A release Run id is positional. The retired `--allow-stale` prepared-state
/// resume has no Run to name, so it fails with the replacement instead.
fn parse_release_resume<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut run: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other if other.starts_with("--") => return Err(unknown_argument(other)),
            other => {
                if run.replace(other.to_owned()).is_some() {
                    return Err(CliParseError::InvalidArguments(
                        "`effigy release resume` takes one release Run id".to_owned(),
                    ));
                }
            }
        }
    }

    let Some(run) = run else {
        return Err(CliParseError::InvalidArguments(
            "`effigy release resume` needs the release Run id to continue, for example `effigy release resume <run>`; `effigy release status` shows the active Run. The pre-v0.15 prepared-state resume is retired with `release prepare` (contract 054 §5)".to_owned(),
        ));
    };

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::ResumeRun { run },
        repo_override,
        output_json,
    }))
}

fn parse_release_settle<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut run = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other if run.is_none() => run = Some(other.to_owned()),
            _ => {
                return Err(CliParseError::InvalidArguments(
                    "`release settle` takes one Run id".into(),
                ))
            }
        }
    }
    let run = run
        .ok_or_else(|| CliParseError::InvalidArguments("`release settle` needs a Run id".into()))?;
    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Settle { run },
        repo_override,
        output_json,
    }))
}

fn parse_release_approve<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut run: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other if other.starts_with("--") => return Err(unknown_argument(other)),
            other => {
                if run.replace(other.to_owned()).is_some() {
                    return Err(CliParseError::InvalidArguments(
                        "`effigy release approve` takes one release Run id".to_owned(),
                    ));
                }
            }
        }
    }

    let Some(run) = run else {
        return Err(CliParseError::InvalidArguments(
            "`effigy release approve` needs the release Run id waiting at an approval checkpoint; `effigy release status` shows it".to_owned(),
        ));
    };

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Approve { run },
        repo_override,
        output_json,
    }))
}

/// `effigy release decide <run> --published|--not-published` records the
/// operator's ruling on a publish whose outcome is unknown (contract 054 §7).
fn parse_release_decide<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut run: Option<String> = None;
    let mut published: Option<bool> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--published" => {
                if published.replace(true).is_some() {
                    return Err(CliParseError::InvalidArguments(
                        "`effigy release decide` takes one of `--published` or `--not-published`"
                            .to_owned(),
                    ));
                }
            }
            "--not-published" => {
                if published.replace(false).is_some() {
                    return Err(CliParseError::InvalidArguments(
                        "`effigy release decide` takes one of `--published` or `--not-published`"
                            .to_owned(),
                    ));
                }
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => {
                if run.replace(other.to_owned()).is_some() {
                    return Err(CliParseError::InvalidArguments(
                        "`effigy release decide` takes one release Run id".to_owned(),
                    ));
                }
            }
        }
    }

    let Some(run) = run else {
        return Err(CliParseError::InvalidArguments(
            "`effigy release decide` needs the release Run id stopped at `awaiting-decision`; `effigy release status` shows it"
                .to_owned(),
        ));
    };
    let Some(published) = published else {
        return Err(CliParseError::InvalidArguments(
            "`effigy release decide` needs `--published` or `--not-published`: the operator records whether the candidate reached the destination before the Run continues"
                .to_owned(),
        ));
    };

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Decide { run, published },
        repo_override,
        output_json,
    }))
}

fn parse_release_receipt<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut run: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other if other.starts_with("--") => return Err(unknown_argument(other)),
            other => {
                if run.replace(other.to_owned()).is_some() {
                    return Err(CliParseError::InvalidArguments(
                        "`effigy release receipt` takes one release Run id".to_owned(),
                    ));
                }
            }
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Receipt { run },
        repo_override,
        output_json,
    }))
}

fn parse_release_plan<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut version_override: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--version" => {
                version_override = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--version".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Plan { version_override },
        repo_override,
        output_json,
    }))
}
