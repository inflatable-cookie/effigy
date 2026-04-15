use super::super::super::render::encode_json;
use super::super::report::DoctorReport;
use crate::runner::error::RunnerError;

pub(super) fn render_json(report: &DoctorReport) -> Result<String, RunnerError> {
    let sections = effigy_doctor::doctor_finding_sections(report);
    let payload = super::contracts::doctor_json_payload(report, &sections);
    encode_json(&payload, true)
}
