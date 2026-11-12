use crate::runner::command_context::resolve_active_command_context;
use crate::runner::container_command::RunnerError;
use effigy_runtime::read::run_container_logs;

pub(in crate::runner::container_command) fn run_container_logs_command(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    service: Option<&str>,
    follow: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(repo_override)?;
    run_container_logs(
        &context.resolved.resolved_root,
        name,
        service,
        follow,
        output_json,
    )
    .map_err(Into::into)
}
