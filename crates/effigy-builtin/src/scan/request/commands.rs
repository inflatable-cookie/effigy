use super::model::ScanCommand;

pub(super) fn parse_scan_command(arg: &str) -> Option<ScanCommand> {
    match arg {
        "god-files" => Some(ScanCommand::GodFiles),
        "duplicate-blocks" => Some(ScanCommand::DuplicateBlocks),
        _ => None,
    }
}

pub(super) fn scan_candidate_mode(args: &[String]) -> Option<ScanCommand> {
    args.iter()
        .find(|arg| !arg.starts_with('-'))
        .and_then(|arg| parse_scan_command(arg))
}

/// Subcommands retired in v0.15. Each names the replacement an operator can act
/// on instead of falling through to "unknown argument".
pub(super) fn retired_scan_subcommand(arg: &str) -> Option<&'static str> {
    match arg {
        "attention-markers" => Some(
            "`scan attention-markers` was removed in v0.15 and has no replacement; search TODO/FIXME markers with `rg -n 'TODO|FIXME' <path>` and delete the `[scan.attention_markers]` table (see CHANGELOG.md)",
        ),
        "comment-ratio" => Some(
            "`scan comment-ratio` was removed in v0.15 and has no replacement; use code review and delete the `[scan.comment_ratio]` table (see CHANGELOG.md)",
        ),
        "generated-assets" => Some(
            "`scan generated-assets` was removed in v0.15 and has no replacement; keep bulky generated output out of versioned paths with `.gitignore` and delete the `[scan.generated_assets]` table (see CHANGELOG.md)",
        ),
        "generated-in-src" => Some(
            "`scan generated-in-src` was removed in v0.15 and has no replacement; keep generated output outside source trees and delete the `[scan.generated_in_src]` table (see CHANGELOG.md)",
        ),
        "stale-suppressions" => Some(
            "`scan stale-suppressions` was removed in v0.15 and has no replacement; delete the `[scan.stale_suppressions]` table and let each tool report its own unused suppressions (see CHANGELOG.md)",
        ),
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
