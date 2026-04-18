//! Report shaping for `effigy container` subcommands.
//!
//! Each subcommand shapes a JSON payload and a human-readable success
//! string from the same inputs. The runner keeps the IO (docker/colima
//! subprocess calls, signal handling) and delegates the presentation
//! contract to this module.

use effigy_catalog::volumes::VolumeClassification;
use serde_json::{json, Value as JsonValue};

use crate::compose::shutdown_label;
use crate::{driver_label, ContainerEjectResult, EffectiveContainerPolicy};

/// Shape returned by each container-command report builder.
///
/// Container commands surfaced via this module always succeed (failures
/// bubble up from the underlying docker/colima calls as runner errors).
/// The report therefore only carries a success text plus the json payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCommandReport {
    pub json: JsonValue,
    pub success_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerStatusService {
    pub name: String,
    pub container_name: String,
    pub status: String,
    pub ports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocatedPortsSummary {
    pub base: u16,
    pub http: u16,
    pub mysql: u16,
    pub postgres: u16,
    pub redis: u16,
    pub memcached: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerStatusAllEntry {
    pub repo_root: String,
    pub container: String,
    pub project_name: String,
    pub profile: String,
    pub primary_service: String,
    pub dns_domain: Option<String>,
    pub dns_tls: bool,
    pub declared_ports: Vec<String>,
    pub allocated_ports: Option<AllocatedPortsSummary>,
    pub services: Vec<ContainerStatusService>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerStatsService {
    pub name: String,
    pub container_name: String,
    pub status: String,
    pub cpu_percent: Option<String>,
    pub memory_usage: Option<String>,
    pub memory_percent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerStatsAllEntry {
    pub repo_root: String,
    pub container: String,
    pub project_name: String,
    pub profile: String,
    pub primary_service: String,
    pub services: Vec<ContainerStatsService>,
}

/// Build the `container up` detached-mode report.
pub fn up_detached_report(
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    health: Option<&'static str>,
) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.up.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "profile": policy.profile,
        "compose_file": policy.compose_file_display,
        "project_name": policy.project_name,
        "primary_service": policy.primary_service,
        "shared_services": shared_services_json(policy),
        "attach_mode": "detached",
        "colima_started": colima_started,
        "ports": policy.declared_ports,
        "mounts": policy.declared_mounts,
        "ui_tabs": policy.ui_tabs,
        "health": health,
    });
    let mut lines = Vec::new();
    if colima_started {
        lines.push(format!("[ok] started Colima profile `{}`", policy.profile));
    }
    lines.push(format!(
        "[ok] container `{}` is ready in detached mode",
        policy.name
    ));
    append_shared_service_lines(&mut lines, policy);
    lines.push(format!(
        "[next] inspect state with `effigy container {} status`",
        policy.name
    ));
    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

/// Build the `container down` report.
pub fn down_report(
    policy: &EffectiveContainerPolicy,
    colima_running: bool,
) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.down.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "profile": policy.profile,
        "shared_services": shared_services_json(policy),
        "colima_running": colima_running,
        "shutdown": shutdown_label(policy.shutdown),
    });
    let success_text = if colima_running {
        format!("[ok] stopped container environment `{}`", policy.name)
    } else {
        format!(
            "[ok] container environment `{}` was already down because Colima profile `{}` is not running",
            policy.name, policy.profile
        )
    };
    ContainerCommandReport { json, success_text }
}

