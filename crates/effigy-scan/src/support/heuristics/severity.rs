use crate::model::{
    DuplicateBlockSeverity, DuplicateBlockThresholds, GodFileSeverity, GodFileThresholds,
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

pub fn severity_rank(severity: GodFileSeverity) -> usize {
    match severity {
        GodFileSeverity::Warning => 1,
        GodFileSeverity::High => 2,
        GodFileSeverity::Critical => 3,
    }
}

pub fn duplicate_block_severity_rank(severity: DuplicateBlockSeverity) -> usize {
    match severity {
        DuplicateBlockSeverity::Warning => 1,
        DuplicateBlockSeverity::High => 2,
        DuplicateBlockSeverity::Critical => 3,
    }
}
