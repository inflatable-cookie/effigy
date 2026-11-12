use super::report::{DeployHistoryReport, DeployPlanReport, DeployStatusReport};

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
    if let Some(provider) = &report.provider_provenance {
        lines.push(format!("provider: {}", provider.describe()));
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
    let mut text = format!(
        "[deploy] status {}\nactive: {active}\nlatest: {latest}",
        report.env
    );
    if let Some(provider) = &report.provider_provenance {
        text.push_str(&format!("\nprovider: {}", provider.describe()));
    }
    text
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

#[cfg(test)]
mod tests {
    use super::{render_deploy_history_text, render_deploy_status_text};
    use crate::runner::deploy_command::report::{
        DeployHistoryItem, DeployHistoryReport, DeployStatusReport, HISTORY_SCHEMA, STATUS_SCHEMA,
    };

    #[test]
    fn history_text_names_the_environment_and_every_attempt() {
        let report = DeployHistoryReport {
            schema: HISTORY_SCHEMA.to_owned(),
            schema_version: 1,
            env: "uat".to_owned(),
            entries: vec![DeployHistoryItem {
                path: ".effigy/reports/deploy/uat/history/1700000000-uat-abc.json".to_owned(),
                deployment_id: "1700000000-uat-abc".to_owned(),
                schema: "effigy.deploy.plan.v1".to_owned(),
                status: "planned".to_owned(),
            }],
            warnings: Vec::new(),
        };

        assert_eq!(
            render_deploy_history_text(&report),
            "[deploy] history uat (1 entries)\n- 1700000000-uat-abc [planned] .effigy/reports/deploy/uat/history/1700000000-uat-abc.json"
        );
    }

    #[test]
    fn status_text_says_present_or_missing_for_each_report() {
        let report = DeployStatusReport {
            schema: STATUS_SCHEMA.to_owned(),
            schema_version: 1,
            env: "production".to_owned(),
            active_path: Some(".effigy/runtime/deploy/active/production.json".to_owned()),
            latest_path: None,
            active: Some(serde_json::json!({})),
            latest: None,
            provider_status: None,
            provider_provenance: None,
            warnings: Vec::new(),
        };

        assert_eq!(
            render_deploy_status_text(&report),
            "[deploy] status production\nactive: present\nlatest: missing"
        );
    }
}
