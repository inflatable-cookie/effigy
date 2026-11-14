use std::path::PathBuf;

use crate::{CliParseError, Command, FmtArgs, FmtSubcommand, HelpTopic};

use super::common::{CommonArgs, CommonFlag};
use super::unknown_argument;

pub(super) fn parse_fmt_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Fmt));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Fmt)),
        "rust" => parse_fmt_rust(args),
        other => Err(unknown_argument(other)),
    }
}

/// `effigy fmt-rust <FILE>...`: the explicit spelling of the built-in Rust
/// formatter, for repositories whose own `fmt` task shadows `effigy fmt`.
pub(super) fn parse_fmt_rust_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    parse_fmt_rust(args)
}

fn parse_fmt_rust<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut files: Vec<PathBuf> = Vec::new();
    let mut common = CommonArgs::default();
    let mut check = false;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match common.take(&arg, &mut args, HelpTopic::Fmt)? {
            CommonFlag::Help(topic) => return Ok(Command::Help(topic)),
            CommonFlag::Consumed => continue,
            CommonFlag::Unclaimed => {}
        }
        match arg.as_str() {
            "--check" => check = true,
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => files.push(PathBuf::from(arg)),
        }
    }
    // An empty file set cannot express an intent: formatting nothing would
    // silently read as success.
    if files.is_empty() {
        return Err(CliParseError::InvalidArguments(
            "`effigy fmt rust` requires at least one file path".to_owned(),
        ));
    }

    Ok(Command::Fmt(FmtArgs {
        subcommand: if check {
            FmtSubcommand::RustCheck { files }
        } else {
            FmtSubcommand::Rust { files }
        },
        repo_override: common.repo_override,
        output_json: common.output_json,
    }))
}
