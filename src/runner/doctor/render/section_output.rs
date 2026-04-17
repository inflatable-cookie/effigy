use effigy_ui::PlainRenderer;

use super::super::text_blocks;
use super::contracts::DoctorFindingSection;
use super::scan_reports::scan_finding_counts;
use crate::runner::error::RunnerError;

pub(super) fn render_summary_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    section: &DoctorFindingSection,
    verbose: bool,
    scan_report_path: Option<&str>,
) -> Result<(), RunnerError> {
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

    if let Some(report_path) = scan_report_path {
        let (warning_count, error_count) = scan_finding_counts(section);
        let scan_rows = text_blocks::key_values_from_pairs(vec![
            ("findings".to_owned(), section.findings.len().to_string()),
            ("warning-findings".to_owned(), warning_count.to_string()),
            ("error-findings".to_owned(), error_count.to_string()),
            ("detail-report".to_owned(), report_path.to_owned()),
        ]);
        return text_blocks::render_key_values(renderer, &scan_rows).map_err(map_render_error);
    }

    if verbose {
        let finding_count_rows = text_blocks::key_values_from_pairs(vec![(
            "findings".to_owned(),
            section.findings.len().to_string(),
        )]);
        text_blocks::render_key_values(renderer, &finding_count_rows).map_err(map_render_error)?;
        render_verbose_findings(renderer, section)?;
    }

    Ok(())
}

pub(super) fn render_root_resolution_details(
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

fn render_verbose_findings(
    renderer: &mut PlainRenderer<Vec<u8>>,
    section: &DoctorFindingSection,
) -> Result<(), RunnerError> {
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
    Ok(())
}

fn map_render_error(error: effigy_ui::UiError) -> RunnerError {
    super::super::render_support::map_doctor_render_error(
        super::super::render_support::DOCTOR_RENDER_TARGET,
        error,
    )
}
