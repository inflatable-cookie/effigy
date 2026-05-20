use std::path::PathBuf;

use crate::value_parsing::{next_required_value, parse_repo_path};
use crate::{
    CliParseError, Command, HelpTopic, ReleaseArgs, ReleaseEvidenceSubcommand, ReleaseSubcommand,
};

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
        "resume" => parse_release_resume(args),
        "verify-install" => parse_release_verify_install(args),
        "validate" => parse_release_validate(args),
        "check-binary" => parse_release_check_binary(args),
        "preflight" => parse_release_preflight(args),
        "proof" => parse_release_proof(args),
        "evidence" => parse_release_evidence(args),
        "simulate" => parse_release_simulate(args),
        "prepare" => parse_release_prepare(args),
        "execute" => parse_release_execute(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_release_validate<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Validate { tag },
        repo_override,
        output_json,
    }))
}

fn parse_release_check_binary<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut binary_path: Option<PathBuf> = None;
    let mut glibc_floor: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--glibc-floor" => {
                glibc_floor = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--glibc-floor".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other if binary_path.is_none() => binary_path = Some(PathBuf::from(other)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::CheckBinary {
            binary_path: binary_path.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "<bin>".to_owned(),
            })?,
            glibc_floor: glibc_floor.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--glibc-floor".to_owned(),
            })?,
        },
        repo_override,
        output_json,
    }))
}

fn parse_release_preflight<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;
    let mut skip_docs = false;
    let mut skip_smoke = false;
    let mut output_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--skip-docs" => skip_docs = true,
            "--skip-smoke" => skip_smoke = true,
            "--output" => {
                output_path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--output".to_owned(),
                    },
                )?));
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Preflight {
            tag,
            skip_docs,
            skip_smoke,
            output_path,
        },
        repo_override,
        output_json,
    }))
}

fn parse_release_proof<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;
    let mut crate_version: Option<String> = None;
    let mut repo_url = "https://github.com/inflatable-cookie/effigy.git".to_owned();
    let mut brew_formula = "inflatable-cookie/effigy/effigy".to_owned();
    let mut skip_homebrew = false;
    let mut artifacts_dir: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--crate-version" => {
                crate_version = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--crate-version".to_owned(),
                    },
                )?);
            }
            "--repo-url" => {
                repo_url = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--repo-url".to_owned(),
                    },
                )?;
            }
            "--brew-formula" => {
                brew_formula = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--brew-formula".to_owned(),
                    },
                )?;
            }
            "--skip-homebrew" => skip_homebrew = true,
            "--artifacts-dir" => {
                artifacts_dir = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--artifacts-dir".to_owned(),
                    },
                )?));
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Proof {
            tag: tag.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--tag".to_owned(),
            })?,
            crate_version,
            repo_url,
            brew_formula,
            skip_homebrew,
            artifacts_dir,
        },
        repo_override,
        output_json,
    }))
}

fn parse_release_evidence<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Release));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Release)),
        "validate" => parse_release_evidence_validate(args),
        "closeout" => parse_release_evidence_closeout(args),
        "summary" => parse_release_evidence_summary(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_release_evidence_validate<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut artifacts_dir: Option<PathBuf> = None;
    let mut expect_homebrew = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--artifacts-dir" => {
                artifacts_dir = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--artifacts-dir".to_owned(),
                    },
                )?));
            }
            "--expect-homebrew" => expect_homebrew = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Evidence {
            subcommand: ReleaseEvidenceSubcommand::Validate {
                artifacts_dir: artifacts_dir.ok_or_else(|| CliParseError::MissingFlagValue {
                    flag: "--artifacts-dir".to_owned(),
                })?,
                expect_homebrew,
            },
        },
        repo_override,
        output_json,
    }))
}

