use std::path::Path;

use crate::{HelpTopic, TaskInvocation};

use super::super::{run_doctor, RunnerError, TaskRuntimeArgs};
use super::command_spec::run_builtin_command;
use super::{reject_verbose_root_for_builtin, render_builtin_help_topic};
#[path = "doctor/request.rs"]
mod request;

pub(super) fn run_builtin_doctor(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    reject_verbose_root_for_builtin(&task.name, runtime_args)?;
    run_builtin_command(
        &runtime_args.passthrough,
        |output_json| render_builtin_help_topic(HelpTopic::Doctor, "doctor", output_json),
        || request::parse_doctor_request(task, &runtime_args.passthrough),
        |request: request::DoctorRequest| {
            run_doctor(crate::DoctorArgs {
                repo_override: Some(target_root.to_path_buf()),
                output_json: request.output_json,
                fix: request.fix,
                verbose: request.verbose,
                explain: request.explain,
            })
            .map(Some)
        },
    )
}
