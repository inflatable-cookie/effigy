use std::path::PathBuf;

use crate::TaskInvocation;

use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use super::super::ensure_no_unknown_builtin_args;
use super::{MigrateArgs, RunnerError};

pub(super) fn parse_migrate_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<MigrateArgs, RunnerError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut apply = false;
    let mut package_path: Option<PathBuf> = None;
    let mut script_filter = std::collections::BTreeSet::<String>::new();
    let unknown = parser.parse_loop_collect_unknown(|parser, arg| {
        if parser.consume_json_flag(arg, &mut output_json)
            || parser.consume_flag(arg, "--apply", &mut apply)
        {
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
    ensure_no_unknown_builtin_args(&task.name, &unknown)?;

    Ok(MigrateArgs {
        output_json,
        apply,
        package_path,
        script_filter,
    })
}
