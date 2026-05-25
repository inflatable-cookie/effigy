use crate::error::ScanError;

use super::super::model::{
    AttentionMarkerScanOptions, BoundaryViolationScanOptions, CommentRatioScanOptions,
    DeadCodeScanOptions, DuplicateBlockScanOptions, GeneratedAssetScanOptions,
    GeneratedInSrcScanOptions, GodFileScanOptions, StaleSuppressionScanOptions,
    ValidationGapScanOptions,
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

impl BoundaryViolationScanOptions {
    pub fn validate(&self) -> Result<(), ScanError> {
        for (layer_name, layer) in &self.layers {
            if layer.paths.is_empty() {
                return Err(ScanError::invocation(format!(
                    "`scan.boundary_violations.layers.{layer_name}` requires at least one configured path glob"
                )));
            }
            if layer.paths.iter().any(|value| value.trim().is_empty()) {
                return Err(ScanError::invocation(format!(
                    "`scan.boundary_violations.layers.{layer_name}.paths` must contain non-empty glob strings"
                )));
            }
            for dependency in &layer.may_depend_on {
                if !self.layers.contains_key(dependency) {
                    return Err(ScanError::invocation(format!(
                        "`scan.boundary_violations.layers.{layer_name}.may_depend_on` references unknown layer `{dependency}`"
                    )));
                }
            }
        }
        Ok(())
    }
}

impl DeadCodeScanOptions {
    pub fn validate(&self) -> Result<(), ScanError> {
        if self.allow_paths.iter().any(|value| value.trim().is_empty()) {
            return Err(ScanError::invocation(
                "`scan.dead_code.allow_paths` must contain non-empty glob strings",
            ));
        }
        if self
            .allow_symbols
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err(ScanError::invocation(
                "`scan.dead_code.allow_symbols` must contain non-empty glob strings",
            ));
        }
        Ok(())
    }
}

impl ValidationGapScanOptions {
    pub fn validate(&self) -> Result<(), ScanError> {
        if self.allow_paths.iter().any(|value| value.trim().is_empty()) {
            return Err(ScanError::invocation(
                "`scan.validation_gaps.allow_paths` must contain non-empty glob strings",
            ));
        }
        if self.hotspot_threshold == 0 {
            return Err(ScanError::invocation(
                "`scan.validation_gaps.hotspot_threshold` must be greater than zero",
            ));
        }
        if self.affected_depth == 0 {
            return Err(ScanError::invocation(
                "`scan.validation_gaps.affected_depth` must be greater than zero",
            ));
        }
        Ok(())
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
