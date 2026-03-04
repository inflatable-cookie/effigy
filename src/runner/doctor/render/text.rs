use crate::ui::{NoticeLevel, PlainRenderer, Renderer, SummaryCounts, TableSpec};

use super::super::render_support;
use super::super::text_blocks;
use super::super::{DoctorReport, RunnerError};
use super::contracts::DoctorFindingSection;

pub(super) fn render_text(report: &DoctorReport, verbose: bool) -> Result<String, RunnerError> {
    let mut renderer = render_support::doctor_plain_renderer();

    renderer
        .section(text_blocks::DOCTOR_REPORT_HEADING)
        .map_err(map_render_error)?;
    if report.findings.is_empty() {
        renderer
            .notice(NoticeLevel::Success, "No findings.")
            .map_err(map_render_error)?;
    } else {
        let sections = super::contracts::doctor_finding_sections(report);
        for section in &sections {
            render_finding_group(&mut renderer, section, verbose)?;
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
) -> Result<(), RunnerError> {
    renderer
        .notice(section.severity.to_notice_level(), &section.check_id)
        .map_err(map_render_error)?;

    let summary_sections = vec![
        text_blocks::bullet_section("evidence", section.evidence.clone()),
        text_blocks::bullet_section("remediation", section.remediation.clone()),
    ];
    text_blocks::render_bullet_sections(renderer, &summary_sections).map_err(map_render_error)?;
    let auto_fix_rows = text_blocks::key_values_from_pairs(vec![(
        "auto-fix".to_owned(),
        if section.auto_fix_available {
            "available".to_owned()
        } else {
            "no".to_owned()
        },
    )]);
    text_blocks::render_key_values(renderer, &auto_fix_rows).map_err(map_render_error)?;

    if verbose {
        let finding_count_rows = text_blocks::key_values_from_pairs(vec![(
            "findings".to_owned(),
            section.findings.len().to_string(),
        )]);
        text_blocks::render_key_values(renderer, &finding_count_rows).map_err(map_render_error)?;
        for (index, item) in section.findings.iter().enumerate() {
            let entry_rows = text_blocks::key_values_from_pairs(vec![
                ("entry".to_owned(), (index + 1).to_string()),
                ("severity".to_owned(), item.severity.as_str().to_owned()),
                ("entry-evidence".to_owned(), item.evidence.clone()),
                ("entry-remediation".to_owned(), item.remediation.clone()),
                (
                    "entry-auto-fix".to_owned(),
                    if item.fixable {
                        "available".to_owned()
                    } else {
                        "no".to_owned()
                    },
                ),
            ]);
            text_blocks::render_key_values(renderer, &entry_rows).map_err(map_render_error)?;
        }
    }

    render_root_resolution_details(renderer, section)?;
    renderer.text("").map_err(map_render_error)?;
    Ok(())
}

fn render_fix_actions(
    renderer: &mut PlainRenderer<Vec<u8>>,
    report: &DoctorReport,
) -> Result<(), RunnerError> {
    renderer.section("Fix Actions").map_err(map_render_error)?;
    let rows = super::contracts::doctor_fixes_table_rows(report);
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
    section: &DoctorFindingSection,
) -> Result<(), RunnerError> {
    let mut root_sections = Vec::<text_blocks::BulletListSection>::new();
    if let Some(section) = text_blocks::optional_bullet_section(
        "root-resolution-trace",
        &section.root_resolution_trace,
    ) {
        root_sections.push(section);
    }
    if let Some(section) = text_blocks::optional_bullet_section(
        "root-resolution-warnings",
        &section.root_resolution_warnings,
    ) {
        root_sections.push(section);
    }
    text_blocks::render_bullet_sections(renderer, &root_sections).map_err(map_render_error)?;
    Ok(())
}

fn map_render_error(error: crate::ui::UiError) -> RunnerError {
    render_support::map_doctor_render_error(render_support::DOCTOR_RENDER_TARGET, error)
}
