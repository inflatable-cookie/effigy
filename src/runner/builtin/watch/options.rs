use crate::TaskInvocation;

use super::super::super::RunnerError;
use super::super::arg_parser::BuiltinArgParser;
use super::super::unknown_builtin_args;

const DEFAULT_DEBOUNCE_MS: u64 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WatchOwner {
    Effigy,
    External,
}

#[derive(Debug)]
pub(super) struct WatchRequest {
    pub(super) output_json: bool,
    pub(super) help: bool,
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
    let mut help = false;
    let mut owner: Option<WatchOwner> = None;
    let mut debounce_ms = DEFAULT_DEBOUNCE_MS;
    let mut include = Vec::<String>::new();
    let mut exclude = Vec::<String>::new();
    let mut max_runs: Option<usize> = None;
    let mut target: Option<TaskInvocation> = None;

    while let Some(arg) = parser.next() {
        match arg {
            "--json" => {
                output_json = true;
            }
            "--help" | "-h" => {
                help = true;
            }
            "--owner" => {
                let value =
                    parser.next_value("`--owner` requires a value (`effigy` or `external`)")?;
                owner = match value {
                    "effigy" => Some(WatchOwner::Effigy),
                    "external" => Some(WatchOwner::External),
                    _ => {
                        return Err(RunnerError::TaskInvocation(format!(
                            "invalid `--owner` value `{value}` (expected `effigy` or `external`)"
                        )));
                    }
                };
            }
            "--debounce-ms" => {
                let value = parser.next_value("`--debounce-ms` requires a numeric value")?;
                let parsed = value.parse::<u64>().map_err(|_| {
                    RunnerError::TaskInvocation(format!(
                        "invalid `--debounce-ms` value `{value}` (expected a positive integer)"
                    ))
                })?;
                if parsed == 0 {
                    return Err(RunnerError::TaskInvocation(
                        "`--debounce-ms` must be greater than zero".to_owned(),
                    ));
                }
                debounce_ms = parsed;
            }
            "--include" => {
                let value = parser.next_value("`--include` requires a glob value")?;
                include.push(value.to_owned());
            }
            "--exclude" => {
                let value = parser.next_value("`--exclude` requires a glob value")?;
                exclude.push(value.to_owned());
            }
            "--once" => {
                max_runs = Some(1);
            }
            "--max-runs" => {
                let value = parser.next_value("`--max-runs` requires a numeric value")?;
                let parsed = value.parse::<usize>().map_err(|_| {
                    RunnerError::TaskInvocation(format!(
                        "invalid `--max-runs` value `{value}` (expected an integer >= 1)"
                    ))
                })?;
                if parsed == 0 {
                    return Err(RunnerError::TaskInvocation(
                        "`--max-runs` must be greater than zero".to_owned(),
                    ));
                }
                max_runs = Some(parsed);
            }
            "--" => {
                return Err(RunnerError::TaskInvocation(
                    "watch requires `<task>` before passthrough arguments (`--`)".to_owned(),
                ));
            }
            _ if arg.starts_with('-') => {
                return Err(unknown_builtin_args(&task.name, &[arg.to_owned()]));
            }
            _ => {
                target = Some(TaskInvocation {
                    name: arg.to_owned(),
                    args: parser.remaining().to_vec(),
                });
                break;
            }
        }
    }

    Ok(WatchRequest {
        output_json,
        help,
        owner,
        debounce_ms,
        include,
        exclude,
        max_runs,
        target,
    })
}
