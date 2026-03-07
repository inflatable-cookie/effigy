use super::super::api::{ScanThresholdOverrideOptions, ScanThresholds};
use crate::runner::scan::model::{
    DuplicateBlockScanOptions, DuplicateBlockThresholds, GeneratedAssetScanOptions,
    GeneratedAssetThresholds, GeneratedInSrcScanOptions, GeneratedInSrcThresholds,
    GodFileScanOptions, GodFileThresholds,
};

impl ScanThresholdOverrideOptions for GodFileScanOptions {
    type Thresholds = GodFileThresholds;

    fn thresholds_mut(&mut self) -> &mut Self::Thresholds {
        &mut self.thresholds
    }
}

impl ScanThresholdOverrideOptions for GeneratedAssetScanOptions {
    type Thresholds = GeneratedAssetThresholds;

    fn thresholds_mut(&mut self) -> &mut Self::Thresholds {
        &mut self.thresholds
    }
}

impl ScanThresholdOverrideOptions for GeneratedInSrcScanOptions {
    type Thresholds = GeneratedInSrcThresholds;

    fn thresholds_mut(&mut self) -> &mut Self::Thresholds {
        &mut self.thresholds
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

impl ScanThresholds for GeneratedAssetThresholds {
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

impl ScanThresholds for GeneratedInSrcThresholds {
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
