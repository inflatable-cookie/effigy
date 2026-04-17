use std::path::PathBuf;

use effigy_cli::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use super::model::MigrateRequest;
use crate::BuiltinError;

pub(super) fn parse_migrate_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<MigrateRequest, BuiltinError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut apply = false;
    let mut package_path: Option<PathBuf> = None;
    let mut script_filter = std::collections::BTreeSet::<String>::new();
    parser.parse_loop_require_no_unknown(&task.name, |parser, arg| {
        if parser.consume_any_bool_flag(
            arg,
            &mut [("--json", &mut output_json), ("--apply", &mut apply)],
        ) {
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--from" {
            let value = parser.flag_string_value("--from", "a file path")?;
            package_path = Some(PathBuf::from(value));
            return Ok(ParseLoopAction::Handled);
        }
        if arg == "--script" {
            let value = parser.flag_string_value("--script", "a script name")?;
            script_filter.insert(value);
            return Ok(ParseLoopAction::Handled);
        }
        Ok(ParseLoopAction::Unknown)
    })?;

    Ok(MigrateRequest {
        output_json,
        apply,
        package_path,
        script_filter,
    })
}
