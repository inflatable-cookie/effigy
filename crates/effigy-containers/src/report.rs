//! Report shaping for `effigy container` subcommands.
//!
//! Each subcommand shapes a JSON payload and a human-readable success
//! string from the same inputs. The runner keeps the IO (docker/colima
//! subprocess calls, signal handling) and delegates the presentation
//! contract to this module.

use effigy_catalog::volumes::VolumeClassification;
use serde_json::{json, Value as JsonValue};
use std::collections::BTreeMap;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerDataVolumeEntry {
    pub name: String,
    pub service: String,
    pub persist: bool,
    pub size_bytes: Option<u64>,
    pub mount_point: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCacheVolumeEntry {
    pub name: String,
    pub service: String,
    pub kind: String,
    pub size_bytes: Option<u64>,
    pub mount_point: Option<String>,
    pub mount_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCacheGlobalEntry {
    pub name: String,
    pub kind: String,
    pub size_bytes: Option<u64>,
    pub mount_point: Option<String>,
    pub project_name: Option<String>,
    pub in_use: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCachePruneEntry {
    pub name: String,
    pub kind: String,
    pub size_bytes: Option<u64>,
    pub project_name: Option<String>,
    pub removed: bool,
    pub in_use: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerDataTransferAction {
    Export,
    Import,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerDataHookResult {
    pub hook: String,
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
        "media_mounts": policy.declared_media_mounts,
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
    preserve_persistent_data: bool,
    wipe_data: bool,
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
        "keep_data": preserve_persistent_data,
        "wipe_data": wipe_data,
        "volumes": {
            "kept": volume_actions.keep,
            "removed": volume_actions.remove,
        },
    });
    let mut lines = vec![if colima_running {
        if preserve_persistent_data {
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

/// Build the `container data pull-production` report.
pub fn data_pull_production_report(
    policy: &EffectiveContainerPolicy,
    result: &ContainerDataHookResult,
    colima_started: bool,
    health: Option<&'static str>,
) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.data-pull-production.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "profile": policy.profile,
        "compose_file": policy.compose_file_display,
        "project_name": policy.project_name,
        "primary_service": policy.primary_service,
        "shared_services": shared_services_json(policy),
        "hook": result.hook,
        "colima_started": colima_started,
        "health": health,
    });
    let mut lines = Vec::new();
    if colima_started {
        lines.push(format!("[ok] started Colima profile `{}`", policy.profile));
    }
    lines.push(format!(
        "[ok] ran production data hook for container `{}`",
        policy.name
    ));
    lines.push(format!("hook: {}", result.hook));
    if let Some(health) = health {
        lines.push(format!("health: {health}"));
    }
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
        "detach_timeout_secs": policy.detach_timeout_secs,
        "compose_ps": compose_ps,
        "media_mounts": policy.declared_media_mounts,
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
    if !policy.declared_media_mounts.is_empty() {
        lines.push(format!(
            "media_mounts: {}",
            policy.declared_media_mounts.join(", ")
        ));
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

pub fn data_list_report(
    policy: &EffectiveContainerPolicy,
    colima_running: bool,
    volumes: &[ContainerDataVolumeEntry],
) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.data-list.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "profile": policy.profile,
        "project_name": policy.project_name,
        "compose_source": match policy.compose_source {
            crate::EffectiveComposeSource::Direct => "direct",
            crate::EffectiveComposeSource::Generated => "generated",
        },
        "colima_running": colima_running,
        "volume_count": volumes.len(),
        "volumes": volumes.iter().map(|volume| {
            json!({
                "name": volume.name,
                "service": volume.service,
                "classification": if volume.persist { "persistent" } else { "ephemeral" },
                "persist": volume.persist,
                "size_bytes": volume.size_bytes,
                "mount_point": volume.mount_point,
                "size_available": volume.size_bytes.is_some(),
            })
        }).collect::<Vec<_>>(),
    });

    if volumes.is_empty() {
        return ContainerCommandReport {
            json,
            success_text: format!(
                "[info] container `{}` has no Effigy-managed named volumes",
                policy.name
            ),
        };
    }

    let mut lines = vec![
        format!(
            "[ok] {} managed data volume{} for `{}`",
            volumes.len(),
            if volumes.len() == 1 { "" } else { "s" },
            policy.name
        ),
        format!("project_name: {}", policy.project_name),
        format!(
            "runtime_metadata: {}",
            if colima_running {
                "best-effort"
            } else {
                "unavailable (Colima profile is not running)"
            }
        ),
    ];
    for volume in volumes {
        let classification = if volume.persist {
            "persistent"
        } else {
            "ephemeral"
        };
        let size = volume
            .size_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "unavailable".to_owned());
        let mount_point = volume.mount_point.as_deref().unwrap_or("unavailable");
        lines.push(format!(
            "- {} [{}] {} (size={}, mount_point={})",
            volume.name, volume.service, classification, size, mount_point
        ));
    }

    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

pub fn cache_list_report(
    policy: &EffectiveContainerPolicy,
    colima_running: bool,
    volumes: &[ContainerCacheVolumeEntry],
) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.cache-list.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "profile": policy.profile,
        "project_name": policy.project_name,
        "compose_source": match policy.compose_source {
            crate::EffectiveComposeSource::Direct => "direct",
            crate::EffectiveComposeSource::Generated => "generated",
        },
        "colima_running": colima_running,
        "cache_count": volumes.len(),
        "caches": volumes.iter().map(|volume| {
            json!({
                "name": volume.name,
                "service": volume.service,
                "kind": volume.kind,
                "size_bytes": volume.size_bytes,
                "mount_point": volume.mount_point,
                "mount_target": volume.mount_target,
                "safe_to_purge": true,
                "size_available": volume.size_bytes.is_some(),
            })
        }).collect::<Vec<_>>(),
    });

    if volumes.is_empty() {
        return ContainerCommandReport {
            json,
            success_text: format!(
                "[info] container `{}` has no purge-safe isolated cache volumes",
                policy.name
            ),
        };
    }

    let mut lines = vec![
        format!(
            "[ok] {} purge-safe cache volume{} for `{}`",
            volumes.len(),
            if volumes.len() == 1 { "" } else { "s" },
            policy.name
        ),
        format!("project_name: {}", policy.project_name),
        format!(
            "runtime_metadata: {}",
            if colima_running {
                "best-effort"
            } else {
                "unavailable (Colima profile is not running)"
            }
        ),
    ];
    for volume in volumes {
        let size = volume
            .size_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "unavailable".to_owned());
        let mount_target = volume.mount_target.as_deref().unwrap_or("unavailable");
        lines.push(format!(
            "- {} [{}] {} (size={}, target={})",
            volume.name, volume.service, volume.kind, size, mount_target
        ));
    }

    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

