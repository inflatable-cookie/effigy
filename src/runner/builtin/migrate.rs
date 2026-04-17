use effigy_cli::HelpTopic;
use effigy_cli::TaskInvocation;
use std::path::Path;

#[path = "migrate/io.rs"]
mod io;
#[path = "migrate/model.rs"]
mod model;
#[path = "migrate/output.rs"]
mod output;
#[path = "migrate/plan.rs"]
mod plan;
#[path = "migrate/request.rs"]
mod request;

use super::command_spec::run_builtin_command;
use super::render_builtin_help_topic;
use crate::runner::error::RunnerError;

const CONFLICT_REASON_TASK_EXISTS: &str = "task already exists";
const BUILTIN_MIGRATE_NAME: &str = "migrate";

pub(super) fn run_builtin_migrate(
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_help_topic(HelpTopic::Migrate, "migrate", output_json),
        || request::parse_migrate_request(task, args),
        |request: model::MigrateRequest| {
            let plan = plan::build_migrate_plan(&request, target_root)?;
            output::render_migrate_output(&plan, request.output_json)
        },
    )
}
