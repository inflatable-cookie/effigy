use std::io::IsTerminal;
use std::path::PathBuf;

use serde_json::json;

use crate::resolver::resolve_target_root;
use crate::ui::theme::resolve_color_enabled;
use crate::ui::{KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer};
use crate::TaskInvocation;

use super::super::catalog::{discover_catalogs, select_catalog_and_task};
use super::super::deferral::{select_deferral, should_attempt_deferral};
use super::super::util::parse_task_selector;
use super::{CatalogSelectionMode, RunnerError};

const DEFERRAL_NOT_CONSIDERED_REASON: &str =
    "deferral was not considered because the selection outcome does not trigger deferral";
const DEFERRAL_SELECTED_REASON: &str =
    "deferral was selected from configured or implicit fallback routing";
const DEFERRAL_NOT_FOUND_REASON: &str =
    "deferral was considered but no eligible fallback route was found";

struct SelectionOutcome {
    status: String,
    catalog: Option<String>,
    mode: Option<String>,
    evidence: Vec<String>,
    error: Option<String>,
    ambiguity_candidates: Vec<String>,
    reasoning: String,
}

struct DeferralOutcome {
    considered: bool,
    selected: bool,
    source: Option<String>,
    working_dir: Option<String>,
    reasoning: String,
}

pub(super) fn run_doctor_explain(
    request: TaskInvocation,
    repo_override: Option<PathBuf>,
    output_json: bool,
    fix: bool,
    verbose: bool,
) -> Result<String, RunnerError> {
    if fix {
        return Err(RunnerError::TaskInvocation(
            "`--fix` is not supported with explain mode (`effigy doctor <task> <args>`)."
                .to_owned(),
        ));
    }

    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    let resolved = resolve_target_root(cwd.clone(), repo_override)?;
    let catalogs = discover_catalogs(&resolved.resolved_root)?;
    let selector = parse_task_selector(&request.name)?;

    let candidates = candidate_catalogs(&catalogs, &selector, &cwd);
    let selection_result = select_catalog_and_task(&selector, &catalogs, &cwd);
    let selection = compute_selection_outcome(&selection_result);
    let deferral = compute_deferral_outcome(
        &selection_result,
        &selector,
        &catalogs,
        &cwd,
        &resolved.resolved_root,
    );

    if output_json {
        return render_explain_json(
            &request,
            &resolved,
            &selector.task_name,
            &selection,
            &deferral,
            &candidates,
        );
    }

    render_explain_text(
        &request,
        &resolved,
        &selection,
        &deferral,
        &candidates,
        &catalogs,
        verbose,
    )
}

fn render_explain_json(
    request: &TaskInvocation,
    resolved: &crate::resolver::ResolvedTarget,
    task_name: &str,
    selection: &SelectionOutcome,
    deferral: &DeferralOutcome,
    candidates: &[String],
) -> Result<String, RunnerError> {
    let payload = json!({
        "schema": "effigy.doctor.explain.v1",
        "schema_version": 1,
        "request": {
            "task": request.name,
            "args": request.args,
        },
        "root_resolution": {
            "resolved_root": resolved.resolved_root.display().to_string(),
            "evidence": resolved.evidence,
            "warnings": resolved.warnings,
        },
        "selection": {
            "status": selection.status,
            "catalog": selection.catalog,
            "task": task_name,
            "mode": selection.mode,
            "evidence": selection.evidence,
            "error": selection.error,
        },
        "candidates": candidates,
        "ambiguity_candidates": selection.ambiguity_candidates,
        "deferral": {
            "considered": deferral.considered,
            "selected": deferral.selected,
            "source": deferral.source,
            "working_dir": deferral.working_dir,
        },
        "reasoning": {
            "selection": selection.reasoning,
            "deferral": deferral.reasoning,
        },
    });
    serde_json::to_string_pretty(&payload)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

fn render_explain_text(
    request: &TaskInvocation,
    resolved: &crate::resolver::ResolvedTarget,
    selection: &SelectionOutcome,
    deferral: &DeferralOutcome,
    candidates: &[String],
    catalogs: &[super::super::LoadedCatalog],
    verbose: bool,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer
        .section("Doctor Explain")
        .map_err(map_render_error)?;
    renderer
        .key_values(&[
            KeyValue::new("request", request.name.clone()),
            KeyValue::new("args", request.args.join(" ")),
            KeyValue::new(
                "resolved-root",
                resolved.resolved_root.display().to_string(),
            ),
            KeyValue::new("selection-status", selection.status.clone()),
            KeyValue::new(
                "selected-catalog",
                selection
                    .catalog
                    .clone()
                    .unwrap_or_else(|| "<none>".to_owned()),
            ),
            KeyValue::new(
                "selected-mode",
                selection
                    .mode
                    .clone()
                    .unwrap_or_else(|| "<none>".to_owned()),
            ),
            KeyValue::new("selection-reasoning", selection.reasoning.clone()),
            KeyValue::new("deferral-considered", deferral.considered.to_string()),
            KeyValue::new("deferral-selected", deferral.selected.to_string()),
            KeyValue::new("deferral-reasoning", deferral.reasoning.clone()),
        ])
        .map_err(map_render_error)?;
    if let Some(source) = deferral.source.as_ref() {
        renderer
            .key_values(&[KeyValue::new("deferral-source", source.clone())])
            .map_err(map_render_error)?;
    }
    if let Some(working_dir) = deferral.working_dir.as_ref() {
        renderer
            .key_values(&[KeyValue::new("deferral-working-dir", working_dir.clone())])
            .map_err(map_render_error)?;
    }
    if let Some(error) = selection.error.as_ref() {
        renderer
            .notice(NoticeLevel::Warning, error)
            .map_err(map_render_error)?;
    }
    renderer.text("").map_err(map_render_error)?;
    renderer
        .bullet_list("candidate-catalogs", candidates)
        .map_err(map_render_error)?;
    if !selection.evidence.is_empty() {
        renderer
            .bullet_list("selection-evidence", &selection.evidence)
            .map_err(map_render_error)?;
    }
    if !selection.ambiguity_candidates.is_empty() {
        renderer
            .bullet_list("ambiguity-candidates", &selection.ambiguity_candidates)
            .map_err(map_render_error)?;
    }
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
        renderer
            .bullet_list("discovered-catalogs", &all_catalogs)
            .map_err(map_render_error)?;
        if !resolved.evidence.is_empty() {
            renderer
                .bullet_list("root-resolution-evidence", &resolved.evidence)
                .map_err(map_render_error)?;
        }
        if !resolved.warnings.is_empty() {
            renderer
                .bullet_list("root-resolution-warnings", &resolved.warnings)
                .map_err(map_render_error)?;
        }
    }
    let out = renderer.into_inner();
    Ok(String::from_utf8_lossy(&out).to_string())
}

