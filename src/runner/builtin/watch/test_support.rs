use effigy_cli::TaskInvocation;

use super::request::{parse_watch_request, WatchOwner};
use crate::runner::error::RunnerError;

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
