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
        "run" => parse_release_run(args),
        "resume" => parse_release_resume(args),
        "approve" => parse_release_approve(args),
        "receipt" => parse_release_receipt(args),
        "verify-install" => parse_release_verify_install(args),
        "validate" => parse_release_validate(args),
        "check-binary" => parse_release_check_binary(args),
        "preflight" => parse_release_preflight(args),
        "proof" => parse_release_proof(args),
        "evidence" => parse_release_evidence(args),
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
