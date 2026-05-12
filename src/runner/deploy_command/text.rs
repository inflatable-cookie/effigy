use super::report::{DeployApplyReport, DeployHistoryReport, DeployPlanReport, DeployStatusReport};

pub(super) fn render_deploy_plan_text(report: &DeployPlanReport) -> String {
    let mut lines = vec![
        format!(
            "[deploy] planned {} deployment to {}",
            report.provider, report.env
        ),
        format!("deployment: {}", report.deployment_id),
        format!("code: {}", report.code.requested_ref),
        format!("release_policy: {}", report.release_policy.mode),
    ];
    if let Some(state) = &report.state {
        lines.push(format!("state: {} ({})", state.stack, state.lineage_id));
    }
    if !report.blockers.is_empty() {
        lines.push(String::new());
        lines.push(format!("Blockers ({})", report.blockers.len()));
        lines.extend(report.blockers.iter().map(|blocker| format!("- {blocker}")));
    }
    if !report.warnings.is_empty() {
        lines.push(String::new());
        lines.push(format!("Warnings ({})", report.warnings.len()));
        lines.extend(report.warnings.iter().map(|warning| format!("- {warning}")));
    }
    lines.join("\n")
}

pub(super) fn render_deploy_apply_text(report: &DeployApplyReport) -> String {
    format!(
        "[deploy] {} {} deployment to {}\ndeployment: {}\nreport: {}",
        report.status,
        report.provider,
        report.env,
        report.deployment_id,
        report
            .written_report_path
            .as_deref()
            .unwrap_or("<not written>")
    )
}

pub(super) fn render_deploy_status_text(report: &DeployStatusReport) -> String {
    let latest = if report.latest.is_some() {
        "present"
    } else {
        "missing"
    };
    let active = if report.active.is_some() {
        "present"
    } else {
        "missing"
    };
    format!(
        "[deploy] status {}\nactive: {active}\nlatest: {latest}",
        report.env
    )
}

pub(super) fn render_deploy_history_text(report: &DeployHistoryReport) -> String {
    let mut lines = vec![format!(
        "[deploy] history {} ({} entries)",
        report.env,
        report.entries.len()
    )];
    lines.extend(report.entries.iter().map(|entry| {
        format!(
            "- {} [{}] {}",
            entry.deployment_id, entry.status, entry.path
        )
    }));
    lines.join("\n")
}
