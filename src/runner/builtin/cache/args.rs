use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use super::super::help_text::{render_titled_help, HelpSection};
use super::{CacheArgs, CacheCommand, RunnerError};

pub(super) fn parse_cache_args(args: &[String]) -> Result<CacheArgs, RunnerError> {
    let mut parser = BuiltinArgParser::new(args);
    let command_raw = parser.required_subcommand("cache", "`inspect` or `invalidate`")?;
    let command = parse_cache_command(command_raw)?;

    let mut output_json = false;
    let mut invalidate_all = false;
    let mut selectors = Vec::<String>::new();
    parser.parse_loop_require_no_unknown("cache", |parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json) {
            return Ok(ParseLoopAction::Handled);
        }
        match command {
            CacheCommand::Inspect => {
                if arg == "--all" {
                    return Err(RunnerError::task_invocation(
                        "`cache inspect` does not support `--all`; use `cache invalidate --all`",
                    ));
                }
                if arg.starts_with('-') {
                    return Ok(ParseLoopAction::Unknown);
                }
                selectors.push(arg.to_owned());
                Ok(ParseLoopAction::Handled)
            }
            CacheCommand::Invalidate => match arg {
                "--all" => {
                    invalidate_all = true;
                    Ok(ParseLoopAction::Handled)
                }
                _ if arg.starts_with('-') => Ok(ParseLoopAction::Unknown),
                value => {
                    selectors.push(value.to_owned());
                    Ok(ParseLoopAction::Handled)
                }
            },
        }
    })?;

    Ok(CacheArgs {
        command,
        output_json,
        invalidate_all,
        selectors,
    })
}

fn parse_cache_command(command_raw: &str) -> Result<CacheCommand, RunnerError> {
    match command_raw {
        "inspect" => Ok(CacheCommand::Inspect),
        "invalidate" => Ok(CacheCommand::Invalidate),
        other => Err(RunnerError::task_invocation(
            BuiltinArgParser::builtin_unknown_subcommand_message(
                "cache",
                other,
                "`inspect` or `invalidate`",
            ),
        )),
    }
}

pub(super) fn render_cache_help() -> String {
    render_titled_help(
        "cache",
        &[
            HelpSection::Plain {
                heading: "Usage",
                lines: &[
                    "effigy cache inspect [<selector>] [--json]",
                    "effigy cache invalidate [<selector>...] [--all] [--json]",
                ],
            },
            HelpSection::Bulleted {
                heading: "Notes",
                items: &[
                    "phase-1 cache is explicit opt-in via `[tasks.<name>.cache]`",
                    "cache hit requires matching fingerprint and declared outputs to exist",
                ],
            },
            HelpSection::Bulleted {
                heading: "Examples",
                items: &[
                    "effigy cache inspect",
                    "effigy cache inspect build",
                    "effigy cache invalidate build",
                    "effigy cache invalidate --all",
                    "effigy cache inspect --json",
                ],
            },
        ],
    )
}
