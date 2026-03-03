use std::path::Path;

use crate::TaskInvocation;

use super::super::{run_doctor, RunnerError, TaskRuntimeArgs};
use super::arg_parser::BuiltinArgParser;
use super::{ensure_no_unknown_builtin_args, reject_verbose_root_for_builtin};

pub(super) fn run_builtin_doctor(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<String, RunnerError> {
    reject_verbose_root_for_builtin(&task.name, runtime_args)?;

    let mut parser = BuiltinArgParser::new(&runtime_args.passthrough);
    let mut output_json = false;
    let mut fix = false;
    let mut verbose = false;
    let mut explain: Option<TaskInvocation> = None;
    let mut unknown = Vec::<String>::new();
    while let Some(arg) = parser.next() {
        if parser.consume_json_flag(arg, &mut output_json)
            || parser.consume_flag(arg, "--fix", &mut fix)
            || parser.consume_flag(arg, "--verbose", &mut verbose)
        {
            continue;
        }
        if arg.starts_with('-') {
            unknown.push(arg.to_owned());
            continue;
        }
        explain = Some(TaskInvocation {
            name: arg.to_owned(),
            args: parser.remaining().to_vec(),
        });
        break;
    }
    ensure_no_unknown_builtin_args(&task.name, &unknown)?;

    run_doctor(crate::DoctorArgs {
        repo_override: Some(target_root.to_path_buf()),
        output_json,
        fix,
        verbose,
        explain,
    })
}
