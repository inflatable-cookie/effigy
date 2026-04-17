use effigy_core::widgets::{NoticeLevel, SummaryCounts, TableSpec};
use effigy_ui::{PlainRenderer, Renderer};

use super::super::render_support;
use super::super::text_blocks;
use super::contracts::DoctorFindingSection;
use crate::DoctorError;
use crate::{DoctorReport, DoctorSeverity};

pub(super) fn render_text(report: &DoctorReport, verbose: bool) -> Result<String, DoctorError> {
    let mut renderer = render_support::doctor_plain_renderer();

    renderer
        .section(text_blocks::DOCTOR_REPORT_HEADING)
        .map_err(map_render_error)?;
    let sections = crate::doctor_finding_sections(report);
    let actionable_sections = sections
        .iter()
        .filter(|section| section.severity != DoctorSeverity::Info)
        .collect::<Vec<&DoctorFindingSection>>();
    if actionable_sections.is_empty() {
        renderer
            .notice(NoticeLevel::Success, "No findings.")
            .map_err(map_render_error)?;
    } else {
        let scan_report_paths =
            super::scan_reports::sync_scan_detail_reports(&report.resolved_root, &sections)?;
        for section in actionable_sections {
            render_finding_group(
                &mut renderer,
                section,
                verbose,
                scan_report_paths.get(&section.check_id).map(String::as_str),
            )?;
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
    section: &DoctorFindingSection,
    verbose: bool,
    scan_report_path: Option<&str>,
) -> Result<(), DoctorError> {
    renderer
        .notice(notice_level(section.severity), &section.check_id)
        .map_err(map_render_error)?;

    super::section_output::render_summary_rows(renderer, section, verbose, scan_report_path)?;
    super::section_output::render_root_resolution_details(renderer, section)?;
    renderer.text("").map_err(map_render_error)?;
    Ok(())
}

fn notice_level(severity: DoctorSeverity) -> NoticeLevel {
    match severity {
        DoctorSeverity::Info => NoticeLevel::Info,
        DoctorSeverity::Warning => NoticeLevel::Warning,
        DoctorSeverity::Error => NoticeLevel::Error,
    }
}

fn render_fix_actions(
    renderer: &mut PlainRenderer<Vec<u8>>,
    report: &DoctorReport,
) -> Result<(), DoctorError> {
    renderer.section("Fix Actions").map_err(map_render_error)?;
    let rows = crate::doctor_fixes_table_rows(report);
    renderer
        .table(&TableSpec::new(
            vec!["status".to_owned(), "fix".to_owned(), "detail".to_owned()],
            rows,
        ))
        .map_err(map_render_error)?;
    renderer.text("").map_err(map_render_error)?;
    Ok(())
}

fn map_render_error(error: effigy_ui::UiError) -> DoctorError {
    render_support::map_doctor_render_error(render_support::DOCTOR_RENDER_TARGET, error)
}
