use effigy_containers::{ContainerCommandReport, EffectiveContainerPolicy};

use crate::EffigyRuntimeError;

pub(super) struct StoppedContainerEnvironment {
    pub(super) repo_root: String,
    pub(super) container: String,
    pub(super) project_name: String,
    pub(super) profile: String,
    pub(super) removed_gateway_domains: Vec<String>,
    pub(super) left_running_shared_services: Vec<String>,
    pub(super) runtime_was_running: bool,
}

pub(super) fn render_container_down_all_report(
    stopped: &[StoppedContainerEnvironment],
    output_json: bool,
) -> Result<String, EffigyRuntimeError> {
    if output_json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "schema": "effigy.container.down-all.v1",
            "schema_version": 1,
            "ok": true,
            "count": stopped.len(),
            "environments": stopped.iter().map(|entry| {
                serde_json::json!({
                    "repo_root": entry.repo_root,
                    "container": entry.container,
                    "project_name": entry.project_name,
                    "profile": entry.profile,
                    "runtime_was_running": entry.runtime_was_running,
                    "removed_gateway_domains": entry.removed_gateway_domains,
                    "left_running_shared_services": entry.left_running_shared_services,
                })
            }).collect::<Vec<_>>(),
        }))
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()));
    }

    if stopped.is_empty() {
        return Ok("[ok] no running Effigy-managed container environments found".to_owned());
    }

    let mut lines = vec![format!(
        "[ok] stopped {} running Effigy-managed container environment{}",
        stopped.len(),
        if stopped.len() == 1 { "" } else { "s" }
    )];
    for entry in stopped {
        lines.push(format!(
            "{} ({}) [{}]",
            entry.repo_root, entry.container, entry.profile
        ));
        if !entry.removed_gateway_domains.is_empty() {
            lines.push(format!(
                "[info] removed gateway routes: {}",
                entry.removed_gateway_domains.join(", ")
            ));
        }
        if !entry.left_running_shared_services.is_empty() {
            lines.push(format!(
                "[warn] shared services left running: {}",
                entry.left_running_shared_services.join(", ")
            ));
        }
    }
    Ok(lines.join("\n"))
}

pub(super) fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

pub(super) fn annotate_removed_gateway_routes(
    report: &mut ContainerCommandReport,
    domains: &[String],
) {
    if domains.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_routes".to_owned(),
            serde_json::json!(domains
                .iter()
                .map(|domain| serde_json::json!({
                    "action": "removed",
                    "domain": domain,
                }))
                .collect::<Vec<_>>()),
        );
    }
    for domain in domains {
        report.success_text.push('\n');
        report
            .success_text
            .push_str(&format!("[gateway] removed {domain}"));
    }
}

pub(super) fn annotate_left_running_shared_services(
    report: &mut ContainerCommandReport,
    policy: &EffectiveContainerPolicy,
) {
    if policy.shared_services.is_empty() {
        return;
    }
    let notes = policy
        .shared_services
        .iter()
        .map(|service| {
            format!(
                "{} [{}] left running at {}:{}",
                service.service_name, service.catalog, service.host, service.host_port
            )
        })
        .collect::<Vec<_>>();
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "shared_service_actions".to_owned(),
            serde_json::json!({
                "action": "left-running",
                "services": notes,
            }),
        );
    }
    for note in notes {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[shared] {note}"));
    }
}

#[cfg(test)]
mod tests {
    use super::{render_container_down_all_report, StoppedContainerEnvironment};

    #[test]
    fn container_down_all_report_renders_text_and_json() {
        let stopped = vec![StoppedContainerEnvironment {
            repo_root: "/tmp/alpha".to_owned(),
            container: "web".to_owned(),
            project_name: "alpha-dev".to_owned(),
            profile: "effigy".to_owned(),
            removed_gateway_domains: vec!["alpha.test".to_owned()],
            left_running_shared_services: vec!["db".to_owned()],
            runtime_was_running: true,
        }];

        let text = render_container_down_all_report(&stopped, false).expect("render text report");
        assert!(text.contains("[ok] stopped 1 running Effigy-managed container environment"));
        assert!(text.contains("/tmp/alpha (web) [effigy]"));
        assert!(text.contains("[info] removed gateway routes: alpha.test"));
        assert!(text.contains("[warn] shared services left running: db"));

        let json = render_container_down_all_report(&stopped, true).expect("render json report");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json report");
        assert_eq!(parsed["schema"], "effigy.container.down-all.v1");
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["environments"][0]["repo_root"], "/tmp/alpha");
        assert_eq!(parsed["environments"][0]["container"], "web");
        assert_eq!(
            parsed["environments"][0]["removed_gateway_domains"][0],
            "alpha.test"
        );
        assert_eq!(
            parsed["environments"][0]["left_running_shared_services"][0],
            "db"
        );
    }
}
