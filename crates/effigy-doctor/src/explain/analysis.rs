use effigy_manifest::{LoadedCatalog, TaskSelection};
use effigy_tasks::TaskSelector;

use super::contracts::{DeferralOutcome, SelectionOutcome};
use super::CatalogSelectionMode;
use crate::{DoctorError, DoctorRuntimePorts};

pub(super) fn candidate_catalogs(
    catalogs: &[LoadedCatalog],
    selector: &TaskSelector,
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

pub(super) fn compute_selection_outcome(
    selection: &Result<TaskSelection<'_>, DoctorError>,
) -> SelectionOutcome {
    match selection {
        Ok(value) => {
            let mode = format_selection_mode(value.mode);
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
            let ambiguity_candidates = ambiguity_candidates(error);
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

pub(super) fn compute_deferral_outcome(
    selection: &Result<TaskSelection<'_>, DoctorError>,
    selector: &TaskSelector,
    catalogs: &[LoadedCatalog],
    cwd: &std::path::Path,
    resolved_root: &std::path::Path,
    ports: &dyn DoctorRuntimePorts,
) -> DeferralOutcome {
    let Err(error) = selection else {
        return deferral_not_considered();
    };
    let considered = should_attempt_deferral(error);
    if !considered {
        return deferral_not_considered();
    }
    if let Some(deferral) = ports.select_deferral(selector, catalogs, cwd, resolved_root) {
        return DeferralOutcome {
            considered: true,
            selected: true,
            source: Some(deferral.source),
            working_dir: Some(deferral.working_dir.display().to_string()),
            reasoning: super::DEFERRAL_SELECTED_REASON.to_owned(),
        };
    }
    DeferralOutcome {
        considered: true,
        selected: false,
        source: None,
        working_dir: None,
        reasoning: super::DEFERRAL_NOT_FOUND_REASON.to_owned(),
    }
}

fn format_selection_mode(mode: CatalogSelectionMode) -> String {
    match mode {
        CatalogSelectionMode::ExplicitPrefix => "explicit_prefix".to_owned(),
        CatalogSelectionMode::CwdNearest => "cwd_nearest".to_owned(),
        CatalogSelectionMode::RootShallowest => "root_shallowest".to_owned(),
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

fn deferral_not_considered() -> DeferralOutcome {
    DeferralOutcome {
        considered: false,
        selected: false,
        source: None,
        working_dir: None,
        reasoning: super::DEFERRAL_NOT_CONSIDERED_REASON.to_owned(),
    }
}

fn should_attempt_deferral(error: &DoctorError) -> bool {
    use effigy_routing::RoutingError;
    matches!(
        error,
        DoctorError::Routing(
            RoutingError::TaskNotFoundAny { .. }
                | RoutingError::TaskCatalogPrefixNotFound { .. }
                | RoutingError::TaskNotFound { .. }
        )
    )
}

fn ambiguity_candidates(error: &DoctorError) -> Vec<String> {
    match error {
        DoctorError::Routing(effigy_routing::RoutingError::TaskAmbiguous {
            candidates, ..
        }) => candidates.clone(),
        _ => Vec::new(),
    }
}
