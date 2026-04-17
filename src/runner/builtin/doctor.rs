use std::path::Path;

use effigy_cli::{HelpTopic, TaskInvocation};

use super::command_spec::run_passthrough_builtin_command;
use super::render_builtin_help_topic;
use crate::runner::builtin_ports::BuiltinRuntimePorts;
use crate::runner::error::RunnerError;
use effigy_tasks::TaskRuntimeArgs;
#[path = "doctor/request.rs"]
mod request;

pub(super) fn run_builtin_doctor(
    ports: &dyn BuiltinRuntimePorts,
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
            ports
                .run_doctor(effigy_cli::DoctorArgs {
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
