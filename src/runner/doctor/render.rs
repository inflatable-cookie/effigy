use std::io::IsTerminal;

use serde_json::json;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};

use super::{DoctorFinding, DoctorReport, DoctorSeverity, RunnerError};

pub(super) fn render_text(report: &DoctorReport, verbose: bool) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);

    renderer
        .section("Doctor's Report")
        .map_err(map_render_error)?;
    if report.findings.is_empty() {
        renderer
            .notice(NoticeLevel::Success, "No findings.")
            .map_err(map_render_error)?;
    } else {
        let grouped = grouped_findings(&report.findings);
        for (check_id, items) in grouped {
            render_finding_group(&mut renderer, &check_id, &items, report, verbose)?;
        }
    }

    if !report.fixes.is_empty() {
        render_fix_actions(&mut renderer, report)?;
    }

    renderer
        .summary(SummaryCounts {
            ok: report.summary.pass,
            warn: report.summary.warning,
            err: report.summary.error,
        })
        .map_err(map_render_error)?;

    let out = renderer.into_inner();
    Ok(String::from_utf8_lossy(&out).to_string())
}

fn grouped_findings(findings: &[DoctorFinding]) -> Vec<(String, Vec<&DoctorFinding>)> {
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

fn group_max_severity(items: &[&DoctorFinding]) -> DoctorSeverity {
    items
        .iter()
        .map(|item| item.severity)
        .max()
        .unwrap_or(DoctorSeverity::Info)
}

fn render_finding_group(
    renderer: &mut PlainRenderer<Vec<u8>>,
    check_id: &str,
    items: &[&DoctorFinding],
    report: &DoctorReport,
    verbose: bool,
) -> Result<(), RunnerError> {
    renderer
        .notice(group_max_severity(items).to_notice_level(), check_id)
        .map_err(map_render_error)?;

    let (evidence_items, remediation_items, any_fixable) = summarize_group(items);
    renderer
        .bullet_list("evidence", &evidence_items)
        .map_err(map_render_error)?;
    renderer
        .bullet_list("remediation", &remediation_items)
        .map_err(map_render_error)?;
    renderer
        .key_values(&[KeyValue::new(
            "auto-fix",
            if any_fixable { "available" } else { "no" },
        )])
        .map_err(map_render_error)?;

    if verbose {
        renderer
            .key_values(&[KeyValue::new("findings", items.len().to_string())])
            .map_err(map_render_error)?;
        for (index, item) in items.iter().enumerate() {
            renderer
                .key_values(&[
                    KeyValue::new("entry", (index + 1).to_string()),
                    KeyValue::new("severity", item.severity.as_str()),
                    KeyValue::new("entry-evidence", item.evidence.clone()),
                    KeyValue::new("entry-remediation", item.remediation.clone()),
                    KeyValue::new(
                        "entry-auto-fix",
                        if item.fixable { "available" } else { "no" },
                    ),
                ])
                .map_err(map_render_error)?;
        }
    }

    render_root_resolution_details(renderer, check_id, report)?;
    renderer.text("").map_err(map_render_error)?;
    Ok(())
}

fn summarize_group(items: &[&DoctorFinding]) -> (Vec<String>, Vec<String>, bool) {
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

fn render_fix_actions(
    renderer: &mut PlainRenderer<Vec<u8>>,
    report: &DoctorReport,
) -> Result<(), RunnerError> {
    renderer.section("Fix Actions").map_err(map_render_error)?;
    let rows = report
        .fixes
        .iter()
        .map(|fix| {
            vec![
                fix.status.as_str().to_owned(),
                fix.fix_id.clone(),
                fix.detail.clone(),
            ]
        })
        .collect::<Vec<Vec<String>>>();
    renderer
        .table(&TableSpec::new(
            vec!["status".to_owned(), "fix".to_owned(), "detail".to_owned()],
            rows,
        ))
        .map_err(map_render_error)?;
    renderer.text("").map_err(map_render_error)?;
    Ok(())
}

fn render_root_resolution_details(
    renderer: &mut PlainRenderer<Vec<u8>>,
    check_id: &str,
    report: &DoctorReport,
) -> Result<(), RunnerError> {
    if check_id != "workspace.root-resolution" {
        return Ok(());
    }
    if !report.root_evidence.is_empty() {
        renderer
            .bullet_list("root-resolution-trace", &report.root_evidence)
            .map_err(map_render_error)?;
    }
    if !report.root_warnings.is_empty() {
        renderer
            .bullet_list("root-resolution-warnings", &report.root_warnings)
            .map_err(map_render_error)?;
    }
    Ok(())
}

pub(super) fn render_json(report: &DoctorReport) -> Result<String, RunnerError> {
    let findings = report
        .findings
        .iter()
        .map(|finding| {
            json!({
                "check_id": finding.check_id,
                "severity": finding.severity.as_str(),
                "evidence": finding.evidence,
                "remediation": finding.remediation,
                "fixable": finding.fixable,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    let payload = json!({
        "schema": "effigy.doctor.v1",
        "schema_version": 1,
        "ok": report.summary.error == 0,
        "summary": {
            "checks": report.summary.checks,
            "pass": report.summary.pass,
            "warning": report.summary.warning,
            "error": report.summary.error,
        },
        "findings": findings,
        "fixes": report.fixes.iter().map(|fix| {
            json!({
                "fix_id": fix.fix_id,
                "status": fix.status.as_str(),
                "detail": fix.detail,
            })
        }).collect::<Vec<serde_json::Value>>(),
        "root_resolution": {
            "evidence": report.root_evidence,
            "warnings": report.root_warnings,
        }
    });
    serde_json::to_string_pretty(&payload)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

fn map_render_error(error: crate::ui::UiError) -> RunnerError {
    RunnerError::Ui(format!("failed to render doctor output: {error}"))
}
