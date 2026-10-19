#[path = "render/contracts.rs"]
mod contracts;
#[path = "render/json_output.rs"]
mod json_output;
#[path = "render/scan_reports.rs"]
mod scan_reports;
#[path = "render/section_output.rs"]
mod section_output;
#[path = "render/shared_contracts.rs"]
pub(crate) mod shared_contracts;
#[path = "render/text.rs"]
mod text;

use crate::DoctorError;
use crate::DoctorReport;

/// Info findings stay out of the report except those an operator acts on.
pub(crate) fn renders_section(section: &crate::DoctorFindingSection) -> bool {
    section.severity != crate::DoctorSeverity::Info
        || section.check_id == crate::check_id::DEPENDENCY_LINK_HEALTH
        || section
            .check_id
            .starts_with(crate::check_id::TASKS_LEGACY_STATE_PREFIX)
}

pub(super) fn render_text(report: &DoctorReport, verbose: bool) -> Result<String, DoctorError> {
    text::render_text(report, verbose)
}

pub(super) fn render_json(report: &DoctorReport) -> Result<String, DoctorError> {
    json_output::render_json(report)
}
