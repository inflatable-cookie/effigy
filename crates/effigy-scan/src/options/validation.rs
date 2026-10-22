use crate::error::ScanError;

use super::super::model::{DuplicateBlockScanOptions, GodFileScanOptions};

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
