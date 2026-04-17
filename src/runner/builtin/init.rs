use std::path::Path;

use effigy_cli::{HelpTopic, TaskInvocation};
use effigy_core::fs_probe::PathPresenceCache;

use super::command_spec::run_builtin_command;
use super::render_builtin_help_topic;
use crate::runner::error::RunnerError;
use effigy_manifest::TASK_MANIFEST_FILE;
#[path = "init/output.rs"]
mod output;
#[path = "init/request.rs"]
mod request;
#[path = "init/scaffold.rs"]
mod scaffold;

pub(super) fn run_builtin_init(
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_help_topic(HelpTopic::Init, "init", output_json),
        || request::parse_init_request(task, args),
        |request: request::InitRequest| run_init_request(request, target_root),
    )
}

fn run_init_request(
    request: request::InitRequest,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let scaffold = scaffold::render_init_scaffold();
    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    let mut probe = PathPresenceCache::new();
    let manifest_exists = probe.exists(&manifest_path);
    if manifest_exists && !request.force && !request.dry_run {
        return Err(RunnerError::task_invocation(format!(
            "{} already exists at {}. Use `effigy init --force` to overwrite or `effigy init --dry-run` to preview.",
            TASK_MANIFEST_FILE,
            manifest_path.display()
        )));
    }

    let mut written = false;
    if !request.dry_run {
        std::fs::write(&manifest_path, scaffold.as_bytes())
            .map_err(|error| RunnerError::task_invocation_failed_write(&manifest_path, error))?;
        written = true;
    }

    output::render_init_response(
        request.output_json,
        &manifest_path,
        scaffold,
        output::InitOutcome {
            manifest_exists,
            written,
        },
    )
}
