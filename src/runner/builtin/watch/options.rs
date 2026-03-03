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
                parser.bool_flag(&mut output_json);
            }
            "--help" | "-h" => {
                parser.bool_flag(&mut help);
            }
            "--owner" => {
                let value =
                    parser.next_value("`--owner` requires a value (`effigy` or `external`)")?;
                owner = Some(parse_watch_owner(value)?);
            }
            "--debounce-ms" => {
                debounce_ms = parse_positive_u64_flag(
                    &mut parser,
                    "--debounce-ms",
                    "`--debounce-ms` requires a numeric value",
                )?;
            }
            "--include" => {
                let value = parser.string_flag_value("`--include` requires a glob value")?;
                include.push(value);
            }
            "--exclude" => {
                let value = parser.string_flag_value("`--exclude` requires a glob value")?;
                exclude.push(value);
            }
            "--once" => {
                max_runs = Some(1);
            }
            "--max-runs" => {
                max_runs = Some(parse_positive_usize_flag(
                    &mut parser,
                    "--max-runs",
                    "`--max-runs` requires a numeric value",
                )?);
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

fn parse_watch_owner(value: &str) -> Result<WatchOwner, RunnerError> {
    match value {
        "effigy" => Ok(WatchOwner::Effigy),
        "external" => Ok(WatchOwner::External),
        _ => Err(RunnerError::TaskInvocation(format!(
            "invalid `--owner` value `{value}` (expected `effigy` or `external`)"
        ))),
    }
}

fn parse_positive_u64_flag(
    parser: &mut BuiltinArgParser<'_>,
    flag: &str,
    missing_message: &str,
) -> Result<u64, RunnerError> {
    let parsed = parser.u64_flag_value(missing_message, |value| {
        format!("invalid `{flag}` value `{value}` (expected a positive integer)")
    })?;
    if parsed == 0 {
        return Err(RunnerError::TaskInvocation(format!(
            "`{flag}` must be greater than zero"
        )));
    }
    Ok(parsed)
}

fn parse_positive_usize_flag(
    parser: &mut BuiltinArgParser<'_>,
    flag: &str,
    missing_message: &str,
) -> Result<usize, RunnerError> {
    let parsed = parser.usize_flag_value(missing_message, |value| {
        format!("invalid `{flag}` value `{value}` (expected an integer >= 1)")
    })?;
    if parsed == 0 {
        return Err(RunnerError::TaskInvocation(format!(
            "`{flag}` must be greater than zero"
        )));
    }
    Ok(parsed)
}
