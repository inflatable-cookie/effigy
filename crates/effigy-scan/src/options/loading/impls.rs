use crate::error::ScanError;
use effigy_manifest::config_sections::{ManifestDuplicateBlocksConfig, ManifestGodFilesConfig};

use super::super::super::model::{DuplicateBlockScanOptions, GodFileScanOptions};
use super::traits::{CommonManifestOptions, ManifestBackedScanOptions};

impl ManifestBackedScanOptions for GodFileScanOptions {
    type ManifestConfig = ManifestGodFilesConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_> {
        CommonManifestOptions {
            fail_on_findings: config.fail_on_findings,
            respect_gitignore: config.respect_gitignore,
            doctor_enabled: config.doctor,
            format: config.format,
            include: &config.include,
            exclude: &config.exclude,
            out: config.out.as_ref(),
        }
    }

    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig) {
        if config.warn.is_some() || config.high.is_some() || config.critical.is_some() {
            self.explicit_thresholds = true;
        }
        if let Some(value) = config.warn {
            self.thresholds.warn = value;
        }
        if let Some(value) = config.high {
            self.thresholds.high = value;
        }
        if let Some(value) = config.critical {
            self.thresholds.critical = value;
        }
    }

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
        self.validate()
    }
}

impl ManifestBackedScanOptions for DuplicateBlockScanOptions {
    type ManifestConfig = ManifestDuplicateBlocksConfig;

    fn common_manifest_options(config: &Self::ManifestConfig) -> CommonManifestOptions<'_> {
        CommonManifestOptions {
            fail_on_findings: config.fail_on_findings,
            respect_gitignore: config.respect_gitignore,
            doctor_enabled: config.doctor,
            format: config.format,
            include: &config.include,
            exclude: &config.exclude,
            out: config.out.as_ref(),
        }
    }

    fn apply_manifest_specific(&mut self, config: &Self::ManifestConfig) {
        if let Some(value) = config.warn {
            self.thresholds.warn = value;
        }
        if let Some(value) = config.high {
            self.thresholds.high = value;
        }
        if let Some(value) = config.critical {
            self.thresholds.critical = value;
        }
        if let Some(value) = config.min_occurrences {
            self.thresholds.min_occurrences = value;
        }
    }

    fn validate_manifest_options(&self) -> Result<(), ScanError> {
        self.validate()
    }
}
