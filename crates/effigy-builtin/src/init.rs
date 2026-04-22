use std::path::{Path, PathBuf};

use effigy_catalog::{StarterFile, StarterResolver};
use effigy_cli::{HelpTopic, TaskInvocation};
use effigy_core::fs_probe::PathPresenceCache;

use super::command_spec::run_builtin_command;
use super::render_builtin_help_topic;
use crate::BuiltinError;
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
    let starter = scaffold::load_starter(&starter_name)?;

    // Resolve per-file target paths once up front so both the conflict
    // check and the emission loop agree on the same paths.
    let planned: Vec<(PathBuf, &StarterFile)> = starter
        .files
        .iter()
        .map(|file| (target_root.join(&file.target), file))
        .collect();

    let mut probe = PathPresenceCache::new();
    let existing: Vec<&PathBuf> = planned
        .iter()
        .filter_map(|(path, _)| probe.exists(path).then_some(path))
        .collect();

    if !existing.is_empty() && !force && !dry_run {
        let listing = existing
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(BuiltinError::task_invocation(format!(
            "starter `{}` path(s) already exists: {}. Use `effigy init --force` to overwrite or `effigy init --dry-run` to preview.",
            starter_name, listing
        )));
    }

    let mut emitted = Vec::with_capacity(planned.len());
    for (path, file) in &planned {
        let existed = probe.exists(path);
        let mut wrote = false;
        if !dry_run {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        BuiltinError::task_invocation_failed_write(parent, error)
                    })?;
                }
            }
            std::fs::write(path, file.contents.as_bytes())
                .map_err(|error| BuiltinError::task_invocation_failed_write(path, error))?;
            wrote = true;
        }
        emitted.push(output::EmittedFile {
            target: file.target.clone(),
            path: path.clone(),
            contents: file.contents.clone(),
            existed,
            written: wrote,
        });
    }

    output::render_init_response(
        output_json,
        &starter,
        emitted,
        output::InitOutcome {
            written: !dry_run,
            dry_run,
        },
    )
}
