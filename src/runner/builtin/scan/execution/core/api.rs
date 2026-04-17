use serde_json::{Map, Value};

use crate::runner::error::RunnerError;
use effigy_scan::ScanRenderFormat;

#[derive(Clone, Copy)]
pub(in crate::runner::builtin::scan::execution) struct ScanModeConfig {
    pub(in crate::runner::builtin::scan::execution) label: &'static str,
    pub(in crate::runner::builtin::scan::execution) schema_name: &'static str,
}

impl ScanModeConfig {
    pub(in crate::runner::builtin::scan::execution) const fn new(
        label: &'static str,
        schema_name: &'static str,
    ) -> Self {
        Self { label, schema_name }
    }
}

pub(in crate::runner::builtin::scan::execution) trait ScanPayloadResult {
    fn root(&self) -> &str;
    fn finding_count(&self) -> usize;
    fn insert_payload_fields(&self, payload: &mut Map<String, Value>);
}

pub(in crate::runner::builtin::scan::execution) trait ScanCommonOptions {
    fn format(&self) -> ScanRenderFormat;
    fn output_path(&self) -> Option<&String>;
    fn fail_on_findings(&self) -> bool;
    fn respect_gitignore(&self) -> bool;
    fn validate(&self) -> Result<(), RunnerError>;
    fn format_mut(&mut self) -> &mut ScanRenderFormat;
    fn fail_on_findings_mut(&mut self) -> &mut bool;
    fn respect_gitignore_mut(&mut self) -> &mut bool;
    fn include_mut(&mut self) -> &mut Vec<String>;
    fn exclude_mut(&mut self) -> &mut Vec<String>;
}

pub(in crate::runner::builtin::scan::execution) trait ScanThresholdOverrideOptions:
    ScanCommonOptions
{
    type Thresholds: ScanThresholds;

    fn thresholds_mut(&mut self) -> &mut Self::Thresholds;
}

pub(in crate::runner::builtin::scan::execution) trait ScanThresholds {
    fn warn_mut(&mut self) -> &mut usize;
    fn high_mut(&mut self) -> &mut usize;
    fn critical_mut(&mut self) -> &mut usize;
}
