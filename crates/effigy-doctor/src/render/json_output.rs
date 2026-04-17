use effigy_ui::encode_json;

use crate::DoctorError;
use crate::DoctorReport;

pub(super) fn render_json(report: &DoctorReport) -> Result<String, DoctorError> {
    let sections = crate::doctor_finding_sections(report);
    let payload = super::contracts::doctor_json_payload(report, &sections);
    Ok(encode_json(&payload, true)?)
}
