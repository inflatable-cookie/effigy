use crate::TaskInvocation;

use super::super::super::RunnerError;
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
    let mut output_json = false;
    let mut help = false;
    let mut owner: Option<WatchOwner> = None;
    let mut debounce_ms = DEFAULT_DEBOUNCE_MS;
    let mut include = Vec::<String>::new();
    let mut exclude = Vec::<String>::new();
    let mut max_runs: Option<usize> = None;
    let mut target: Option<TaskInvocation> = None;
    let mut i = 0usize;

    while i < args.len() {
        if target.is_some() {
            break;
        }
        let arg = &args[i];
        match arg.as_str() {
            "--json" => {
                output_json = true;
                i += 1;
            }
            "--help" | "-h" => {
                help = true;
                i += 1;
            }
            "--owner" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--owner` requires a value (`effigy` or `external`)".to_owned(),
                    ));
                };
                owner = match value.as_str() {
                    "effigy" => Some(WatchOwner::Effigy),
                    "external" => Some(WatchOwner::External),
                    _ => {
                        return Err(RunnerError::TaskInvocation(format!(
                            "invalid `--owner` value `{value}` (expected `effigy` or `external`)"
                        )));
                    }
                };
                i += 2;
            }
            "--debounce-ms" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--debounce-ms` requires a numeric value".to_owned(),
                    ));
                };
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
                i += 2;
            }
            "--include" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--include` requires a glob value".to_owned(),
                    ));
                };
                include.push(value.clone());
                i += 2;
            }
            "--exclude" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--exclude` requires a glob value".to_owned(),
                    ));
                };
                exclude.push(value.clone());
                i += 2;
            }
            "--once" => {
                max_runs = Some(1);
                i += 1;
            }
            "--max-runs" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--max-runs` requires a numeric value".to_owned(),
                    ));
                };
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
                i += 2;
            }
            "--" => {
                return Err(RunnerError::TaskInvocation(
                    "watch requires `<task>` before passthrough arguments (`--`)".to_owned(),
                ));
            }
            _ if arg.starts_with('-') => {
                return Err(unknown_builtin_args(&task.name, std::slice::from_ref(arg)));
            }
            _ => {
                target = Some(TaskInvocation {
                    name: arg.clone(),
                    args: args.iter().skip(i + 1).cloned().collect(),
                });
                i = args.len();
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
