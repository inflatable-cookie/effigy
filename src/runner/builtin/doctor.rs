use std::path::Path;

use crate::TaskInvocation;

use super::super::{run_doctor, RunnerError, TaskRuntimeArgs};
use super::{reject_verbose_root_for_builtin, unknown_builtin_args};

pub(super) fn run_builtin_doctor(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<String, RunnerError> {
    reject_verbose_root_for_builtin(&task.name, runtime_args)?;

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
        return Err(unknown_builtin_args(&task.name, &unknown));
    }

    run_doctor(crate::DoctorArgs {
        repo_override: Some(target_root.to_path_buf()),
        output_json,
        fix,
        verbose,
        explain,
    })
}
