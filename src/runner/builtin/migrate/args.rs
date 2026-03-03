use std::path::PathBuf;

use crate::TaskInvocation;

use super::super::arg_parser::BuiltinArgParser;
use super::super::unknown_builtin_arg;
use super::{MigrateArgs, RunnerError};

pub(super) fn parse_migrate_args(
    task: &TaskInvocation,
    args: &[String],
) -> Result<MigrateArgs, RunnerError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut help = false;
    let mut apply = false;
    let mut package_path: Option<PathBuf> = None;
    let mut script_filter = std::collections::BTreeSet::<String>::new();
    while let Some(arg) = parser.next() {
        if parser.consume_json_flag(arg, &mut output_json)
            || parser.consume_help_flag(arg, &mut help)
            || parser.consume_flag(arg, "--apply", &mut apply)
        {
            continue;
        }
        if arg == "--from" {
            let value = parser.string_flag_value("`--from` requires a file path")?;
            package_path = Some(PathBuf::from(value));
            continue;
        }
        if arg == "--script" {
            let value = parser.string_flag_value("`--script` requires a script name")?;
            script_filter.insert(value);
            continue;
        }
        return Err(unknown_builtin_arg(&task.name, arg));
    }
    Ok(MigrateArgs {
        output_json,
        help,
        apply,
        package_path,
        script_filter,
    })
}
