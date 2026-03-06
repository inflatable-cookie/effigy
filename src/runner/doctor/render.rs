#[path = "render/contracts.rs"]
mod contracts;
#[path = "render/grouping.rs"]
mod grouping;
#[path = "render/json_output.rs"]
mod json_output;
#[path = "render/scan_reports.rs"]
mod scan_reports;
#[path = "render/section_output.rs"]
mod section_output;
#[path = "render/shared_contracts.rs"]
pub(in crate::runner) mod shared_contracts;
#[path = "render/text.rs"]
mod text;

use super::report::DoctorReport;
use crate::runner::error::RunnerError;

pub(super) fn render_text(report: &DoctorReport, verbose: bool) -> Result<String, RunnerError> {
    text::render_text(report, verbose)
}

pub(super) fn render_json(report: &DoctorReport) -> Result<String, RunnerError> {
    json_output::render_json(report)
}
