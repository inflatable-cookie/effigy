use serde_json::json;

use super::super::contracts::check_id as doctor_check_id;
use super::super::report::{DoctorFinding, DoctorReport, DoctorSeverity};

pub(super) struct DoctorFindingSection {
    pub(super) check_id: String,
    pub(super) severity: DoctorSeverity,
    pub(super) evidence: Vec<String>,
    pub(super) remediation: Vec<String>,
    pub(super) auto_fix_available: bool,
    pub(super) findings: Vec<DoctorSectionFinding>,
    pub(super) root_resolution_trace: Vec<String>,
    pub(super) root_resolution_warnings: Vec<String>,
}

pub(super) struct DoctorSectionFinding {
    pub(super) severity: DoctorSeverity,
    pub(super) evidence: String,
    pub(super) remediation: String,
    pub(super) fixable: bool,
}

pub(super) fn doctor_finding_sections(report: &DoctorReport) -> Vec<DoctorFindingSection> {
    super::grouping::grouped_findings(&report.findings)
        .into_iter()
        .map(|(check_id, items)| {
            let (evidence, remediation, auto_fix_available) = if is_scan_check_id(&check_id) {
                scan_group_summary(&items)
            } else {
                super::grouping::summarize_group(&items)
            };
            let severity = super::grouping::group_max_severity(&items);
            let findings = items
                .iter()
                .map(|item| DoctorSectionFinding {
                    severity: item.severity,
                    evidence: item.evidence.clone(),
                    remediation: item.remediation.clone(),
                    fixable: item.fixable,
                })
                .collect::<Vec<DoctorSectionFinding>>();
            let (root_resolution_trace, root_resolution_warnings) =
                root_resolution_section_details(&check_id, report);
            DoctorFindingSection {
                check_id,
                severity,
                evidence,
                remediation,
                auto_fix_available,
                findings,
                root_resolution_trace,
                root_resolution_warnings,
            }
        })
        .collect::<Vec<DoctorFindingSection>>()
}

fn is_scan_check_id(check_id: &str) -> bool {
    matches!(
        check_id,
        doctor_check_id::SCAN_GOD_FILES
            | doctor_check_id::SCAN_DUPLICATE_BLOCKS
            | doctor_check_id::SCAN_COMMENT_RATIO
            | doctor_check_id::SCAN_GENERATED_ASSETS
            | doctor_check_id::SCAN_ATTENTION_MARKERS
    )
}

fn scan_group_summary(items: &[&DoctorFinding]) -> (Vec<String>, Vec<String>, bool) {
    let (warning_count, error_count) =
        items
            .iter()
            .fold((0usize, 0usize), |(warn, err), item| match item.severity {
                DoctorSeverity::Warning => (warn + 1, err),
                DoctorSeverity::Error => (warn, err + 1),
                DoctorSeverity::Info => (warn, err),
            });
    let mut remediation = Vec::<String>::new();
    for item in items {
        if !remediation.iter().any(|value| value == &item.remediation) {
            remediation.push(item.remediation.clone());
        }
    }
    (
        vec![format!(
            "{} scan findings detected (warning={}, error={}). See detail report for file-level entries.",
            items.len(),
            warning_count,
            error_count
        )],
        remediation,
        false,
    )
}

pub(super) fn doctor_fixes_table_rows(report: &DoctorReport) -> Vec<Vec<String>> {
    report
        .fixes
        .iter()
        .map(|fix| {
            vec![
                fix.status.as_str().to_owned(),
                fix.fix_id.clone(),
                fix.detail.clone(),
            ]
        })
        .collect::<Vec<Vec<String>>>()
}

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

fn root_resolution_section_details(
    check_id: &str,
    report: &DoctorReport,
) -> (Vec<String>, Vec<String>) {
    if check_id != doctor_check_id::WORKSPACE_ROOT_RESOLUTION {
        return (Vec::new(), Vec::new());
    }
    (report.root_evidence.clone(), report.root_warnings.clone())
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
