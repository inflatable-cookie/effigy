use std::io::IsTerminal;

use serde_json::json;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};

use super::{DoctorFinding, DoctorReport, DoctorSeverity, RunnerError};

pub(super) fn render_text(report: &DoctorReport, verbose: bool) -> String {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);

    let _ = renderer.section("Doctor's Report");
    if report.findings.is_empty() {
        let _ = renderer.notice(NoticeLevel::Success, "No findings.");
    } else {
        let mut grouped = Vec::<(String, Vec<&DoctorFinding>)>::new();
        for finding in &report.findings {
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
            let left_severity = left_items
                .iter()
                .map(|item| item.severity)
                .max()
                .unwrap_or(DoctorSeverity::Info);
            let right_severity = right_items
                .iter()
                .map(|item| item.severity)
                .max()
                .unwrap_or(DoctorSeverity::Info);
            right_severity
                .rank()
                .cmp(&left_severity.rank())
                .then_with(|| left_id.cmp(right_id))
        });

        for (check_id, items) in grouped {
            let max_severity = items
                .iter()
                .map(|item| item.severity)
                .max()
                .unwrap_or(DoctorSeverity::Info);
            let _ = renderer.notice(max_severity.to_notice_level(), &check_id);

            let mut evidence_items = Vec::<String>::new();
            let mut remediation_items = Vec::<String>::new();
            let mut any_fixable = false;
            for item in &items {
                if !evidence_items.contains(&item.evidence) {
                    evidence_items.push(item.evidence.clone());
                }
                if !remediation_items.contains(&item.remediation) {
                    remediation_items.push(item.remediation.clone());
                }
                any_fixable = any_fixable || item.fixable;
            }

            let _ = renderer.bullet_list("evidence", &evidence_items);
            let _ = renderer.bullet_list("remediation", &remediation_items);
            let _ = renderer.key_values(&[KeyValue::new(
                "auto-fix",
                if any_fixable { "available" } else { "no" },
            )]);
            if verbose {
                let _ = renderer.key_values(&[KeyValue::new("findings", items.len().to_string())]);
                for (index, item) in items.iter().enumerate() {
                    let _ = renderer.key_values(&[
                        KeyValue::new("entry", (index + 1).to_string()),
                        KeyValue::new("severity", item.severity.as_str()),
                        KeyValue::new("entry-evidence", item.evidence.clone()),
                        KeyValue::new("entry-remediation", item.remediation.clone()),
                        KeyValue::new(
                            "entry-auto-fix",
                            if item.fixable { "available" } else { "no" },
                        ),
                    ]);
                }
            }

            if check_id == "workspace.root-resolution" {
                if !report.root_evidence.is_empty() {
                    let _ = renderer.bullet_list("root-resolution-trace", &report.root_evidence);
                }
                if !report.root_warnings.is_empty() {
                    let _ = renderer.bullet_list("root-resolution-warnings", &report.root_warnings);
                }
            }
            let _ = renderer.text("");
        }
    }

    if !report.fixes.is_empty() {
        let _ = renderer.section("Fix Actions");
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
        let _ = renderer.table(&TableSpec::new(
            vec!["status".to_owned(), "fix".to_owned(), "detail".to_owned()],
            rows,
        ));
        let _ = renderer.text("");
    }

    let _ = renderer.summary(SummaryCounts {
        ok: report.summary.pass,
        warn: report.summary.warning,
        err: report.summary.error,
    });

    let out = renderer.into_inner();
    String::from_utf8_lossy(&out).to_string()
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
