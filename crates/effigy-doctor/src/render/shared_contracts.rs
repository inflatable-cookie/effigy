use serde_json::json;

pub(crate) fn root_resolution_payload(
    resolved_root: Option<&str>,
    evidence: &[String],
    warnings: &[String],
) -> serde_json::Value {
    match resolved_root {
        Some(resolved_root) => json!({
            "resolved_root": resolved_root,
            "evidence": evidence,
            "warnings": warnings,
        }),
        None => json!({
            "evidence": evidence,
            "warnings": warnings,
        }),
    }
}

pub(crate) struct ExplainSummary<'a> {
    pub(crate) request_task: &'a str,
    pub(crate) request_args: &'a [String],
    pub(crate) resolved_root: &'a str,
    pub(crate) selection_status: &'a str,
    pub(crate) selected_catalog: Option<&'a str>,
    pub(crate) selected_mode: Option<&'a str>,
    pub(crate) selection_reasoning: &'a str,
    pub(crate) deferral_considered: bool,
    pub(crate) deferral_selected: bool,
    pub(crate) deferral_reasoning: &'a str,
}

pub(crate) fn explain_summary_rows(summary: ExplainSummary<'_>) -> Vec<(String, String)> {
    let ExplainSummary {
        request_task,
        request_args,
        resolved_root,
        selection_status,
        selected_catalog,
        selected_mode,
        selection_reasoning,
        deferral_considered,
        deferral_selected,
        deferral_reasoning,
    } = summary;
    vec![
        ("request".to_owned(), request_task.to_owned()),
        ("args".to_owned(), request_args.join(" ")),
        ("resolved-root".to_owned(), resolved_root.to_owned()),
        ("selection-status".to_owned(), selection_status.to_owned()),
        (
            "selected-catalog".to_owned(),
            selected_catalog.unwrap_or("<none>").to_owned(),
        ),
        (
            "selected-mode".to_owned(),
            selected_mode.unwrap_or("<none>").to_owned(),
        ),
        (
            "selection-reasoning".to_owned(),
            selection_reasoning.to_owned(),
        ),
        (
            "deferral-considered".to_owned(),
            deferral_considered.to_string(),
        ),
        (
            "deferral-selected".to_owned(),
            deferral_selected.to_string(),
        ),
        (
            "deferral-reasoning".to_owned(),
            deferral_reasoning.to_owned(),
        ),
    ]
}
