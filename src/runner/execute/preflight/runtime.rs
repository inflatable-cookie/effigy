use super::super::super::util::parse_task_runtime_args;
use crate::runner::error::RunnerError;
use crate::runner::model::catalog::TaskRuntimeArgs;

pub(super) fn prepare_execution_runtime_args(
    args: &[String],
) -> Result<(TaskRuntimeArgs, TaskRuntimeArgs, bool), RunnerError> {
    let runtime_args_raw = parse_task_runtime_args(args)?;
    let (passthrough_without_json, output_json) =
        strip_task_json_flag(&runtime_args_raw.passthrough);
    let runtime_args_exec = TaskRuntimeArgs {
        repo_override: runtime_args_raw.repo_override.clone(),
        verbose_root: runtime_args_raw.verbose_root,
        env_schema_override: runtime_args_raw.env_schema_override.clone(),
        passthrough: passthrough_without_json,
    };
    Ok((runtime_args_raw, runtime_args_exec, output_json))
}

fn strip_task_json_flag(args: &[String]) -> (Vec<String>, bool) {
    let mut stripped = Vec::with_capacity(args.len());
    let mut json_mode = false;
    let mut passthrough_mode = false;
    for arg in args {
        if arg == "--" {
            passthrough_mode = true;
            stripped.push(arg.clone());
            continue;
        }
        if !passthrough_mode && arg == "--json" {
            json_mode = true;
            continue;
        }
        stripped.push(arg.clone());
    }
    (stripped, json_mode)
}
