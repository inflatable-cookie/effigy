use std::path::Path;

use crate::{HelpTopic, TaskInvocation};

use super::super::doctor::run_doctor;
use super::super::model::catalog::TaskRuntimeArgs;
use super::command_spec::run_passthrough_builtin_command;
use super::render_builtin_help_topic;
use crate::runner::error::RunnerError;
#[path = "doctor/request.rs"]
mod request;

pub(super) fn run_builtin_doctor(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    run_passthrough_builtin_command(
        &task.name,
        runtime_args,
        |output_json| render_builtin_help_topic(HelpTopic::Doctor, "doctor", output_json),
        |args| request::parse_doctor_request(task, args),
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
