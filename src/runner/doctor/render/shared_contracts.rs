use serde_json::json;

pub(in crate::runner) fn root_resolution_payload(
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

pub(in crate::runner) fn explain_summary_rows(
    request_task: &str,
    request_args: &[String],
    resolved_root: &str,
    selection_status: &str,
    selected_catalog: Option<&str>,
    selected_mode: Option<&str>,
    selection_reasoning: &str,
    deferral_considered: bool,
    deferral_selected: bool,
    deferral_reasoning: &str,
) -> Vec<(String, String)> {
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
