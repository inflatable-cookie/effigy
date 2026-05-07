use std::path::Path;

use effigy_containers::{
    AllocatedPortsSummary, ContainerCommandReport, ContainerStatusAllEntry, ContainerStatusService,
};
use effigy_gateway::ports::PortRegistry;

use super::DiscoveredRunningEnvironment;

pub(super) fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

pub(super) fn annotate_warning_lines(report: &mut ContainerCommandReport, warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert("warnings".to_owned(), serde_json::json!(warnings));
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
        profile: policy.profile.clone(),
        primary_service: policy.primary_service.clone(),
        dns_domain: policy.dns_domain.clone(),
        dns_tls: policy.dns_tls,
        declared_ports: policy.declared_ports.clone(),
        allocated_ports: load_port_registry()
            .as_ref()
            .and_then(|value| value.port_map(&policy.project_name))
            .map(|ports| AllocatedPortsSummary {
                base: ports.base,
                http: ports.http,
                mysql: ports.mysql,
                postgres: ports.postgres,
                redis: ports.redis,
                memcached: ports.memcached,
            }),
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
            })
            .collect(),
    }
}

fn load_port_registry() -> Option<PortRegistry> {
    let home = std::env::var_os("HOME")?;
    let path = Path::new(&home).join(".effigy/ports.json");
    PortRegistry::load(&path).ok()
}
