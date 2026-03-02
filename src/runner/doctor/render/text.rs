use std::io::IsTerminal;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};

use super::super::{DoctorFinding, DoctorReport, RunnerError};

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
        let grouped = super::grouping::grouped_findings(&report.findings);
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

fn render_finding_group(
    renderer: &mut PlainRenderer<Vec<u8>>,
    check_id: &str,
    items: &[&DoctorFinding],
    report: &DoctorReport,
    verbose: bool,
) -> Result<(), RunnerError> {
    renderer
        .notice(
            super::grouping::group_max_severity(items).to_notice_level(),
            check_id,
        )
        .map_err(map_render_error)?;

    let (evidence_items, remediation_items, any_fixable) = super::grouping::summarize_group(items);
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

fn map_render_error(error: crate::ui::UiError) -> RunnerError {
    RunnerError::Ui(format!("failed to render doctor output: {error}"))
}
