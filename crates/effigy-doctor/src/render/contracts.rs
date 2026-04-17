use serde_json::json;

pub(super) use crate::DoctorFindingSection;
use crate::{DoctorReport, DoctorSeverity};

pub(super) fn doctor_json_payload(
    report: &DoctorReport,
    sections: &[DoctorFindingSection],
) -> serde_json::Value {
    let renderable_sections = renderable_sections(sections);
    let findings = flatten_section_findings(&renderable_sections);
    let fixes = report
        .fixes
        .iter()
        .map(|fix| {
            json!({
                "fix_id": fix.fix_id,
                "status": fix.status.as_str(),
                "detail": fix.detail,
            })
        })
        .collect::<Vec<serde_json::Value>>();

    json!({
        "schema": "effigy.doctor.v1",
        "schema_version": 1,
        "ok": report.summary.error == 0,
        "summary": {
            "checks": report.summary.checks,
            "pass": report.summary.pass,
            "warning": report.summary.warning,
            "error": report.summary.error,
        },
        "sections": section_payloads(&renderable_sections),
        "findings": findings,
        "fixes": fixes,
        "root_resolution": super::shared_contracts::root_resolution_payload(
            None,
            &report.root_evidence,
            &report.root_warnings,
        )
    })
}

fn renderable_sections(sections: &[DoctorFindingSection]) -> Vec<&DoctorFindingSection> {
    sections
        .iter()
        .filter(|section| section.severity != DoctorSeverity::Info)
        .collect::<Vec<&DoctorFindingSection>>()
}

fn section_payloads(sections: &[&DoctorFindingSection]) -> Vec<serde_json::Value> {
    sections
        .iter()
        .map(|section| {
            json!({
                "check_id": &section.check_id,
                "severity": section.severity.as_str(),
                "evidence": &section.evidence,
                "remediation": &section.remediation,
                "auto_fix_available": section.auto_fix_available,
                "findings": section.findings.iter().map(|item| {
                    json!({
                        "severity": item.severity.as_str(),
                        "evidence": &item.evidence,
                        "remediation": &item.remediation,
                        "fixable": item.fixable,
                    })
                }).collect::<Vec<serde_json::Value>>(),
                "root_resolution_trace": &section.root_resolution_trace,
                "root_resolution_warnings": &section.root_resolution_warnings,
            })
        })
        .collect::<Vec<serde_json::Value>>()
}

fn flatten_section_findings(sections: &[&DoctorFindingSection]) -> Vec<serde_json::Value> {
    let mut values = Vec::<serde_json::Value>::new();
    for section in sections {
        for item in &section.findings {
            values.push(json!({
                "check_id": &section.check_id,
                "severity": item.severity.as_str(),
                "evidence": &item.evidence,
                "remediation": &item.remediation,
                "fixable": item.fixable,
            }));
        }
    }
    values
}
