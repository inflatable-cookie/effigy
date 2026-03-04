use super::super::super::render::encode_json;
use super::super::{DoctorReport, RunnerError};

pub(super) fn render_json(report: &DoctorReport) -> Result<String, RunnerError> {
    let sections = super::contracts::doctor_finding_sections(report);
    let payload = super::contracts::doctor_json_payload(report, &sections);
    encode_json(&payload, true)
}
