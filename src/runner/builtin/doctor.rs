use std::path::Path;

use crate::TaskInvocation;

use super::super::{run_doctor, RunnerError, TaskRuntimeArgs};

pub(super) fn run_builtin_doctor(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<String, RunnerError> {
    if runtime_args.verbose_root {
        return Err(RunnerError::TaskInvocation(format!(
            "`--verbose-root` is not supported for built-in `{}`",
            task.name
        )));
    }

    let mut output_json = false;
    let mut fix = false;
    let mut unknown = Vec::<String>::new();
    for arg in &runtime_args.passthrough {
        match arg.as_str() {
            "--json" => output_json = true,
            "--fix" => fix = true,
            _ => unknown.push(arg.clone()),
        }
    }
    if !unknown.is_empty() {
        return Err(RunnerError::TaskInvocation(format!(
            "unknown argument(s) for built-in `{}`: {}",
            task.name,
            unknown.join(" ")
        )));
    }

    run_doctor(crate::DoctorArgs {
        repo_override: Some(target_root.to_path_buf()),
        output_json,
        fix,
    })
}
