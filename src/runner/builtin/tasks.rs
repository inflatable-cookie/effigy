use std::path::Path;

use crate::{TaskInvocation, TasksArgs};

use super::super::{run_tasks, RunnerError, TaskRuntimeArgs};
use super::arg_parser::BuiltinArgParser;
use super::{ensure_no_unknown_builtin_args, reject_verbose_root_for_builtin};

pub(super) fn run_builtin_tasks(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
    catalogs_compat_alias: bool,
) -> Result<String, RunnerError> {
    reject_verbose_root_for_builtin(&task.name, runtime_args)?;

    let mut parser = BuiltinArgParser::new(&runtime_args.passthrough);
    let mut task_name: Option<String> = None;
    let mut resolve_selector: Option<String> = None;
    let mut output_json = false;
    let mut pretty_json = true;
    let mut unknown = Vec::<String>::new();
    while let Some(arg) = parser.next() {
        if parser.consume_json_flag(arg, &mut output_json) {
            continue;
        }
        if arg == "--task" {
            let value = parser.next_value("task argument --task requires a value")?;
            task_name = Some(value.to_owned());
            continue;
        }
        if arg == "--resolve" {
            let value = parser.next_value(&format!(
                "{} argument --resolve requires a value",
                task.name
            ))?;
            resolve_selector = Some(value.to_owned());
            continue;
        }
        if arg == "--pretty" {
            pretty_json = parser.bool_literal_flag_value(
                &format!(
                    "{} argument --pretty requires a value (`true` or `false`)",
                    task.name
                ),
                |value| {
                    format!(
                        "{} argument --pretty value `{value}` is invalid (expected `true` or `false`)",
                        task.name
                    )
                },
            )?;
            continue;
        }
        unknown.push(arg.to_owned());
    }
    ensure_no_unknown_builtin_args(&task.name, &unknown)?;

    if !output_json && !pretty_json {
        return Err(RunnerError::TaskInvocation(format!(
            "`--pretty` is only supported together with `--json` for built-in `{}`",
            task.name
        )));
    }

    let _ = catalogs_compat_alias;
    run_tasks(TasksArgs {
        repo_override: Some(target_root.to_path_buf()),
        task_name,
        resolve_selector,
        output_json,
        pretty_json,
    })
}
