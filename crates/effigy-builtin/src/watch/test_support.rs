use effigy_cli::TaskInvocation;

use super::request::{parse_watch_request, WatchOwner};
use crate::BuiltinError;

pub struct WatchParseContract {
    pub output_json: bool,
    pub owner: Option<&'static str>,
    pub debounce_ms: u64,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub max_runs: Option<usize>,
    pub target_name: Option<String>,
    pub target_args: Vec<String>,
}

pub fn parse_watch_contract_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<WatchParseContract, BuiltinError> {
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
