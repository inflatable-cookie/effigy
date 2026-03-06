use crate::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::runner::error::RunnerError;

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

impl WatchRequest {
    pub(super) fn validate_execution_policy(&self) -> Result<(), RunnerError> {
        if self.output_json && self.max_runs.is_none() {
            return Err(RunnerError::task_invocation(
                "`--json` requires a bounded watch run (`--once` or `--max-runs <N>`).",
            ));
        }

        let owner = self.owner.ok_or_else(|| {
            RunnerError::task_invocation(
                "`--owner <effigy|external>` is required to avoid nested watcher conflicts.",
            )
        })?;
        if owner == WatchOwner::External {
            return Err(RunnerError::task_invocation(
                "watch owner `external` means task-managed watching is expected. Run the task directly (without `effigy watch`) to avoid nested watcher loops.",
            ));
        }

        let target = self.target.as_ref().ok_or_else(|| {
            RunnerError::task_invocation(
                "watch requires a target task selector (for example `effigy watch --owner effigy test`).",
            )
        })?;
        if target.name == "watch" {
            return Err(RunnerError::task_invocation(
                "watch target cannot be `watch` (nested watch loops are blocked by owner policy).",
            ));
        }

        Ok(())
    }

    pub(super) fn validated_target(&self) -> Result<&TaskInvocation, RunnerError> {
        self.target.as_ref().ok_or_else(|| {
            RunnerError::task_invocation(
                "watch requires a target task selector (for example `effigy watch --owner effigy test`).",
            )
        })
    }
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
