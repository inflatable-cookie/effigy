use crate::error::ScanError;

use super::super::model::{
    AttentionMarkerScanOptions, CommentRatioScanOptions, DuplicateBlockScanOptions,
    GeneratedAssetScanOptions, GeneratedInSrcScanOptions, GodFileScanOptions,
    StaleSuppressionScanOptions,
};

impl GodFileScanOptions {
    pub fn validate(&self) -> Result<(), ScanError> {
        validate_thresholds(
            "scan.god_files",
            self.thresholds.warn,
            self.thresholds.high,
            self.thresholds.critical,
        )
    }
}

impl GeneratedAssetScanOptions {
    pub fn validate(&self) -> Result<(), ScanError> {
        validate_thresholds(
            "scan.generated_assets",
            self.thresholds.warn,
            self.thresholds.high,
            self.thresholds.critical,
        )
    }
}

impl GeneratedInSrcScanOptions {
    pub fn validate(&self) -> Result<(), ScanError> {
        validate_thresholds(
            "scan.generated_in_src",
            self.thresholds.warn,
            self.thresholds.high,
            self.thresholds.critical,
        )?;
        if self.source_roots.is_empty() {
            return Err(ScanError::invocation(
                "`scan.generated_in_src.source_roots` requires at least one configured glob",
            ));
        }
        if self
            .source_roots
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err(ScanError::invocation(
                "`scan.generated_in_src.source_roots` must contain non-empty glob strings",
            ));
        }
        Ok(())
    }
}

impl AttentionMarkerScanOptions {
    pub fn validate(&self) -> Result<(), ScanError> {
        let total_patterns =
            self.patterns.warning.len() + self.patterns.high.len() + self.patterns.critical.len();
        if total_patterns == 0 {
            return Err(ScanError::invocation(
                "`scan.attention_markers` requires at least one configured marker",
            ));
        }
        if self
            .patterns
            .warning
            .iter()
            .chain(self.patterns.high.iter())
            .chain(self.patterns.critical.iter())
            .any(|value| value.trim().is_empty())
        {
            return Err(ScanError::invocation(
                "`scan.attention_markers` markers must be non-empty strings",
            ));
        }
        Ok(())
    }
}

impl DuplicateBlockScanOptions {
    pub fn validate(&self) -> Result<(), ScanError> {
        validate_thresholds(
            "scan.duplicate_blocks",
            self.thresholds.warn,
            self.thresholds.high,
            self.thresholds.critical,
        )?;
        if self.thresholds.min_occurrences < 2 {
            return Err(ScanError::invocation(
                "`scan.duplicate_blocks.min_occurrences` must be at least 2",
            ));
        }
        Ok(())
    }
}

impl CommentRatioScanOptions {
    pub fn validate(&self) -> Result<(), ScanError> {
        validate_ratio_thresholds(
            "scan.comment_ratio",
            self.thresholds.warn,
            self.thresholds.high,
            self.thresholds.critical,
        )?;
        if self.thresholds.min_code_lines == 0 {
            return Err(ScanError::invocation(
                "`scan.comment_ratio.min_code_lines` must be greater than zero",
            ));
        }
        Ok(())
    }
}

impl StaleSuppressionScanOptions {
    pub fn validate(&self) -> Result<(), ScanError> {
        let total_patterns =
            self.patterns.warning.len() + self.patterns.high.len() + self.patterns.critical.len();
        if total_patterns == 0 {
            return Err(ScanError::invocation(
                "`scan.stale_suppressions` requires at least one configured marker",
            ));
        }
        if self
            .patterns
            .warning
            .iter()
            .chain(self.patterns.high.iter())
            .chain(self.patterns.critical.iter())
            .any(|value| value.trim().is_empty())
        {
            return Err(ScanError::invocation(
                "`scan.stale_suppressions` markers must be non-empty strings",
            ));
        }
        Ok(())
    }
}

fn validate_thresholds(
    section_name: &str,
    warn: usize,
    high: usize,
    critical: usize,
) -> Result<(), ScanError> {
    if warn == 0 || high == 0 || critical == 0 {
        return Err(ScanError::invocation(format!(
            "`{section_name}` thresholds must be greater than zero"
        )));
    }
    if warn > high || high > critical {
        return Err(ScanError::invocation(format!(
            "`{section_name}` thresholds must be ordered `warn <= high <= critical`"
        )));
    }
    Ok(())
}

fn validate_ratio_thresholds(
    section_name: &str,
    warn: f64,
    high: f64,
    critical: f64,
) -> Result<(), ScanError> {
    if warn <= 0.0 || high <= 0.0 || critical <= 0.0 {
        return Err(ScanError::invocation(format!(
            "`{section_name}` thresholds must be greater than zero"
        )));
    }
    if warn > high || high > critical {
        return Err(ScanError::invocation(format!(
            "`{section_name}` thresholds must be ordered `warn <= high <= critical`"
        )));
    }
    Ok(())
}