fn parse_release_evidence_closeout<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;
    let mut artifacts_dir: Option<PathBuf> = None;
    let mut output_path: Option<PathBuf> = None;
    let mut owner = "release".to_owned();
    let mut expect_homebrew = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--artifacts-dir" => {
                artifacts_dir = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--artifacts-dir".to_owned(),
                    },
                )?));
            }
            "--output" => {
                output_path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--output".to_owned(),
                    },
                )?));
            }
            "--owner" => {
                owner = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--owner".to_owned(),
                    },
                )?;
            }
            "--expect-homebrew" => expect_homebrew = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Evidence {
            subcommand: ReleaseEvidenceSubcommand::Closeout {
                tag: tag.ok_or_else(|| CliParseError::MissingFlagValue {
                    flag: "--tag".to_owned(),
                })?,
                artifacts_dir: artifacts_dir.ok_or_else(|| CliParseError::MissingFlagValue {
                    flag: "--artifacts-dir".to_owned(),
                })?,
                output_path,
                owner,
                expect_homebrew,
            },
        },
        repo_override,
        output_json,
    }))
}

fn parse_release_evidence_summary<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;
    let mut artifacts_dir: Option<PathBuf> = None;
    let mut crate_version: Option<String> = None;
    let mut repo_url = "https://github.com/inflatable-cookie/effigy.git".to_owned();
    let mut brew_formula = "inflatable-cookie/effigy/effigy".to_owned();
    let mut homebrew_executed = false;
    let mut log_files = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--artifacts-dir" => {
                artifacts_dir = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--artifacts-dir".to_owned(),
                    },
                )?));
            }
            "--crate-version" => {
                crate_version = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--crate-version".to_owned(),
                    },
                )?);
            }
            "--repo-url" => {
                repo_url = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--repo-url".to_owned(),
                    },
                )?;
            }
            "--brew-formula" => {
                brew_formula = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--brew-formula".to_owned(),
                    },
                )?;
            }
            "--homebrew-executed" => homebrew_executed = true,
            "--log-file" => log_files.push(next_required_value(
                &mut args,
                CliParseError::MissingFlagValue {
                    flag: "--log-file".to_owned(),
                },
            )?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Evidence {
            subcommand: ReleaseEvidenceSubcommand::Summary {
                tag: tag.ok_or_else(|| CliParseError::MissingFlagValue {
                    flag: "--tag".to_owned(),
                })?,
                artifacts_dir: artifacts_dir.ok_or_else(|| CliParseError::MissingFlagValue {
                    flag: "--artifacts-dir".to_owned(),
                })?,
                crate_version,
                repo_url,
                brew_formula,
                homebrew_executed,
                log_files,
            },
        },
        repo_override,
        output_json,
    }))
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

fn parse_release_resume<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut allow_stale = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--allow-stale" => allow_stale = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Resume { allow_stale },
        repo_override,
        output_json,
    }))
}

fn parse_release_simulate<I>(args: I) -> Result<Command, CliParseError>
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
        subcommand: ReleaseSubcommand::Simulate { version_override },
        repo_override,
        output_json,
    }))
}

fn parse_release_verify_install<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;
    let mut repo_url: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--repo-url" => {
                repo_url = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--repo-url".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::VerifyInstall { tag, repo_url },
        repo_override,
        output_json,
    }))
}

fn parse_release_prepare<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut check_gates = false;
    let mut plan = false;
    let mut yes = false;
    let mut version_override: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--check-gates" => check_gates = true,
            "--plan" | "--dry-run" => plan = true,
            "--yes" => yes = true,
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
        subcommand: ReleaseSubcommand::Prepare {
            plan,
            check_gates,
            yes,
            version_override,
        },
        repo_override,
        output_json,
    }))
}

fn parse_release_execute<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut plan = false;
    let mut yes = false;
    let mut allow_stale = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--plan" | "--dry-run" => plan = true,
            "--yes" => yes = true,
            "--allow-stale" => allow_stale = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Execute {
            plan,
            yes,
            allow_stale,
        },
        repo_override,
        output_json,
    }))
}
