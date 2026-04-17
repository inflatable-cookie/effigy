use crate::model::{
    AttentionMarkerSeverity, CommentRatioSeverity, CommentRatioThresholds, DuplicateBlockSeverity,
    DuplicateBlockThresholds, GeneratedAssetSeverity, GeneratedAssetThresholds,
    GeneratedInSrcCategory, GeneratedInSrcSeverity, GeneratedInSrcThresholds, GodFileSeverity,
    GodFileThresholds, StaleSuppressionSeverity,
};

pub fn classify_severity(
    code_lines: usize,
    thresholds: &GodFileThresholds,
) -> Option<GodFileSeverity> {
    if code_lines >= thresholds.critical {
        return Some(GodFileSeverity::Critical);
    }
    if code_lines >= thresholds.high {
        return Some(GodFileSeverity::High);
    }
    if code_lines >= thresholds.warn {
        return Some(GodFileSeverity::Warning);
    }
    None
}

pub fn classify_generated_asset_severity(
    bytes: usize,
    thresholds: &GeneratedAssetThresholds,
) -> Option<GeneratedAssetSeverity> {
    if bytes >= thresholds.critical {
        return Some(GeneratedAssetSeverity::Critical);
    }
    if bytes >= thresholds.high {
        return Some(GeneratedAssetSeverity::High);
    }
    if bytes >= thresholds.warn {
        return Some(GeneratedAssetSeverity::Warning);
    }
    None
}

pub fn classify_generated_in_src_severity(
    bytes: usize,
    thresholds: &GeneratedInSrcThresholds,
) -> Option<GeneratedInSrcSeverity> {
    if bytes >= thresholds.critical {
        return Some(GeneratedInSrcSeverity::Critical);
    }
    if bytes >= thresholds.high {
        return Some(GeneratedInSrcSeverity::High);
    }
    if bytes >= thresholds.warn {
        return Some(GeneratedInSrcSeverity::Warning);
    }
    None
}

pub fn classify_duplicate_block_severity(
    block_lines: usize,
    thresholds: &DuplicateBlockThresholds,
) -> Option<DuplicateBlockSeverity> {
    if block_lines >= thresholds.critical {
        return Some(DuplicateBlockSeverity::Critical);
    }
    if block_lines >= thresholds.high {
        return Some(DuplicateBlockSeverity::High);
    }
    if block_lines >= thresholds.warn {
        return Some(DuplicateBlockSeverity::Warning);
    }
    None
}

pub fn classify_comment_ratio_severity(
    ratio: f64,
    thresholds: &CommentRatioThresholds,
) -> Option<CommentRatioSeverity> {
    if ratio >= thresholds.critical {
        return Some(CommentRatioSeverity::Critical);
    }
    if ratio >= thresholds.high {
        return Some(CommentRatioSeverity::High);
    }
    if ratio >= thresholds.warn {
        return Some(CommentRatioSeverity::Warning);
    }
    None
}

pub fn severity_rank(severity: GodFileSeverity) -> usize {
    match severity {
        GodFileSeverity::Warning => 1,
        GodFileSeverity::High => 2,
        GodFileSeverity::Critical => 3,
    }
}

pub fn generated_asset_severity_rank(severity: GeneratedAssetSeverity) -> usize {
    match severity {
        GeneratedAssetSeverity::Warning => 1,
        GeneratedAssetSeverity::High => 2,
        GeneratedAssetSeverity::Critical => 3,
    }
}

pub fn generated_in_src_severity_rank(severity: GeneratedInSrcSeverity) -> usize {
    match severity {
        GeneratedInSrcSeverity::Warning => 1,
        GeneratedInSrcSeverity::High => 2,
        GeneratedInSrcSeverity::Critical => 3,
    }
}

pub fn generated_in_src_category_rank(category: GeneratedInSrcCategory) -> usize {
    match category {
        GeneratedInSrcCategory::ContentMarker => 4,
        GeneratedInSrcCategory::BundledArtifact => 3,
        GeneratedInSrcCategory::GeneratedFilename => 2,
        GeneratedInSrcCategory::GeneratedPath => 1,
    }
}

pub fn attention_marker_severity_rank(severity: AttentionMarkerSeverity) -> usize {
    match severity {
        AttentionMarkerSeverity::Warning => 1,
        AttentionMarkerSeverity::High => 2,
        AttentionMarkerSeverity::Critical => 3,
    }
}

pub fn duplicate_block_severity_rank(severity: DuplicateBlockSeverity) -> usize {
    match severity {
        DuplicateBlockSeverity::Warning => 1,
        DuplicateBlockSeverity::High => 2,
        DuplicateBlockSeverity::Critical => 3,
    }
}

pub fn comment_ratio_severity_rank(severity: CommentRatioSeverity) -> usize {
    match severity {
        CommentRatioSeverity::Warning => 1,
        CommentRatioSeverity::High => 2,
        CommentRatioSeverity::Critical => 3,
    }
}

pub fn stale_suppression_severity_rank(severity: StaleSuppressionSeverity) -> usize {
    match severity {
        StaleSuppressionSeverity::Warning => 1,
        StaleSuppressionSeverity::High => 2,
        StaleSuppressionSeverity::Critical => 3,
    }
}
