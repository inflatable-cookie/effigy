use effigy_state::{
    StateCaptureSetReport, StateStackApplyReport, StateStackCaptureReport, StateStackHistoryReport,
    StateStackLineageReport,
};

pub(in crate::runner) fn render_state_plan_text(report: &StateStackLineageReport) -> String {
    let mut lines = vec![
        "State stack plan".to_owned(),
        format!("schema: {}", report.schema),
        format!("stack: {}", report.stack_name),
        format!("environment: {:?}", report.environment),
        format!("lineage: {}", report.lineage_id),
        report
            .written_report_path
            .as_ref()
            .map(|path| format!("report: {path}"))
            .unwrap_or_else(|| "report: not written".to_owned()),
        report
            .written_history_path
            .as_ref()
            .map(|path| format!("history: {path}"))
            .unwrap_or_else(|| "history: not written".to_owned()),
        "layers:".to_owned(),
    ];
    for layer in &report.layers {
        lines.push(format!(
            "- {}: {:?} via {:?} ({:?})",
            layer.key, layer.role, layer.apply_mode, layer.environment_policy
        ));
    }
    if !report.artifact_reports.is_empty() {
        lines.push("artifact operations:".to_owned());
        for artifact in &report.artifact_reports {
            lines.push(format!(
                "- {}: {:?} {}",
                artifact.layer_key, artifact.operation, artifact.source_ref
            ));
        }
    }
    lines.join("\n")
}

pub(in crate::runner) fn render_state_apply_text(report: &StateStackApplyReport) -> String {
    let mut lines = vec![
        "State stack apply".to_owned(),
        format!("schema: {}", report.schema),
        format!("stack: {}", report.stack_name),
        format!("environment: {:?}", report.environment),
        format!("mode: {}", if report.executed { "execute" } else { "plan" }),
        report
            .written_report_path
            .as_ref()
            .map(|path| format!("report: {path}"))
            .unwrap_or_else(|| "report: not written".to_owned()),
        report
            .written_history_path
            .as_ref()
            .map(|path| format!("history: {path}"))
            .unwrap_or_else(|| "history: not written".to_owned()),
        "layers:".to_owned(),
    ];
    for layer in &report.layers {
        lines.push(format!(
            "- {}: {:?} via {:?} ({})",
            layer.key, layer.role, layer.apply_mode, layer.status
        ));
        if let Some(hook) = layer.hook.as_deref() {
            let hook_status = layer
                .hook_status
                .map(|status| status.to_string())
                .unwrap_or_else(|| "not-run".to_owned());
            lines.push(format!("  hook: {hook} ({hook_status})"));
        }
        if let Some(error) = layer.error.as_deref() {
            lines.push(format!("  error: {error}"));
        }
        if let Some(error) = layer.hook_error.as_deref() {
            lines.push(format!("  hook error: {error}"));
        }
    }
    lines.join("\n")
}

pub(in crate::runner) fn render_state_capture_text(report: &StateStackCaptureReport) -> String {
    let mut lines = vec![
        "State stack capture".to_owned(),
        format!("schema: {}", report.schema),
        format!("stack: {}", report.stack_name),
        format!("source environment: {}", report.source_environment),
        format!("mode: {}", report.capture_mode),
        format!(
            "execution: {}",
            if report.executed {
                "staged local artifact"
            } else {
                "plan-only"
            }
        ),
        report
            .written_report_path
            .as_ref()
            .map(|path| format!("report: {path}"))
            .unwrap_or_else(|| "report: not written".to_owned()),
        report
            .written_history_path
            .as_ref()
            .map(|path| format!("history: {path}"))
            .unwrap_or_else(|| "history: not written".to_owned()),
        "produced layers:".to_owned(),
    ];
    for layer in &report.produced_layers {
        lines.push(format!(
            "- {}: {:?} via {:?}",
            layer.key, layer.role, layer.apply_mode
        ));
    }
    if !report.capture_artifacts.is_empty() {
        lines.push("capture artifacts:".to_owned());
        for artifact in &report.capture_artifacts {
            lines.push(format!(
                "- {}: {}",
                artifact.layer_key,
                artifact
                    .ref_
                    .as_deref()
                    .unwrap_or("destination ref not specified")
            ));
        }
    }
    lines.join("\n")
}

pub(in crate::runner) fn render_state_capture_set_text(report: &StateCaptureSetReport) -> String {
    let mut lines = vec![
        "State capture set".to_owned(),
        format!("stack: {}", report.stack),
        format!("key: {}", report.key),
        format!("executed: {}", report.executed),
        format!("ok: {}", report.ok),
        report
            .written_report_path
            .as_ref()
            .map(|path| format!("report: {path}"))
            .unwrap_or_else(|| "report: not written".to_owned()),
        report
            .written_history_path
            .as_ref()
            .map(|path| format!("history: {path}"))
            .unwrap_or_else(|| "history: not written".to_owned()),
        "captures:".to_owned(),
    ];
    for capture in &report.captures {
        if let Some(error) = &capture.error {
            lines.push(format!("- {}: failed ({error})", capture.profile));
        } else {
            lines.push(format!("- {}: {}", capture.profile, capture.ok));
        }
    }
    lines.join("\n")
}

pub(in crate::runner) fn render_state_history_text(report: &StateStackHistoryReport) -> String {
    let mut lines = vec![
        "State stack history".to_owned(),
        format!("schema: {}", report.schema),
        format!("stack: {}", report.stack_name),
        format!("reports: {}", report.reports.len()),
    ];
    for item in &report.reports {
        lines.push(format!(
            "- {}: {} ({})",
            item.kind,
            item.path,
            item.lineage_id
                .as_deref()
                .or(item.parent_lineage_id.as_deref())
                .unwrap_or("lineage unknown")
        ));
    }
    if !report.warnings.is_empty() {
        lines.push("warnings:".to_owned());
        for warning in &report.warnings {
            lines.push(format!("- {warning}"));
        }
    }
    lines.join("\n")
}
