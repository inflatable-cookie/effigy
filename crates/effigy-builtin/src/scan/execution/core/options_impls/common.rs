use super::super::api::ScanCommonOptions;
use crate::BuiltinError;
use effigy_scan::{DuplicateBlockScanOptions, GodFileScanOptions, ScanRenderFormat};

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
