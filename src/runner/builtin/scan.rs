use std::path::Path;

use crate::TaskInvocation;

use super::super::{LoadedCatalog, RunnerError, TaskRuntimeArgs};
use super::command_spec::run_passthrough_builtin_command;
use super::render_builtin_help_text;

mod execution;
mod help;
mod request;

use execution::run_scan_request;
use help::{render_generated_assets_help, render_god_files_help, render_scan_help};
use request::{parse_scan_request, scan_candidate_mode, ScanCommand};

pub(super) fn run_builtin_scan(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    run_passthrough_builtin_command(
        &task.name,
        runtime_args,
        |output_json| {
            let help = match scan_candidate_mode(&runtime_args.passthrough) {
                Some(ScanCommand::GodFiles) => render_god_files_help(),
                Some(ScanCommand::GeneratedAssets) => render_generated_assets_help(),
                None => render_scan_help(),
            };
            render_builtin_help_text("scan", help, output_json)
        },
        |args| parse_scan_request(task, args),
        |request| run_scan_request(request, target_root, catalogs),
    )
}

#[cfg(test)]
#[path = "scan/tests.rs"]
mod tests;
