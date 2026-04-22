use std::path::Path;

use effigy_catalog::StarterResolver;
use effigy_cli::{HelpTopic, TaskInvocation};
use effigy_core::fs_probe::PathPresenceCache;

use super::command_spec::run_builtin_command;
use super::render_builtin_help_topic;
use crate::BuiltinError;
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
) -> Result<Option<String>, BuiltinError> {
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
) -> Result<Option<String>, BuiltinError> {
    match request.mode {
        request::InitMode::List => run_list(request.output_json),
        request::InitMode::Emit { starter_name } => run_emit(
            starter_name,
            target_root,
            request.output_json,
            request.force,
            request.dry_run,
        ),
    }
}

fn run_list(output_json: bool) -> Result<Option<String>, BuiltinError> {
    let resolver = StarterResolver::new();
    let starters = resolver.list();
    output::render_init_list_response(output_json, starters)
}

fn run_emit(
    starter_name: String,
    target_root: &Path,
    output_json: bool,
    force: bool,
    dry_run: bool,
) -> Result<Option<String>, BuiltinError> {
    let scaffold = scaffold::render_starter_scaffold(&starter_name)?;
    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    let mut probe = PathPresenceCache::new();
    let manifest_exists = probe.exists(&manifest_path);
    if manifest_exists && !force && !dry_run {
        return Err(BuiltinError::task_invocation(format!(
            "{} already exists at {}. Use `effigy init --force` to overwrite or `effigy init --dry-run` to preview.",
            TASK_MANIFEST_FILE,
            manifest_path.display()
        )));
    }

    let mut written = false;
    if !dry_run {
        std::fs::write(&manifest_path, scaffold.as_bytes())
            .map_err(|error| BuiltinError::task_invocation_failed_write(&manifest_path, error))?;
        written = true;
    }

    output::render_init_response(
        output_json,
        &starter_name,
        &manifest_path,
        scaffold,
        output::InitOutcome {
            manifest_exists,
            written,
        },
    )
}
