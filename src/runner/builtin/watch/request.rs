use crate::TaskInvocation;

use super::super::super::RunnerError;
use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};

const DEFAULT_DEBOUNCE_MS: u64 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WatchOwner {
    Effigy,
    External,
}

#[derive(Debug)]
pub(super) struct WatchRequest {
    pub(super) output_json: bool,
    pub(super) owner: Option<WatchOwner>,
    pub(super) debounce_ms: u64,
    pub(super) include: Vec<String>,
    pub(super) exclude: Vec<String>,
    pub(super) max_runs: Option<usize>,
    pub(super) target: Option<TaskInvocation>,
}

pub(super) fn parse_watch_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<WatchRequest, RunnerError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut owner: Option<WatchOwner> = None;
    let mut debounce_ms = DEFAULT_DEBOUNCE_MS;
    let mut include = Vec::<String>::new();
    let mut exclude = Vec::<String>::new();
    let mut max_runs: Option<usize> = None;
    let mut target: Option<TaskInvocation> = None;

    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json) {
            return Ok(ParseLoopAction::Handled);
        }
        match arg {
            "--owner" => {
                owner = Some(parser.quoted_choice_flag_value(
                    "--owner",
                    "`effigy` or `external`",
                    |value| match value {
                        "effigy" => Some(WatchOwner::Effigy),
                        "external" => Some(WatchOwner::External),
                        _ => None,
                    },
                )?);
                Ok(ParseLoopAction::Handled)
            }
            "--debounce-ms" => {
                debounce_ms = parser.positive_u64_flag_value(
                    "--debounce-ms",
                    "`--debounce-ms` requires a numeric value",
                )?;
                Ok(ParseLoopAction::Handled)
            }
            "--include" => {
                let value = parser.flag_string_value("--include", "a glob value")?;
                include.push(value);
                Ok(ParseLoopAction::Handled)
            }
            "--exclude" => {
                let value = parser.flag_string_value("--exclude", "a glob value")?;
                exclude.push(value);
                Ok(ParseLoopAction::Handled)
            }
            "--once" => {
                max_runs = Some(1);
                Ok(ParseLoopAction::Handled)
            }
            "--max-runs" => {
                max_runs = Some(parser.positive_usize_flag_value(
                    "--max-runs",
                    "`--max-runs` requires a numeric value",
                )?);
                Ok(ParseLoopAction::Handled)
            }
            "--" => Err(RunnerError::task_invocation(
                "watch requires `<task>` before passthrough arguments (`--`)",
            )),
            _ => parser.unknown_if_flag_or(arg, |value| {
                target = Some(parser.positional_task_invocation(value));
                Ok(ParseLoopAction::Break)
            }),
        }
    })?;

    Ok(WatchRequest {
        output_json,
        owner,
        debounce_ms,
        include,
        exclude,
        max_runs,
        target,
    })
}

#[cfg(test)]
pub(in crate::runner) struct WatchParseContract {
    pub(in crate::runner) output_json: bool,
    pub(in crate::runner) owner: Option<&'static str>,
    pub(in crate::runner) debounce_ms: u64,
    pub(in crate::runner) include: Vec<String>,
    pub(in crate::runner) exclude: Vec<String>,
    pub(in crate::runner) max_runs: Option<usize>,
    pub(in crate::runner) target_name: Option<String>,
    pub(in crate::runner) target_args: Vec<String>,
}

#[cfg(test)]
pub(in crate::runner) fn parse_watch_contract_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<WatchParseContract, RunnerError> {
    let parsed = parse_watch_request(task, args)?;
    let owner = match parsed.owner {
        Some(WatchOwner::Effigy) => Some("effigy"),
        Some(WatchOwner::External) => Some("external"),
        None => None,
    };
    let (target_name, target_args) = match parsed.target {
        Some(target) => (Some(target.name), target.args),
        None => (None, Vec::new()),
    };
    Ok(WatchParseContract {
        output_json: parsed.output_json,
        owner,
        debounce_ms: parsed.debounce_ms,
        include: parsed.include,
        exclude: parsed.exclude,
        max_runs: parsed.max_runs,
        target_name,
        target_args,
    })
}
