#[path = "render/grouping.rs"]
mod grouping;
#[path = "render/json_output.rs"]
mod json_output;
#[path = "render/text.rs"]
mod text;

use super::{DoctorReport, RunnerError};

pub(super) fn render_text(report: &DoctorReport, verbose: bool) -> Result<String, RunnerError> {
    text::render_text(report, verbose)
}

pub(super) fn render_json(report: &DoctorReport) -> Result<String, RunnerError> {
    json_output::render_json(report)
}
