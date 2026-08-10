use serde_json::json;

use effigy_cli::TaskInvocation;
use effigy_core::resolver::ResolvedTarget;

#[derive(Debug, Clone)]
pub(super) struct SelectionOutcome {
    pub(super) status: String,
    pub(super) catalog: Option<String>,
    pub(super) mode: Option<String>,
    pub(super) evidence: Vec<String>,
    pub(super) error: Option<String>,
    pub(super) ambiguity_candidates: Vec<String>,
    pub(super) reasoning: String,
}

#[derive(Debug, Clone)]
pub(super) struct DeferralOutcome {
    pub(super) considered: bool,
    pub(super) selected: bool,
    pub(super) source: Option<String>,
    pub(super) working_dir: Option<String>,
    pub(super) reasoning: String,
}

#[derive(Debug, Clone)]
pub(super) struct ExplainRenderContract {
    pub(super) request_task: String,
    pub(super) request_args: Vec<String>,
    pub(super) resolved_root: String,
    pub(super) root_evidence: Vec<String>,
    pub(super) root_warnings: Vec<String>,
    pub(super) selection_task: String,
    pub(super) selection: SelectionOutcome,
    pub(super) deferral: DeferralOutcome,
    pub(super) candidates: Vec<String>,
}

pub(super) fn build_explain_render_contract(
    request: &TaskInvocation,
    resolved: &ResolvedTarget,
    task_name: &str,
    selection: &SelectionOutcome,
    deferral: &DeferralOutcome,
    candidates: &[String],
) -> ExplainRenderContract {
    ExplainRenderContract {
        request_task: request.name.clone(),
        request_args: request.args.clone(),
        resolved_root: resolved.resolved_root.display().to_string(),
        root_evidence: resolved.evidence.clone(),
        root_warnings: resolved.warnings.clone(),
        selection_task: task_name.to_owned(),
        selection: selection.clone(),
        deferral: deferral.clone(),
        candidates: candidates.to_vec(),
    }
}

pub(super) fn explain_json_payload(contract: &ExplainRenderContract) -> serde_json::Value {
    json!({
        "schema": "effigy.doctor.explain.v1",
        "schema_version": 1,
        "request": {
            "task": &contract.request_task,
            "args": &contract.request_args,
        },
        "root_resolution": super::super::render::shared_contracts::root_resolution_payload(
            Some(&contract.resolved_root),
            &contract.root_evidence,
            &contract.root_warnings,
        ),
        "selection": {
            "status": &contract.selection.status,
            "catalog": &contract.selection.catalog,
            "task": &contract.selection_task,
            "mode": &contract.selection.mode,
            "evidence": &contract.selection.evidence,
            "error": &contract.selection.error,
        },
        "candidates": &contract.candidates,
        "ambiguity_candidates": &contract.selection.ambiguity_candidates,
        "deferral": {
            "considered": contract.deferral.considered,
            "selected": contract.deferral.selected,
            "source": &contract.deferral.source,
            "working_dir": &contract.deferral.working_dir,
        },
        "reasoning": {
            "selection": &contract.selection.reasoning,
            "deferral": &contract.deferral.reasoning,
        },
    })
}

pub(super) fn explain_text_summary_rows(contract: &ExplainRenderContract) -> Vec<(String, String)> {
    super::super::render::shared_contracts::explain_summary_rows(
        super::super::render::shared_contracts::ExplainSummary {
            request_task: &contract.request_task,
            request_args: &contract.request_args,
            resolved_root: &contract.resolved_root,
            selection_status: &contract.selection.status,
            selected_catalog: contract.selection.catalog.as_deref(),
            selected_mode: contract.selection.mode.as_deref(),
            selection_reasoning: &contract.selection.reasoning,
            deferral_considered: contract.deferral.considered,
            deferral_selected: contract.deferral.selected,
            deferral_reasoning: &contract.deferral.reasoning,
        },
    )
}
