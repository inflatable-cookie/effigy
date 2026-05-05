use effigy_cli::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use crate::BuiltinError;
use crate::LockScope;

pub(super) struct UnlockRequest {
    pub(super) output_json: bool,
    pub(super) unlock_all_flag: bool,
    pub(super) yes: bool,
    pub(super) scopes: Vec<LockScope>,
}

pub(super) fn parse_unlock_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<UnlockRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut unlock_all_flag = false;
    let mut yes = false;
    let mut scopes = Vec::<LockScope>::new();

    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_any_bool_flag(
            arg,
            &mut [
                ("--json", &mut output_json),
                ("--all", &mut unlock_all_flag),
                ("--yes", &mut yes),
            ],
        ) {
            return Ok(ParseLoopAction::Handled);
        }
        parser.unknown_if_flag_or(arg, |value| {
            let Some(scope) = LockScope::parse(value) else {
                return Err(BuiltinError::task_invocation(format!(
                    "`{}` unlock target `{value}` is invalid; expected `workspace`, `shared:<name>`, `task:<name>`, or `profile:<task>/<profile>`",
                    task.name
                )));
            };
            scopes.push(scope);
            Ok(ParseLoopAction::Handled)
        })
    })?;

    if unlock_all_flag && !scopes.is_empty() {
        return Err(BuiltinError::task_invocation(
            "`unlock` accepts either `--all` or explicit scope values, not both",
        ));
    }
    if !unlock_all_flag && scopes.is_empty() {
        return Err(BuiltinError::task_invocation(
            "`unlock` requires at least one scope (or `--all`)",
        ));
    }

    Ok(UnlockRequest {
        output_json,
        unlock_all_flag,
        yes,
        scopes,
    })
}
