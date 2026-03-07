use super::super::super::super::super::scan::model::{
    AttentionMarkerScanOptions, CommentRatioScanOptions, DuplicateBlockScanOptions,
    DuplicateBlockThresholds, GeneratedAssetScanOptions, GeneratedAssetThresholds,
    GeneratedInSrcScanOptions, GeneratedInSrcThresholds, GodFileScanOptions, GodFileThresholds,
    ScanRenderFormat, StaleSuppressionScanOptions,
};
use super::api::{ScanCommonOptions, ScanThresholdOverrideOptions, ScanThresholds};
use crate::runner::error::RunnerError;

impl ScanCommonOptions for GodFileScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        GodFileScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
    }
    fn fail_on_findings_mut(&mut self) -> &mut bool {
        &mut self.fail_on_findings
    }
    fn respect_gitignore_mut(&mut self) -> &mut bool {
        &mut self.respect_gitignore
    }
    fn include_mut(&mut self) -> &mut Vec<String> {
        &mut self.include
    }
    fn exclude_mut(&mut self) -> &mut Vec<String> {
        &mut self.exclude
    }
}

impl ScanCommonOptions for GeneratedAssetScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        GeneratedAssetScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
    }
    fn fail_on_findings_mut(&mut self) -> &mut bool {
        &mut self.fail_on_findings
    }
    fn respect_gitignore_mut(&mut self) -> &mut bool {
        &mut self.respect_gitignore
    }
    fn include_mut(&mut self) -> &mut Vec<String> {
        &mut self.include
    }
    fn exclude_mut(&mut self) -> &mut Vec<String> {
        &mut self.exclude
    }
}

impl ScanCommonOptions for GeneratedInSrcScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        GeneratedInSrcScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
    }
    fn fail_on_findings_mut(&mut self) -> &mut bool {
        &mut self.fail_on_findings
    }
    fn respect_gitignore_mut(&mut self) -> &mut bool {
        &mut self.respect_gitignore
    }
    fn include_mut(&mut self) -> &mut Vec<String> {
        &mut self.include
    }
    fn exclude_mut(&mut self) -> &mut Vec<String> {
        &mut self.exclude
    }
}

impl ScanCommonOptions for DuplicateBlockScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        DuplicateBlockScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
    }
    fn fail_on_findings_mut(&mut self) -> &mut bool {
        &mut self.fail_on_findings
    }
    fn respect_gitignore_mut(&mut self) -> &mut bool {
        &mut self.respect_gitignore
    }
    fn include_mut(&mut self) -> &mut Vec<String> {
        &mut self.include
    }
    fn exclude_mut(&mut self) -> &mut Vec<String> {
        &mut self.exclude
    }
}

impl ScanCommonOptions for CommentRatioScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        CommentRatioScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
    }
    fn fail_on_findings_mut(&mut self) -> &mut bool {
        &mut self.fail_on_findings
    }
    fn respect_gitignore_mut(&mut self) -> &mut bool {
        &mut self.respect_gitignore
    }
    fn include_mut(&mut self) -> &mut Vec<String> {
        &mut self.include
    }
    fn exclude_mut(&mut self) -> &mut Vec<String> {
        &mut self.exclude
    }
}

impl ScanCommonOptions for AttentionMarkerScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        AttentionMarkerScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
    }
    fn fail_on_findings_mut(&mut self) -> &mut bool {
        &mut self.fail_on_findings
    }
    fn respect_gitignore_mut(&mut self) -> &mut bool {
        &mut self.respect_gitignore
    }
    fn include_mut(&mut self) -> &mut Vec<String> {
        &mut self.include
    }
    fn exclude_mut(&mut self) -> &mut Vec<String> {
        &mut self.exclude
    }
}

impl ScanCommonOptions for StaleSuppressionScanOptions {
    fn format(&self) -> ScanRenderFormat {
        self.format
    }
    fn output_path(&self) -> Option<&String> {
        self.out.as_ref()
    }
    fn fail_on_findings(&self) -> bool {
        self.fail_on_findings
    }
    fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }
    fn validate(&self) -> Result<(), RunnerError> {
        StaleSuppressionScanOptions::validate(self)
    }
    fn format_mut(&mut self) -> &mut ScanRenderFormat {
        &mut self.format
    }
    fn fail_on_findings_mut(&mut self) -> &mut bool {
        &mut self.fail_on_findings
    }
    fn respect_gitignore_mut(&mut self) -> &mut bool {
        &mut self.respect_gitignore
    }
    fn include_mut(&mut self) -> &mut Vec<String> {
        &mut self.include
    }
    fn exclude_mut(&mut self) -> &mut Vec<String> {
        &mut self.exclude
    }
}

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
