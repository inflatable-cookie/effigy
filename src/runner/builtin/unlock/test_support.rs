use effigy_cli::TaskInvocation;

use super::request::parse_unlock_request;
use crate::runner::error::RunnerError;

pub(in crate::runner) struct UnlockParseContract {
    pub(in crate::runner) output_json: bool,
    pub(in crate::runner) unlock_all_flag: bool,
    pub(in crate::runner) scopes: Vec<String>,
}

pub(in crate::runner) fn parse_unlock_contract_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<UnlockParseContract, RunnerError> {
    let parsed = parse_unlock_request(task, args)?;
    Ok(UnlockParseContract {
        output_json: parsed.output_json,
        unlock_all_flag: parsed.unlock_all_flag,
        scopes: parsed
            .scopes
            .into_iter()
            .map(|scope| scope.label())
            .collect(),
    })
}
