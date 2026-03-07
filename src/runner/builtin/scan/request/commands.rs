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
