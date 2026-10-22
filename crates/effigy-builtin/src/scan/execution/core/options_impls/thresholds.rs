use super::super::api::{ScanThresholdOverrideOptions, ScanThresholds};
use effigy_scan::{
    DuplicateBlockScanOptions, DuplicateBlockThresholds, GodFileScanOptions, GodFileThresholds,
};

impl ScanThresholdOverrideOptions for GodFileScanOptions {
    type Thresholds = GodFileThresholds;

    fn thresholds_mut(&mut self) -> &mut Self::Thresholds {
        &mut self.thresholds
    }

    fn mark_thresholds_configured(&mut self) {
        self.explicit_thresholds = true;
    }
}

impl ScanThresholdOverrideOptions for DuplicateBlockScanOptions {
    type Thresholds = DuplicateBlockThresholds;

    fn thresholds_mut(&mut self) -> &mut Self::Thresholds {
        &mut self.thresholds
    }
}

impl ScanThresholds for GodFileThresholds {
    fn warn_mut(&mut self) -> &mut usize {
        &mut self.warn
    }
    fn high_mut(&mut self) -> &mut usize {
        &mut self.high
    }
    fn critical_mut(&mut self) -> &mut usize {
        &mut self.critical
    }
}

impl ScanThresholds for DuplicateBlockThresholds {
    fn warn_mut(&mut self) -> &mut usize {
        &mut self.warn
    }
    fn high_mut(&mut self) -> &mut usize {
        &mut self.high
    }
    fn critical_mut(&mut self) -> &mut usize {
        &mut self.critical
    }
}
