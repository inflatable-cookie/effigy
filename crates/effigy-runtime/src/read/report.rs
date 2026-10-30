use effigy_containers::{ContainerCommandReport, ContainerStatusAllEntry, ContainerStatusService};

use super::DiscoveredRunningEnvironment;

pub(super) fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

pub(super) fn annotate_warning_lines(report: &mut ContainerCommandReport, warnings: &[String]) {
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("warnings".to_owned(), serde_json::json!(warnings));
    }
    if warnings.is_empty() {
        return;
    }
    for warning in warnings {
        report.success_text.push('\n');
        report.success_text.push_str(&format!("[warn] {warning}"));
    }
}

pub(super) fn environment_status_entry(
    environment: &DiscoveredRunningEnvironment,
) -> ContainerStatusAllEntry {
    let policy = &environment.policy;
    ContainerStatusAllEntry {
        repo_root: environment.repo_root.clone(),
        container: policy.name.clone(),
        project_name: policy.project_name.clone(),
        profile: environment.runtime_profile.clone(),
        primary_service: policy.primary_service.clone(),
        dns_domain: policy.dns_domain.clone(),
        dns_tls: policy.dns_tls,
        declared_ports: policy.declared_ports.clone(),
        services: environment
            .services
            .iter()
            .map(|service| ContainerStatusService {
                name: service
                    .service
                    .clone()
                    .unwrap_or_else(|| service.container_name.clone()),
                container_name: service.container_name.clone(),
                status: service.status.clone(),
                ports: service.ports.clone(),
                health: None,
            })
            .collect(),
    }
}
