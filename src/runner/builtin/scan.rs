use std::path::Path;

use crate::TaskInvocation;

use super::command_spec::run_passthrough_builtin_command;
use super::{has_builtin_json_flag, render_builtin_help_text};
use crate::runner::error::RunnerError;
use effigy_manifest::LoadedCatalog;
use effigy_tasks::TaskRuntimeArgs;

mod execution;
mod help;
mod request;

use execution::run_scan_request;
use help::{
    render_attention_markers_help, render_comment_ratio_help, render_duplicate_blocks_help,
    render_generated_assets_help, render_generated_in_src_help, render_god_files_help,
    render_scan_help, render_stale_suppressions_help,
};
use request::{parse_scan_request, scan_candidate_mode, ScanCommand};

pub(super) fn run_builtin_scan(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    if runtime_args.passthrough.is_empty() {
        return render_builtin_help_text("scan", render_scan_help(), false).map(Some);
    }
    if runtime_args.passthrough.len() == 1 && has_builtin_json_flag(&runtime_args.passthrough) {
        return render_builtin_help_text("scan", render_scan_help(), true).map(Some);
    }
    run_passthrough_builtin_command(
        &task.name,
        runtime_args,
        |output_json| {
            let help = match scan_candidate_mode(&runtime_args.passthrough) {
                Some(ScanCommand::GodFiles) => render_god_files_help(),
                Some(ScanCommand::DuplicateBlocks) => render_duplicate_blocks_help(),
                Some(ScanCommand::CommentRatio) => render_comment_ratio_help(),
                Some(ScanCommand::GeneratedAssets) => render_generated_assets_help(),
                Some(ScanCommand::GeneratedInSrc) => render_generated_in_src_help(),
                Some(ScanCommand::AttentionMarkers) => render_attention_markers_help(),
                Some(ScanCommand::StaleSuppressions) => render_stale_suppressions_help(),
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
