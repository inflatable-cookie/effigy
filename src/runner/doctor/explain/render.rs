use super::super::render_support;
use super::super::text_blocks;
use super::contracts::{self, DeferralOutcome, SelectionOutcome};
use crate::runner::error::RunnerError;
use effigy_cli::TaskInvocation;
use effigy_core::widgets::NoticeLevel;
use effigy_manifest::LoadedCatalog;
use effigy_ui::{encode_json, Renderer};

pub(super) fn render_explain_json(
    request: &TaskInvocation,
    resolved: &effigy_core::resolver::ResolvedTarget,
    task_name: &str,
    selection: &SelectionOutcome,
    deferral: &DeferralOutcome,
    candidates: &[String],
) -> Result<String, RunnerError> {
    let contract = contracts::build_explain_render_contract(
        request, resolved, task_name, selection, deferral, candidates,
    );
    let payload = contracts::explain_json_payload(&contract);
    Ok(encode_json(&payload, true)?)
}

pub(super) fn render_explain_text(
    request: &TaskInvocation,
    resolved: &effigy_core::resolver::ResolvedTarget,
    selection: &SelectionOutcome,
    deferral: &DeferralOutcome,
    candidates: &[String],
    catalogs: &[LoadedCatalog],
    verbose: bool,
) -> Result<String, RunnerError> {
    let contract = contracts::build_explain_render_contract(
        request,
        resolved,
        &request.name,
        selection,
        deferral,
        candidates,
    );
    let mut renderer = render_support::doctor_plain_renderer();
    renderer
        .section(text_blocks::DOCTOR_EXPLAIN_HEADING)
        .map_err(map_render_error)?;
    let summary_rows = contracts::explain_text_summary_rows(&contract)
        .into_iter()
        .collect::<Vec<(String, String)>>();
    let summary_key_values = text_blocks::key_values_from_pairs(summary_rows);
    text_blocks::render_key_values(&mut renderer, &summary_key_values).map_err(map_render_error)?;
    if let Some(source) = contract.deferral.source.as_ref() {
        let rows = text_blocks::key_values_from_pairs(vec![(
            "deferral-source".to_owned(),
            source.clone(),
        )]);
        text_blocks::render_key_values(&mut renderer, &rows).map_err(map_render_error)?;
    }
    if let Some(working_dir) = contract.deferral.working_dir.as_ref() {
        let rows = text_blocks::key_values_from_pairs(vec![(
            "deferral-working-dir".to_owned(),
            working_dir.clone(),
        )]);
        text_blocks::render_key_values(&mut renderer, &rows).map_err(map_render_error)?;
    }
    if let Some(error) = contract.selection.error.as_ref() {
        renderer
            .notice(NoticeLevel::Warning, error)
            .map_err(map_render_error)?;
    }
    renderer.text("").map_err(map_render_error)?;
    let mut sections = vec![text_blocks::bullet_section(
        "candidate-catalogs",
        contract.candidates.clone(),
    )];
    if let Some(section) =
        text_blocks::optional_bullet_section("selection-evidence", &contract.selection.evidence)
    {
        sections.push(section);
    }
    if let Some(section) = text_blocks::optional_bullet_section(
        "ambiguity-candidates",
        &contract.selection.ambiguity_candidates,
    ) {
        sections.push(section);
    }
    text_blocks::render_bullet_sections(&mut renderer, &sections).map_err(map_render_error)?;
    if verbose {
        let mut all_catalogs = catalogs
            .iter()
            .map(|catalog| {
                format!(
                    "{} ({}) depth={} has_defer={}",
                    catalog.alias,
                    catalog.manifest_path.display(),
                    catalog.depth,
                    catalog.defer_run.is_some()
                )
            })
            .collect::<Vec<String>>();
        all_catalogs.sort();
        let mut verbose_sections = vec![text_blocks::bullet_section(
            "discovered-catalogs",
            all_catalogs,
        )];
        if let Some(section) = text_blocks::optional_bullet_section(
            "root-resolution-evidence",
            &contract.root_evidence,
        ) {
            verbose_sections.push(section);
        }
        if let Some(section) = text_blocks::optional_bullet_section(
            "root-resolution-warnings",
            &contract.root_warnings,
        ) {
            verbose_sections.push(section);
        }
        text_blocks::render_bullet_sections(&mut renderer, &verbose_sections)
            .map_err(map_render_error)?;
    }
    let out = renderer.into_inner();
    Ok(String::from_utf8_lossy(&out).to_string())
}

fn map_render_error(error: effigy_ui::UiError) -> RunnerError {
    render_support::map_doctor_render_error(render_support::DOCTOR_EXPLAIN_RENDER_TARGET, error)
}
