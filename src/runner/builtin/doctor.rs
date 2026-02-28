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
    let mut verbose = false;
    let mut explain: Option<TaskInvocation> = None;
    let mut unknown = Vec::<String>::new();
    for arg in &runtime_args.passthrough {
        if let Some(request) = explain.as_mut() {
            request.args.push(arg.clone());
            continue;
        }
        match arg.as_str() {
            "--json" => output_json = true,
            "--fix" => fix = true,
            "--verbose" => verbose = true,
            "-h" | "--help" => unknown.push(arg.clone()),
            other if other.starts_with('-') => unknown.push(arg.clone()),
            other => {
                explain = Some(TaskInvocation {
                    name: other.to_owned(),
                    args: Vec::new(),
                });
            }
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
        verbose,
        explain,
    })
}
