use std::path::PathBuf;

use crate::{Command, DistributionArgs, DistributionSubcommand, HelpTopic};

use crate::value_parsing::{next_required_value, parse_repo_path};

use super::{unknown_argument, CliParseError};

pub(super) fn parse_distribution_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Distribution));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Distribution)),
        "validate-metadata" => parse_distribution_validate_metadata(args),
        "check-glibc-floor" => parse_distribution_check_glibc_floor(args),
        "preflight" => parse_distribution_preflight(args),
        "first-publish" => parse_distribution_first_publish(args),
        "validate-artifacts" => parse_distribution_validate_artifacts(args),
        "generate-closeout" => parse_distribution_generate_closeout(args),
        "write-summary" => parse_distribution_write_summary(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_distribution_validate_metadata<I>(args: I) -> Result<Command, CliParseError>
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::ValidateMetadata { tag },
        repo_override,
        output_json,
    }))
}

fn parse_distribution_check_glibc_floor<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut binary_path: Option<PathBuf> = None;
    let mut max_glibc: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--binary" => {
                binary_path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--binary".to_owned(),
                    },
                )?));
            }
            "--max-glibc" => {
                max_glibc = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--max-glibc".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::CheckGlibcFloor {
            binary_path: binary_path.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--binary".to_owned(),
            })?,
            max_glibc: max_glibc.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--max-glibc".to_owned(),
            })?,
        },
        repo_override,
        output_json,
    }))
}

fn parse_distribution_preflight<I>(args: I) -> Result<Command, CliParseError>
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::Preflight {
            tag,
            skip_docs,
            skip_smoke,
            output_path,
        },
        repo_override,
        output_json,
    }))
}

fn parse_distribution_first_publish<I>(args: I) -> Result<Command, CliParseError>
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::FirstPublish {
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

fn parse_distribution_validate_artifacts<I>(args: I) -> Result<Command, CliParseError>
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    let artifacts_dir = artifacts_dir.ok_or_else(|| CliParseError::MissingFlagValue {
        flag: "--artifacts-dir".to_owned(),
    })?;

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::ValidateArtifacts {
            artifacts_dir,
            expect_homebrew,
        },
        repo_override,
        output_json,
    }))
}

fn parse_distribution_generate_closeout<I>(args: I) -> Result<Command, CliParseError>
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::GenerateCloseout {
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
        repo_override,
        output_json,
    }))
}

fn parse_distribution_write_summary<I>(args: I) -> Result<Command, CliParseError>
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::WriteSummary {
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
        repo_override,
        output_json,
    }))
}
