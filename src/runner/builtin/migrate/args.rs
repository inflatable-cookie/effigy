use std::path::PathBuf;

use crate::TaskInvocation;

use super::{MigrateArgs, RunnerError};
use super::super::arg_parser::BuiltinArgParser;
use super::super::unknown_builtin_args;

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
        match arg {
            "--json" => {
                parser.bool_flag(&mut output_json);
            }
            "--help" | "-h" => {
                parser.bool_flag(&mut help);
            }
            "--apply" => {
                parser.bool_flag(&mut apply);
            }
            "--from" => {
                let value = parser.string_flag_value("`--from` requires a file path")?;
                package_path = Some(PathBuf::from(value));
            }
            "--script" => {
                let value = parser.string_flag_value("`--script` requires a script name")?;
                script_filter.insert(value);
            }
            unknown => {
                return Err(unknown_builtin_args(&task.name, &[unknown.to_owned()]));
            }
        }
    }
    Ok(MigrateArgs {
        output_json,
        help,
        apply,
        package_path,
        script_filter,
    })
}