fn candidate_catalogs(
    catalogs: &[super::super::LoadedCatalog],
    selector: &super::super::TaskSelector,
    cwd: &std::path::Path,
) -> Vec<String> {
    let mut candidates = catalogs
        .iter()
        .filter(|catalog| {
            if selector
                .prefix
                .as_ref()
                .is_some_and(|prefix| prefix != &catalog.alias)
            {
                return false;
            }
            catalog.manifest.tasks.contains_key(&selector.task_name)
        })
        .map(|catalog| {
            format!(
                "{} ({}) depth={} in_scope={} has_defer={}",
                catalog.alias,
                catalog.manifest_path.display(),
                catalog.depth,
                cwd.starts_with(&catalog.catalog_root),
                catalog.defer_run.is_some()
            )
        })
        .collect::<Vec<String>>();
    candidates.sort();
    candidates
}

fn compute_selection_outcome(
    selection: &Result<super::super::TaskSelection<'_>, RunnerError>,
) -> SelectionOutcome {
    match selection {
        Ok(value) => {
            let mode = format_selection_mode(value.mode.clone());
            SelectionOutcome {
                status: "ok".to_owned(),
                catalog: Some(value.catalog.alias.clone()),
                mode: Some(mode.clone()),
                evidence: value.evidence.clone(),
                error: None,
                ambiguity_candidates: Vec::new(),
                reasoning: selection_reasoning(Some(mode), false),
            }
        }
        Err(error) => {
            let ambiguity_candidates = match &error {
                RunnerError::TaskAmbiguous { candidates, .. } => candidates.clone(),
                _ => Vec::new(),
            };
            SelectionOutcome {
                status: "error".to_owned(),
                catalog: None,
                mode: None,
                evidence: Vec::new(),
                error: Some(error.to_string()),
                ambiguity_candidates: ambiguity_candidates.clone(),
                reasoning: selection_reasoning(None, !ambiguity_candidates.is_empty()),
            }
        }
    }
}

fn selection_reasoning(mode: Option<String>, ambiguous: bool) -> String {
    if let Some(mode) = mode.as_deref() {
        return match mode {
            "explicit_prefix" => "selected catalog by explicit task prefix".to_owned(),
            "cwd_nearest" => {
                "selected nearest in-scope catalog from current working directory".to_owned()
            }
            "root_shallowest" => {
                "selected shallowest matching catalog from workspace root".to_owned()
            }
            _ => "selection completed".to_owned(),
        };
    }
    if ambiguous {
        "selection failed due to ambiguity across matching catalogs".to_owned()
    } else {
        "selection failed because no unambiguous task target was resolved".to_owned()
    }
}

fn compute_deferral_outcome(
    selection: &Result<super::super::TaskSelection<'_>, RunnerError>,
    selector: &super::super::TaskSelector,
    catalogs: &[super::super::LoadedCatalog],
    cwd: &std::path::Path,
    resolved_root: &std::path::Path,
) -> DeferralOutcome {
    let Err(error) = selection else {
        return deferral_not_considered();
    };
    let considered = should_attempt_deferral(error);
    if !considered {
        return deferral_not_considered();
    }
    if let Some(deferral) = select_deferral(selector, catalogs, cwd, resolved_root) {
        return DeferralOutcome {
            considered: true,
            selected: true,
            source: Some(deferral.source),
            working_dir: Some(deferral.working_dir.display().to_string()),
            reasoning: DEFERRAL_SELECTED_REASON.to_owned(),
        };
    }
    DeferralOutcome {
        considered: true,
        selected: false,
        source: None,
        working_dir: None,
        reasoning: DEFERRAL_NOT_FOUND_REASON.to_owned(),
    }
}

fn format_selection_mode(mode: CatalogSelectionMode) -> String {
    match mode {
        CatalogSelectionMode::ExplicitPrefix => "explicit_prefix".to_owned(),
        CatalogSelectionMode::CwdNearest => "cwd_nearest".to_owned(),
        CatalogSelectionMode::RootShallowest => "root_shallowest".to_owned(),
    }
}

fn deferral_not_considered() -> DeferralOutcome {
    DeferralOutcome {
        considered: false,
        selected: false,
        source: None,
        working_dir: None,
        reasoning: DEFERRAL_NOT_CONSIDERED_REASON.to_owned(),
    }
}

fn map_render_error(error: crate::ui::UiError) -> RunnerError {
    RunnerError::Ui(format!("failed to render doctor explain output: {error}"))
}
