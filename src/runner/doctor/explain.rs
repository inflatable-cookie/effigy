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

    let selection = select_catalog_and_task(&selector, &catalogs, &cwd);
    let (selection_status, selected_catalog, selected_mode, selected_evidence, selection_error) =
        match &selection {
            Ok(value) => (
                "ok".to_owned(),
                Some(value.catalog.alias.clone()),
                Some(format_selection_mode(value.mode.clone())),
                value.evidence.clone(),
                None,
            ),
            Err(error) => (
                "error".to_owned(),
                None,
                None,
                Vec::new(),
                Some(error.to_string()),
            ),
        };

    let ambiguity_candidates = match &selection {
        Err(RunnerError::TaskAmbiguous { candidates, .. }) => candidates.clone(),
        _ => Vec::new(),
    };
    let selection_reasoning = if let Some(mode) = selected_mode.as_deref() {
        match mode {
            "explicit_prefix" => "selected catalog by explicit task prefix".to_owned(),
            "cwd_nearest" => {
                "selected nearest in-scope catalog from current working directory".to_owned()
            }
            "root_shallowest" => {
                "selected shallowest matching catalog from workspace root".to_owned()
            }
            _ => "selection completed".to_owned(),
        }
    } else if !ambiguity_candidates.is_empty() {
        "selection failed due to ambiguity across matching catalogs".to_owned()
    } else {
        "selection failed because no unambiguous task target was resolved".to_owned()
    };

    let mut deferral_considered = false;
    let mut deferral_selected = false;
    let mut deferral_source: Option<String> = None;
    let mut deferral_working_dir: Option<String> = None;
    if let Err(error) = &selection {
        deferral_considered = should_attempt_deferral(error);
        if deferral_considered {
            if let Some(deferral) =
                select_deferral(&selector, &catalogs, &cwd, &resolved.resolved_root)
            {
                deferral_selected = true;
                deferral_source = Some(deferral.source);
                deferral_working_dir = Some(deferral.working_dir.display().to_string());
            }
        }
    }
    let deferral_reasoning = if !deferral_considered {
        "deferral was not considered because the selection outcome does not trigger deferral"
            .to_owned()
    } else if deferral_selected {
        "deferral was selected from configured or implicit fallback routing".to_owned()
    } else {
        "deferral was considered but no eligible fallback route was found".to_owned()
    };

    if output_json {
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
                "status": selection_status,
                "catalog": selected_catalog,
                "task": selector.task_name,
                "mode": selected_mode,
                "evidence": selected_evidence,
                "error": selection_error,
            },
            "candidates": candidates,
            "ambiguity_candidates": ambiguity_candidates,
            "deferral": {
                "considered": deferral_considered,
                "selected": deferral_selected,
                "source": deferral_source,
                "working_dir": deferral_working_dir,
            },
            "reasoning": {
                "selection": selection_reasoning,
                "deferral": deferral_reasoning,
            },
        });
        return serde_json::to_string_pretty(&payload)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    let _ = renderer.section("Doctor Explain");
    let _ = renderer.key_values(&[
        KeyValue::new("request", request.name),
        KeyValue::new("args", request.args.join(" ")),
        KeyValue::new(
            "resolved-root",
            resolved.resolved_root.display().to_string(),
        ),
        KeyValue::new("selection-status", selection_status),
        KeyValue::new(
            "selected-catalog",
            selected_catalog.unwrap_or_else(|| "<none>".to_owned()),
        ),
        KeyValue::new(
            "selected-mode",
            selected_mode.unwrap_or_else(|| "<none>".to_owned()),
        ),
        KeyValue::new("selection-reasoning", selection_reasoning),
        KeyValue::new("deferral-considered", deferral_considered.to_string()),
        KeyValue::new("deferral-selected", deferral_selected.to_string()),
        KeyValue::new("deferral-reasoning", deferral_reasoning),
    ]);
    if let Some(source) = deferral_source {
        let _ = renderer.key_values(&[KeyValue::new("deferral-source", source)]);
    }
    if let Some(working_dir) = deferral_working_dir {
        let _ = renderer.key_values(&[KeyValue::new("deferral-working-dir", working_dir)]);
    }
    if let Some(error) = selection_error {
        let _ = renderer.notice(NoticeLevel::Warning, &error);
    }
    let _ = renderer.text("");
    let _ = renderer.bullet_list("candidate-catalogs", &candidates);
    if !selected_evidence.is_empty() {
        let _ = renderer.bullet_list("selection-evidence", &selected_evidence);
    }
    if !ambiguity_candidates.is_empty() {
        let _ = renderer.bullet_list("ambiguity-candidates", &ambiguity_candidates);
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
        let _ = renderer.bullet_list("discovered-catalogs", &all_catalogs);
        if !resolved.evidence.is_empty() {
            let _ = renderer.bullet_list("root-resolution-evidence", &resolved.evidence);
        }
        if !resolved.warnings.is_empty() {
            let _ = renderer.bullet_list("root-resolution-warnings", &resolved.warnings);
        }
    }

    let out = renderer.into_inner();
    Ok(String::from_utf8_lossy(&out).to_string())
}

fn format_selection_mode(mode: CatalogSelectionMode) -> String {
    match mode {
        CatalogSelectionMode::ExplicitPrefix => "explicit_prefix".to_owned(),
        CatalogSelectionMode::CwdNearest => "cwd_nearest".to_owned(),
        CatalogSelectionMode::RootShallowest => "root_shallowest".to_owned(),
    }
}
