use super::super::{DoctorFinding, DoctorSeverity};

pub(super) fn grouped_findings(findings: &[DoctorFinding]) -> Vec<(String, Vec<&DoctorFinding>)> {
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

pub(super) fn group_max_severity(items: &[&DoctorFinding]) -> DoctorSeverity {
    items
        .iter()
        .map(|item| item.severity)
        .max()
        .unwrap_or(DoctorSeverity::Info)
}

pub(super) fn summarize_group(items: &[&DoctorFinding]) -> (Vec<String>, Vec<String>, bool) {
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

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|item| item == value) {
        values.push(value.to_owned());
    }
}
