use super::super::api::ScanCommonOptions;
use crate::runner::error::RunnerError;
use crate::runner::scan::model::{
    AttentionMarkerScanOptions, CommentRatioScanOptions, DuplicateBlockScanOptions,
    GeneratedAssetScanOptions, GeneratedInSrcScanOptions, GodFileScanOptions, ScanRenderFormat,
    StaleSuppressionScanOptions,
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
