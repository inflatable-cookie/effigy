use super::model::ScanCommand;

pub(super) fn parse_scan_command(arg: &str) -> Option<ScanCommand> {
    match arg {
        "god-files" => Some(ScanCommand::GodFiles),
        "duplicate-blocks" => Some(ScanCommand::DuplicateBlocks),
        "comment-ratio" => Some(ScanCommand::CommentRatio),
        "generated-assets" => Some(ScanCommand::GeneratedAssets),
        "generated-in-src" => Some(ScanCommand::GeneratedInSrc),
        "attention-markers" => Some(ScanCommand::AttentionMarkers),
        "stale-suppressions" => Some(ScanCommand::StaleSuppressions),
        _ => None,
    }
}

pub(super) fn scan_candidate_mode(args: &[String]) -> Option<ScanCommand> {
    args.iter()
        .find(|arg| !arg.starts_with('-'))
        .and_then(|arg| parse_scan_command(arg))
}

/// Subcommands retired in v0.15 with the code graph. Each names the replacement
/// an operator can act on instead of falling through to "unknown argument".
pub(super) fn retired_scan_subcommand(arg: &str) -> Option<&'static str> {
    match arg {
        "boundary-violations" => Some(
            "`scan boundary-violations` was removed in v0.15 with the code graph and has no replacement; delete the `[scan.boundary_violations]` layers and rely on code review (see CHANGELOG.md)",
        ),
        "dead-code" => Some(
            "`scan dead-code` was removed in v0.15 with the code graph and has no replacement; delete the `[scan.dead_code]` table (see CHANGELOG.md)",
        ),
        "validation-gaps" => Some(
            "`scan validation-gaps` was removed in v0.15 with the code graph; use `effigy test --plan` to choose tests for changed files and delete the `[scan.validation_gaps]` table (see CHANGELOG.md)",
        ),
        _ => None,
    }
}