/// Build the `container reset` report.
pub fn reset_report(
    policy: &EffectiveContainerPolicy,
    colima_running: bool,
    keep_data: bool,
    volume_actions: Option<&VolumeClassification>,
) -> ContainerCommandReport {
    let volume_actions = volume_actions.cloned().unwrap_or(VolumeClassification {
        remove: Vec::new(),
        keep: Vec::new(),
    });
    let json = json!({
        "schema": "effigy.container.reset.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "profile": policy.profile,
        "shared_services": shared_services_json(policy),
        "colima_running": colima_running,
        "keep_data": keep_data,
        "volumes": {
            "kept": volume_actions.keep,
            "removed": volume_actions.remove,
        },
    });
    let mut lines = vec![if colima_running {
        if keep_data {
            format!(
                "[ok] reset container environment `{}` and preserved persistent data volumes",
                policy.name
            )
        } else {
            format!(
                "[ok] reset container environment `{}` and removed compose-managed volumes",
                policy.name
            )
        }
    } else {
        format!(
            "[ok] skipped reset for `{}` because Colima profile `{}` is not running",
            policy.name, policy.profile
        )
    }];
    if colima_running && !volume_actions.keep.is_empty() {
        lines.push(format!("kept_volumes: {}", volume_actions.keep.join(", ")));
    }
    if colima_running && !volume_actions.remove.is_empty() {
        lines.push(format!(
            "removed_volumes: {}",
            volume_actions.remove.join(", ")
        ));
    }
    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

/// Build the `container eject` report.
pub fn eject_report(
    policy: &EffectiveContainerPolicy,
    result: &ContainerEjectResult,
) -> ContainerCommandReport {
    let compose_path = result.compose_path.display().to_string();
    let json = json!({
        "schema": "effigy.container.eject.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "compose_path": compose_path,
        "dockerfile_count": result.dockerfile_count,
        "config_count": result.config_count,
    });
    let success_text = format!(
        "[ok] ejected catalog-backed compose ownership for `{}` to {}",
        policy.name, compose_path
    );
    ContainerCommandReport { json, success_text }
}

/// Build the `container status` report.
pub fn status_report(
    policy: &EffectiveContainerPolicy,
    colima_running: bool,
    health: Option<&'static str>,
    compose_ps: Option<&str>,
) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.status.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "driver": "colima",
        "profile": policy.profile,
        "compose_file": policy.compose_file_display,
        "project_name": policy.project_name,
        "primary_service": policy.primary_service,
        "shared_services": shared_services_json(policy),
        "colima_running": colima_running,
        "health": health,
        "ports": policy.declared_ports,
        "mounts": policy.declared_mounts,
        "ui_tabs": policy.ui_tabs,
        "detach_timeout_secs": policy.detach_timeout_secs,
        "compose_ps": compose_ps,
    });

    let mut lines = vec![
        format!("[container] {}", policy.name),
        format!("driver: {}", driver_label(policy.driver)),
        format!("profile: {}", policy.profile),
        format!("compose_file: {}", policy.compose_file_display),
        format!("project_name: {}", policy.project_name),
        format!("primary_service: {}", policy.primary_service),
        format!("colima_running: {}", yes_no(colima_running)),
    ];
    if !policy.declared_ports.is_empty() {
        lines.push(format!("ports: {}", policy.declared_ports.join(", ")));
    }
    if !policy.declared_mounts.is_empty() {
        lines.push(format!("mounts: {}", policy.declared_mounts.join(", ")));
    }
    if !policy.ui_tabs.is_empty() {
        lines.push(format!("ui_tabs: {}", policy.ui_tabs.join(", ")));
    }
    append_shared_service_lines(&mut lines, policy);
    lines.push(format!(
        "detach_timeout_secs: {}",
        policy.detach_timeout_secs
    ));
    if let Some(health) = health {
        lines.push(format!("health: {health}"));
    }
    if let Some(compose_ps) = compose_ps {
        lines.push(String::new());
        lines.push("compose status:".to_owned());
        lines.push(compose_ps.trim().to_owned());
    }
    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

