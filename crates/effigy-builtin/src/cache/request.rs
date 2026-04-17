use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use super::super::help_text::{render_titled_help, HelpSection};
use crate::BuiltinError;

#[derive(Debug)]
pub(super) enum CacheRequest {
    Inspect(InspectRequest),
    Invalidate(InvalidateRequest),
}

#[derive(Debug)]
pub(super) struct InspectRequest {
    pub(super) output_json: bool,
    pub(super) selector: Option<String>,
}

#[derive(Debug)]
pub(super) struct InvalidateRequest {
    pub(super) output_json: bool,
    pub(super) invalidate_all: bool,
    pub(super) selectors: Vec<String>,
}

pub(super) fn parse_cache_request(args: &[String]) -> Result<CacheRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let command_raw = parser.required_subcommand("cache", "`inspect` or `invalidate`")?;
    match parse_cache_command(command_raw)? {
        CacheCommand::Inspect => parse_inspect_request(parser),
        CacheCommand::Invalidate => parse_invalidate_request(parser),
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

fn parse_inspect_request(mut parser: BuiltinArgParser<'_>) -> Result<CacheRequest, BuiltinError> {
    let mut output_json = false;
    let mut selectors = Vec::<String>::new();
    parser.parse_loop_require_no_unknown("cache", |parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json) {
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--all" {
            return Err(BuiltinError::task_invocation(
                "`cache inspect` does not support `--all`; use `cache invalidate --all`",
            ));
        }
        if arg.starts_with('-') {
            return Ok(ParseLoopAction::Unknown);
        }
        selectors.push(arg.to_owned());
        Ok(ParseLoopAction::Handled)
    })?;

    if selectors.len() > 1 {
        return Err(BuiltinError::task_invocation(
            "`cache inspect` accepts at most one selector",
        ));
    }

    Ok(CacheRequest::Inspect(InspectRequest {
        output_json,
        selector: selectors.into_iter().next(),
    }))
}

fn parse_invalidate_request(
    mut parser: BuiltinArgParser<'_>,
) -> Result<CacheRequest, BuiltinError> {
    let mut output_json = false;
    let mut invalidate_all = false;
    let mut selectors = Vec::<String>::new();
    parser.parse_loop_require_no_unknown("cache", |parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json) {
            return Ok(ParseLoopAction::Handled);
        }
        match arg {
            "--all" => {
                invalidate_all = true;
                Ok(ParseLoopAction::Handled)
            }
            _ if arg.starts_with('-') => Ok(ParseLoopAction::Unknown),
            value => {
                selectors.push(value.to_owned());
                Ok(ParseLoopAction::Handled)
            }
        }
    })?;

    if invalidate_all && !selectors.is_empty() {
        return Err(BuiltinError::task_invocation(
            "`cache invalidate` accepts either `--all` or selectors, not both",
        ));
    }
    if !invalidate_all && selectors.is_empty() {
        return Err(BuiltinError::task_invocation(
            "`cache invalidate` requires one or more selectors (or `--all`)",
        ));
    }

    Ok(CacheRequest::Invalidate(InvalidateRequest {
        output_json,
        invalidate_all,
        selectors,
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheCommand {
    Inspect,
    Invalidate,
}

fn parse_cache_command(command_raw: &str) -> Result<CacheCommand, BuiltinError> {
    match command_raw {
        "inspect" => Ok(CacheCommand::Inspect),
        "invalidate" => Ok(CacheCommand::Invalidate),
        other => Err(BuiltinError::task_invocation(
            BuiltinArgParser::builtin_unknown_subcommand_message(
                "cache",
                other,
                "`inspect` or `invalidate`",
            ),
        )),
    }
}
