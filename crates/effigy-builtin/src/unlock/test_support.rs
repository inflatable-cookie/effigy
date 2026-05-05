use effigy_cli::TaskInvocation;

use super::request::parse_unlock_request;
use crate::BuiltinError;

pub struct UnlockParseContract {
    pub output_json: bool,
    pub unlock_all_flag: bool,
    pub yes: bool,
    pub scopes: Vec<String>,
}

pub fn parse_unlock_contract_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<UnlockParseContract, BuiltinError> {
    let parsed = parse_unlock_request(task, args)?;
    Ok(UnlockParseContract {
        output_json: parsed.output_json,
        unlock_all_flag: parsed.unlock_all_flag,
        yes: parsed.yes,
        scopes: parsed
            .scopes
            .into_iter()
            .map(|scope| scope.label())
            .collect(),
    })
}