/// Build the `container status --all` report.
pub fn status_all_report(entries: &[ContainerStatusAllEntry]) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.status-all.v1",
        "schema_version": 1,
        "ok": true,
        "environment_count": entries.len(),
        "environments": entries.iter().map(|entry| {
            json!({
                "repo_root": entry.repo_root,
                "container": entry.container,
                "project_name": entry.project_name,
                "profile": entry.profile,
                "primary_service": entry.primary_service,
                "dns_domain": entry.dns_domain,
                "dns_tls": entry.dns_tls,
                "declared_ports": entry.declared_ports,
                "allocated_ports": entry.allocated_ports.as_ref().map(|ports| json!({
                    "base": ports.base,
                    "http": ports.http,
                    "mysql": ports.mysql,
                    "postgres": ports.postgres,
                    "redis": ports.redis,
                    "memcached": ports.memcached,
                })),
                "services": entry.services.iter().map(|service| {
                    json!({
                        "name": service.name,
                        "container_name": service.container_name,
                        "status": service.status,
                        "ports": service.ports,
                    })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    });

    if entries.is_empty() {
        return ContainerCommandReport {
            json,
            success_text: "[info] no running Effigy-managed container environments found"
                .to_owned(),
        };
    }

    let mut lines = vec![format!(
        "[ok] {} running Effigy-managed container environment{}",
        entries.len(),
        if entries.len() == 1 { "" } else { "s" }
    )];
    for entry in entries {
        lines.push(String::new());
        lines.push(format!("[container] {}", entry.container));
        lines.push(format!("repo: {}", entry.repo_root));
        lines.push(format!("project_name: {}", entry.project_name));
        lines.push(format!("profile: {}", entry.profile));
        lines.push(format!("primary_service: {}", entry.primary_service));
        if let Some(domain) = entry.dns_domain.as_deref() {
            lines.push(format!(
                "domain: {}{}",
                domain,
                if entry.dns_tls { " (tls)" } else { "" }
            ));
        }
        if !entry.declared_ports.is_empty() {
            lines.push(format!(
                "declared_ports: {}",
                entry.declared_ports.join(", ")
            ));
        }
        if let Some(ports) = entry.allocated_ports.as_ref() {
            lines.push(format!(
                "allocated_ports: base={}, http={}, mysql={}, postgres={}, redis={}, memcached={}",
                ports.base, ports.http, ports.mysql, ports.postgres, ports.redis, ports.memcached
            ));
        }
        lines.push(format!("services: {}", entry.services.len()));
        for service in &entry.services {
            let ports = if service.ports.is_empty() {
                "no published ports".to_owned()
            } else {
                service.ports.join(", ")
            };
            lines.push(format!(
                "- {} [{}] {} ({})",
                service.name, service.container_name, service.status, ports
            ));
        }
    }

    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

/// Build the `container stats --all` report.
pub fn stats_all_report(
    entries: &[ContainerStatsAllEntry],
    stats_warning: Option<&str>,
) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.stats-all.v1",
        "schema_version": 1,
        "ok": true,
        "environment_count": entries.len(),
        "stats_warning": stats_warning,
        "environments": entries.iter().map(|entry| {
            json!({
                "repo_root": entry.repo_root,
                "container": entry.container,
                "project_name": entry.project_name,
                "profile": entry.profile,
                "primary_service": entry.primary_service,
                "services": entry.services.iter().map(|service| {
                    json!({
                        "name": service.name,
                        "container_name": service.container_name,
                        "status": service.status,
                        "cpu_percent": service.cpu_percent,
                        "memory_usage": service.memory_usage,
                        "memory_percent": service.memory_percent,
                        "stats_available": service.cpu_percent.is_some()
                            || service.memory_usage.is_some()
                            || service.memory_percent.is_some(),
                    })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    });

    if entries.is_empty() {
        return ContainerCommandReport {
            json,
            success_text: "[info] no running Effigy-managed container environments found"
                .to_owned(),
        };
    }

    let mut lines = vec![format!(
        "[ok] {} running Effigy-managed container environment{}",
        entries.len(),
        if entries.len() == 1 { "" } else { "s" }
    )];
    if let Some(warning) = stats_warning {
        lines.push(format!("[warn] {warning}"));
    }
    for entry in entries {
        lines.push(String::new());
        lines.push(format!("[container] {}", entry.container));
        lines.push(format!("repo: {}", entry.repo_root));
        lines.push(format!("project_name: {}", entry.project_name));
        lines.push(format!("profile: {}", entry.profile));
        lines.push(format!("primary_service: {}", entry.primary_service));
        lines.push(format!("services: {}", entry.services.len()));
        for service in &entry.services {
            let cpu = service.cpu_percent.as_deref().unwrap_or("unavailable");
            let memory = service.memory_usage.as_deref().unwrap_or("unavailable");
            let memory_percent = service.memory_percent.as_deref().unwrap_or("unavailable");
            lines.push(format!(
                "- {} [{}] {} (cpu={}, memory={}, memory_percent={})",
                service.name, service.container_name, service.status, cpu, memory, memory_percent
            ));
        }
    }

    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

/// Build the `container logs` (non-follow) report.
///
/// `rendered` is the trimmed stdout body produced by `docker compose logs`.
pub fn logs_report(
    policy: &EffectiveContainerPolicy,
    service: &str,
    rendered: &str,
) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.logs.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "service": service,
        "logs": rendered,
    });
    ContainerCommandReport {
        json,
        success_text: rendered.to_owned(),
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn shared_services_json(policy: &EffectiveContainerPolicy) -> Vec<JsonValue> {
    policy
        .shared_services
        .iter()
        .map(|service| {
            json!({
                "service_name": service.service_name,
                "catalog": service.catalog,
                "project_name": service.project_name,
                "target_host": service.host,
                "target_port": service.host_port,
                "container_port": service.container_port,
            })
        })
        .collect()
}

fn append_shared_service_lines(lines: &mut Vec<String>, policy: &EffectiveContainerPolicy) {
    if policy.shared_services.is_empty() {
        return;
    }
    lines.push(format!("shared_services: {}", policy.shared_services.len()));
    for service in &policy.shared_services {
        lines.push(format!(
            "- {} [{}] -> {}:{} (project={})",
            service.service_name,
            service.catalog,
            service.host,
            service.host_port,
            service.project_name
        ));
    }
}