pub fn cache_list_all_report(
    profile: &str,
    volumes: &[ContainerCacheGlobalEntry],
) -> ContainerCommandReport {
    let in_use_count = volumes.iter().filter(|volume| volume.in_use).count();
    let mut grouped = BTreeMap::<String, Vec<&ContainerCacheGlobalEntry>>::new();
    for volume in volumes {
        grouped
            .entry(
                volume
                    .project_name
                    .clone()
                    .unwrap_or_else(|| "unknown".to_owned()),
            )
            .or_default()
            .push(volume);
    }
    let json = json!({
        "schema": "effigy.container.cache-list-all.v1",
        "schema_version": 1,
        "ok": true,
        "profile": profile,
        "cache_count": volumes.len(),
        "in_use_count": in_use_count,
        "available_count": volumes.len().saturating_sub(in_use_count),
        "projects": grouped.iter().map(|(project, caches)| {
            json!({
                "project_name": project,
                "cache_count": caches.len(),
                "in_use_count": caches.iter().filter(|cache| cache.in_use).count(),
                "caches": caches.iter().map(|volume| {
                    json!({
                        "name": volume.name,
                        "kind": volume.kind,
                        "size_bytes": volume.size_bytes,
                        "mount_point": volume.mount_point,
                        "project_name": volume.project_name,
                        "in_use": volume.in_use,
                        "safe_to_purge": !volume.in_use,
                        "size_available": volume.size_bytes.is_some(),
                    })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "caches": volumes.iter().map(|volume| {
            json!({
                "name": volume.name,
                "kind": volume.kind,
                "size_bytes": volume.size_bytes,
                "mount_point": volume.mount_point,
                "project_name": volume.project_name,
                "in_use": volume.in_use,
                "safe_to_purge": !volume.in_use,
                "size_available": volume.size_bytes.is_some(),
            })
        }).collect::<Vec<_>>(),
    });

    if volumes.is_empty() {
        return ContainerCommandReport {
            json,
            success_text: format!(
                "[info] no purge-safe cache volumes found in Colima profile `{profile}`"
            ),
        };
    }

    let mut lines = vec![format!(
        "[ok] {} purge-safe cache volume{} in Colima profile `{}` (in_use={}, purgeable={})",
        volumes.len(),
        if volumes.len() == 1 { "" } else { "s" },
        profile,
        in_use_count,
        volumes.len().saturating_sub(in_use_count),
    )];
    for (project, caches) in grouped {
        lines.push(format!("{project}:"));
        for volume in caches {
            let size = volume
                .size_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "unavailable".to_owned());
            lines.push(format!(
                "- {} {} (size={}, {})",
                volume.name,
                volume.kind,
                size,
                if volume.in_use { "in-use" } else { "purgeable" },
            ));
        }
    }

    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

pub fn cache_prune_report(
    scope_label: &str,
    entries: &[ContainerCachePruneEntry],
) -> ContainerCommandReport {
    let removed_count = entries.iter().filter(|entry| entry.removed).count();
    let skipped_count = entries.len().saturating_sub(removed_count);
    let json = json!({
        "schema": "effigy.container.cache-prune.v1",
        "schema_version": 1,
        "ok": true,
        "scope": scope_label,
        "removed_count": removed_count,
        "skipped_count": skipped_count,
        "caches": entries.iter().map(|entry| {
            json!({
                "name": entry.name,
                "kind": entry.kind,
                "size_bytes": entry.size_bytes,
                "project_name": entry.project_name,
                "removed": entry.removed,
                "in_use": entry.in_use,
            })
        }).collect::<Vec<_>>(),
    });

    if entries.is_empty() {
        return ContainerCommandReport {
            json,
            success_text: format!("[info] no purge-safe cache volumes matched {scope_label}"),
        };
    }

    let mut lines = vec![format!(
        "[ok] removed {} cache volume{} and skipped {} for {}",
        removed_count,
        if removed_count == 1 { "" } else { "s" },
        skipped_count,
        scope_label
    )];
    for entry in entries {
        let size = entry
            .size_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "unavailable".to_owned());
        let project = entry.project_name.as_deref().unwrap_or("unknown");
        lines.push(format!(
            "- {} {} (size={}, project={}, {})",
            entry.name,
            entry.kind,
            size,
            project,
            if entry.removed {
                "removed"
            } else if entry.in_use {
                "skipped: in-use"
            } else {
                "skipped"
            }
        ));
    }

    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

pub fn data_transfer_report(
    policy: &EffectiveContainerPolicy,
    action: ContainerDataTransferAction,
    volume: &ContainerDataVolumeEntry,
    archive_path: &std::path::Path,
) -> ContainerCommandReport {
    let action_label = match action {
        ContainerDataTransferAction::Export => "export",
        ContainerDataTransferAction::Import => "import",
    };
    let path_label = match action {
        ContainerDataTransferAction::Export => "output_path",
        ContainerDataTransferAction::Import => "input_path",
    };
    let schema = match action {
        ContainerDataTransferAction::Export => "effigy.container.data-export.v1",
        ContainerDataTransferAction::Import => "effigy.container.data-import.v1",
    };
    let archive_path = archive_path.display().to_string();
    let mut json = json!({
        "schema": schema,
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "profile": policy.profile,
        "project_name": policy.project_name,
        "action": action_label,
        "volume": {
            "name": volume.name,
            "service": volume.service,
            "classification": if volume.persist { "persistent" } else { "ephemeral" },
            "persist": volume.persist,
        },
    });
    if let Some(json_object) = json.as_object_mut() {
        json_object.insert(path_label.to_owned(), json!(archive_path));
    }
    let success_text = format!(
        "[ok] {}ed managed volume `{}` for `{}` {} {}",
        action_label,
        volume.name,
        policy.name,
        if matches!(action, ContainerDataTransferAction::Export) {
            "to"
        } else {
            "from"
        },
        archive_path
    );
    ContainerCommandReport { json, success_text }
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

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
