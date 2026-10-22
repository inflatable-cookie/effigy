use super::super::constants::{
    DEFAULT_DUPLICATE_BLOCKS_CRITICAL, DEFAULT_DUPLICATE_BLOCKS_HIGH,
    DEFAULT_DUPLICATE_BLOCKS_MIN_OCCURRENCES, DEFAULT_DUPLICATE_BLOCKS_WARN,
    DEFAULT_GOD_FILES_CRITICAL, DEFAULT_GOD_FILES_HIGH, DEFAULT_GOD_FILES_WARN,
};
use super::super::model::{
    DuplicateBlockScanOptions, DuplicateBlockThresholds, GodFileScanOptions, GodFileThresholds,
    ScanRenderFormat,
};

impl Default for GodFileScanOptions {
    fn default() -> Self {
        Self {
            thresholds: GodFileThresholds {
                warn: DEFAULT_GOD_FILES_WARN,
                high: DEFAULT_GOD_FILES_HIGH,
                critical: DEFAULT_GOD_FILES_CRITICAL,
            },
            explicit_thresholds: false,
            fail_on_findings: false,
            respect_gitignore: true,
            doctor_enabled: true,
            include: Vec::new(),
            exclude: Vec::new(),
            format: ScanRenderFormat::Text,
            out: None,
        }
    }
}

impl Default for DuplicateBlockScanOptions {
    fn default() -> Self {
        Self {
            thresholds: DuplicateBlockThresholds {
                warn: DEFAULT_DUPLICATE_BLOCKS_WARN,
                high: DEFAULT_DUPLICATE_BLOCKS_HIGH,
                critical: DEFAULT_DUPLICATE_BLOCKS_CRITICAL,
                min_occurrences: DEFAULT_DUPLICATE_BLOCKS_MIN_OCCURRENCES,
            },
            fail_on_findings: false,
            respect_gitignore: true,
            doctor_enabled: false,
            include: Vec::new(),
            exclude: Vec::new(),
            format: ScanRenderFormat::Text,
            out: None,
        }
    }
}
