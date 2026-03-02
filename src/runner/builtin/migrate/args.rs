use std::path::PathBuf;

use crate::TaskInvocation;

use super::{MigrateArgs, RunnerError};

pub(super) fn parse_migrate_args(
    task: &TaskInvocation,
    args: &[String],
) -> Result<MigrateArgs, RunnerError> {
    let mut output_json = false;
    let mut help = false;
    let mut apply = false;
    let mut package_path: Option<PathBuf> = None;
    let mut script_filter = std::collections::BTreeSet::<String>::new();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                output_json = true;
                i += 1;
            }
            "--help" | "-h" => {
                help = true;
                i += 1;
            }
            "--apply" => {
                apply = true;
                i += 1;
            }
            "--from" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--from` requires a file path".to_owned(),
                    ));
                };
                package_path = Some(PathBuf::from(value));
                i += 2;
            }
            "--script" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--script` requires a script name".to_owned(),
                    ));
                };
                script_filter.insert(value.clone());
                i += 2;
            }
            unknown => {
                return Err(RunnerError::TaskInvocation(format!(
                    "unknown argument(s) for built-in `{}`: {}",
                    task.name, unknown
                )));
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
