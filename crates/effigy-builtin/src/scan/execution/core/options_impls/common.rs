use super::super::api::ScanCommonOptions;
use crate::BuiltinError;
use effigy_scan::{
    AttentionMarkerScanOptions, BoundaryViolationScanOptions, CommentRatioScanOptions,
    DeadCodeScanOptions, DuplicateBlockScanOptions, GeneratedAssetScanOptions,
    GeneratedInSrcScanOptions, GodFileScanOptions, ScanRenderFormat, StaleSuppressionScanOptions,
    ValidationGapScanOptions,
};

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
    fn validate(&self) -> Result<(), BuiltinError> {
        GodFileScanOptions::validate(self).map_err(Into::into)
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
    fn validate(&self) -> Result<(), BuiltinError> {
        GeneratedAssetScanOptions::validate(self).map_err(Into::into)
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

impl ScanCommonOptions for BoundaryViolationScanOptions {
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
    fn validate(&self) -> Result<(), BuiltinError> {
        BoundaryViolationScanOptions::validate(self).map_err(Into::into)
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

impl ScanCommonOptions for DeadCodeScanOptions {
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
    fn validate(&self) -> Result<(), BuiltinError> {
        DeadCodeScanOptions::validate(self).map_err(Into::into)
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

impl ScanCommonOptions for ValidationGapScanOptions {
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
    fn validate(&self) -> Result<(), BuiltinError> {
        ValidationGapScanOptions::validate(self).map_err(Into::into)
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
    fn validate(&self) -> Result<(), BuiltinError> {
        GeneratedInSrcScanOptions::validate(self).map_err(Into::into)
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
    fn validate(&self) -> Result<(), BuiltinError> {
        DuplicateBlockScanOptions::validate(self).map_err(Into::into)
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
    fn validate(&self) -> Result<(), BuiltinError> {
        CommentRatioScanOptions::validate(self).map_err(Into::into)
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
    fn validate(&self) -> Result<(), BuiltinError> {
        AttentionMarkerScanOptions::validate(self).map_err(Into::into)
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
    fn validate(&self) -> Result<(), BuiltinError> {
        StaleSuppressionScanOptions::validate(self).map_err(Into::into)
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
