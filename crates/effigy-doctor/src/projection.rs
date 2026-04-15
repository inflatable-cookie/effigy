use crate::{check_id as doctor_check_id, DoctorFinding, DoctorReport, DoctorSeverity};

#[derive(Debug, Clone)]
pub struct DoctorFindingSection {
    pub check_id: String,
    pub severity: DoctorSeverity,
    pub evidence: Vec<String>,
    pub remediation: Vec<String>,
    pub auto_fix_available: bool,
    pub findings: Vec<DoctorSectionFinding>,
    pub root_resolution_trace: Vec<String>,
    pub root_resolution_warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DoctorSectionFinding {
    pub severity: DoctorSeverity,
    pub evidence: String,
    pub remediation: String,
    pub fixable: bool,
}

pub fn grouped_findings(findings: &[DoctorFinding]) -> Vec<(String, Vec<&DoctorFinding>)> {
    let mut grouped = Vec::<(String, Vec<&DoctorFinding>)>::new();
    for finding in findings {
        if let Some((_, items)) = grouped
            .iter_mut()
            .find(|(check_id, _)| check_id == &finding.check_id)
        {
            items.push(finding);
        } else {
            grouped.push((finding.check_id.clone(), vec![finding]));
        }
    }
    grouped.sort_by(|(left_id, left_items), (right_id, right_items)| {
        let left_severity = group_max_severity(left_items);
        let right_severity = group_max_severity(right_items);
        right_severity
            .rank()
            .cmp(&left_severity.rank())
            .then_with(|| left_id.cmp(right_id))
    });
    grouped
}

pub fn group_max_severity(items: &[&DoctorFinding]) -> DoctorSeverity {
    items
        .iter()
        .map(|item| item.severity)
        .max()
        .unwrap_or(DoctorSeverity::Info)
}

pub fn summarize_group(items: &[&DoctorFinding]) -> (Vec<String>, Vec<String>, bool) {
    let mut evidence_items = Vec::<String>::new();
    let mut remediation_items = Vec::<String>::new();
    let mut any_fixable = false;
    for item in items {
        push_unique(&mut evidence_items, &item.evidence);
        push_unique(&mut remediation_items, &item.remediation);
        any_fixable = any_fixable || item.fixable;
    }
    (evidence_items, remediation_items, any_fixable)
}

pub fn doctor_finding_sections(report: &DoctorReport) -> Vec<DoctorFindingSection> {
    grouped_findings(&report.findings)
        .into_iter()
        .map(|(check_id, items)| {
            let (evidence, remediation, auto_fix_available) = if is_scan_check_id(&check_id) {
                scan_group_summary(&items)
            } else {
                summarize_group(&items)
            };
            let severity = group_max_severity(&items);
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

pub fn doctor_fixes_table_rows(report: &DoctorReport) -> Vec<Vec<String>> {
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

fn is_scan_check_id(check_id: &str) -> bool {
    matches!(
        check_id,
        doctor_check_id::SCAN_GOD_FILES
            | doctor_check_id::SCAN_DUPLICATE_BLOCKS
            | doctor_check_id::SCAN_COMMENT_RATIO
            | doctor_check_id::SCAN_GENERATED_ASSETS
            | doctor_check_id::SCAN_GENERATED_IN_SRC
            | doctor_check_id::SCAN_ATTENTION_MARKERS
            | doctor_check_id::SCAN_STALE_SUPPRESSIONS
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

fn root_resolution_section_details(
    check_id: &str,
    report: &DoctorReport,
) -> (Vec<String>, Vec<String>) {
    if check_id != doctor_check_id::WORKSPACE_ROOT_RESOLUTION {
        return (Vec::new(), Vec::new());
    }
    (report.root_evidence.clone(), report.root_warnings.clone())
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|item| item == value) {
        values.push(value.to_owned());
    }
}
