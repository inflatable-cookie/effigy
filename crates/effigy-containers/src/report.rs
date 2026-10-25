//! Report shaping for `effigy container` subcommands.
//!
//! Each subcommand shapes a JSON payload and a human-readable success
//! string from the same inputs. The runner keeps the IO (docker/colima
//! subprocess calls, signal handling) and delegates the presentation
//! contract to this module.

use effigy_catalog::volumes::VolumeClassification;
use serde::Serialize;
use serde_json::{json, Value as JsonValue};
use std::collections::BTreeMap;

use crate::compose::shutdown_label;
use crate::{driver_label, EffectiveContainerPolicy};
use crate::{WorkspaceExtraMountDecision, WorkspaceExtraMountDecisionKind};

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

/// Add bounded workspace extra-mount decisions to a successful Compose-up report.
/// The report records the Compose spec and dispatch result, not runtime mount
/// verification.
pub fn annotate_workspace_extra_mount_decisions(
    report: &mut ContainerCommandReport,
    decisions: &[WorkspaceExtraMountDecision],
) {
    if decisions.is_empty() {
        return;
    }
    const MAX_JSON_DECISIONS: usize = 64;
    let decision_rows = decisions
        .iter()
        .take(MAX_JSON_DECISIONS)
        .map(|decision| WorkspaceExtraMountDecision {
            source: bounded_workspace_mount_path(&decision.source),
            target: bounded_workspace_mount_path(&decision.target),
            decision: decision.decision,
        })
        .collect::<Vec<_>>();
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "workspace_extra_mounts".to_owned(),
            json!({
                "compose_up_dispatch": "succeeded",
                "runtime_mount_verification": "not_performed",
                "decisions": decision_rows,
                "omitted_decision_count": decisions.len().saturating_sub(MAX_JSON_DECISIONS),
            }),
        );
    }
    for line in workspace_extra_mount_decision_lines(decisions) {
        report.success_text.push('\n');
        report.success_text.push_str(&line);
    }
}

/// Render bounded human-readable decisions after Compose up succeeds.
pub fn workspace_extra_mount_decision_lines(
    decisions: &[WorkspaceExtraMountDecision],
) -> Vec<String> {
    const MAX_TEXT_DECISIONS: usize = 32;
    let mut lines = decisions
        .iter()
        .take(MAX_TEXT_DECISIONS)
        .map(|decision| {
            let action = match decision.decision {
                WorkspaceExtraMountDecisionKind::IncludedInComposeSpec => {
                    "included in compose spec"
                }
                WorkspaceExtraMountDecisionKind::SkippedSourceAbsent => "skipped; source absent",
            };
            format!(
                "[workspace mount] {} -> {}: {}; compose up succeeded; actual runtime mount state was not verified",
                bounded_workspace_mount_path(&decision.source),
                bounded_workspace_mount_path(&decision.target),
                action
            )
        })
        .collect::<Vec<_>>();
    if decisions.len() > MAX_TEXT_DECISIONS {
        lines.push(format!(
            "[workspace mount] {} additional optional mount decision(s) omitted from diagnostics",
            decisions.len() - MAX_TEXT_DECISIONS
        ));
    }
    lines
}

fn bounded_workspace_mount_path(value: &str) -> String {
    const MAX_PATH_CHARS: usize = 384;
    let mut rendered = String::new();
    let mut chars = value.chars();
    for character in chars.by_ref().take(MAX_PATH_CHARS) {
        rendered.push(if character.is_control() {
            '\u{fffd}'
        } else {
            character
        });
    }
    if chars.next().is_some() {
        rendered.push('…');
    }
    rendered
}

/// What a service's own health check last reported, for a service that is
/// not healthy. A status line says "unhealthy"; this says why (effigy#313).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerServiceHealth {
    /// `unhealthy` or `starting`.
    pub status: String,
    /// Consecutive failed checks.
    pub failing_streak: u64,
    /// The last check's own output, bounded to one line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_output: Option<String>,
}

impl ContainerServiceHealth {
    /// `unhealthy after 12 failed checks; its last check said: `GET / 404``
    pub fn text(&self) -> String {
        let mut text = format!(
            "{} after {} failed check{}",
            self.status,
            self.failing_streak,
            if self.failing_streak == 1 { "" } else { "s" }
        );
        if let Some(output) = self.last_output.as_deref() {
            text.push_str(&format!("; its last check said: `{output}`"));
        }
        text
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerStatusService {
    pub name: String,
    pub container_name: String,
    pub status: String,
    pub ports: Vec<String>,
    /// Present only for a service whose own check is not healthy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<ContainerServiceHealth>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerStatusAllEntry {
    pub repo_root: String,
    pub container: String,
    pub project_name: String,
    pub profile: String,
    pub primary_service: String,
    pub dns_domain: Option<String>,
    pub dns_tls: bool,
    pub declared_ports: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerVolumeGlobalEntry {
    pub name: String,
    pub backend: String,
    pub profile: String,
    pub project_name: Option<String>,
    pub repo_root: Option<String>,
    pub service: Option<String>,
    pub mount_target: Option<String>,
    pub persist: Option<bool>,
    pub size_bytes: Option<u64>,
    pub in_use: bool,
    pub orphaned: bool,
    pub orphan_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerVolumePruneEntry {
    pub name: String,
    pub backend: String,
    pub profile: String,
    pub project_name: Option<String>,
    pub size_bytes: Option<u64>,
    pub removed: bool,
    pub in_use: bool,
    pub orphan_reason: Option<String>,
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

/// Build the `container down` report for an environment this checkout never
/// prepared whose backend project owns no containers, networks or volumes
/// (effigy#286). Nothing was running, so the stop renders nothing and calls no
/// Compose: a never-prepared render deliberately fails closed on its
/// unassigned ports. `colima_running` is true because the backend answered the
/// emptiness probe.
pub fn down_noop_report(policy: &EffectiveContainerPolicy, reason: &str) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.down.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "profile": policy.profile,
        "shared_services": shared_services_json(policy),
        "colima_running": true,
        "shutdown": shutdown_label(policy.shutdown),
        "noop": reason,
    });
    ContainerCommandReport {
        json,
        success_text: format!("[ok] nothing to stop for `{}`: {reason}", policy.name),
    }
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
    backend: &str,
    runtime_running: bool,
    health: Option<&'static str>,
    primary_service_exec_ready: Option<bool>,
    services: Option<&[ContainerStatusService]>,
    compose_ps: Option<&str>,
) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.status.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "driver": "colima",
        "manifest_driver": driver_label(policy.driver),
        "backend": backend,
        "profile": policy.profile,
        "compose_file": policy.compose_file_display,
        "project_name": policy.project_name,
        "primary_service": policy.primary_service,
        "shared_services": shared_services_json(policy),
        "runtime_running": runtime_running,
        "health": health,
        "primary_service_exec_ready": primary_service_exec_ready,
        "ports": policy.declared_ports,
        "mounts": policy.declared_mounts,
        "detach_timeout_secs": policy.detach_timeout_secs,
        "services": services,
        "compose_ps": compose_ps,
        "media_mounts": policy.declared_media_mounts,
    });

    let mut lines = vec![
        format!("[container] {}", policy.name),
        format!("manifest_driver: {}", driver_label(policy.driver)),
        format!("backend: {backend}"),
        format!("profile: {}", policy.profile),
        format!("compose_file: {}", policy.compose_file_display),
        format!("project_name: {}", policy.project_name),
        format!("primary_service: {}", policy.primary_service),
        format!("runtime_running: {}", yes_no(runtime_running)),
    ];
    if let Some(primary_service_exec_ready) = primary_service_exec_ready {
        lines.push(format!(
            "primary_service_exec_ready: {}",
            yes_no(primary_service_exec_ready)
        ));
    }
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
    if let Some(services) = services {
        lines.push(String::new());
        lines.push("services:".to_owned());
        if services.is_empty() {
            lines.push("(none)".to_owned());
        } else {
            for service in services {
                let ports = if service.ports.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", service.ports.join(", "))
                };
                lines.push(format!("- {}: {}{}", service.name, service.status, ports));
                if let Some(health) = service.health.as_ref() {
                    lines.push(format!("  health: {}", health.text()));
                }
            }
        }
    } else if let Some(compose_ps) = compose_ps {
        lines.push(String::new());
        lines.push("compose status:".to_owned());
        lines.push(compose_ps.trim().to_owned());
    }
    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

/// Build the `container status --global` report.
pub fn status_global_report(entries: &[ContainerStatusAllEntry]) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.status-global.v1",
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

/// Build the `container stats --global` report.
pub fn stats_global_report(
    entries: &[ContainerStatsAllEntry],
    stats_warning: Option<&str>,
) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.stats-global.v1",
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
    let size_totals = cache_size_totals(volumes.iter().map(|volume| (false, volume.size_bytes)));
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
        "reclaimable_size_bytes": size_totals.reclaimable_size_bytes,
        "unknown_size_count": size_totals.unknown_size_count,
        "size_complete": size_totals.size_complete,
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
            "[ok] {} purge-safe cache volume{} for `{}` (reclaimable={})",
            volumes.len(),
            if volumes.len() == 1 { "" } else { "s" },
            policy.name,
            format_size_total(
                size_totals.reclaimable_size_bytes,
                size_totals.unknown_size_count
            ),
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

pub fn cache_list_global_report(
    profile: &str,
    scope_label: &str,
    volumes: &[ContainerCacheGlobalEntry],
) -> ContainerCommandReport {
    let size_totals = cache_size_totals(
        volumes
            .iter()
            .map(|volume| (volume.in_use, volume.size_bytes)),
    );
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
        "schema": "effigy.container.cache-list-global.v1",
        "schema_version": 1,
        "ok": true,
        "profile": profile,
        "scope": scope_label,
        "cache_count": volumes.len(),
        "in_use_count": in_use_count,
        "available_count": volumes.len().saturating_sub(in_use_count),
        "reclaimable_size_bytes": size_totals.reclaimable_size_bytes,
        "in_use_size_bytes": size_totals.in_use_size_bytes,
        "unknown_size_count": size_totals.unknown_size_count,
        "size_complete": size_totals.size_complete,
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
            success_text: format!("[info] no purge-safe cache volumes found in {scope_label}"),
        };
    }

    let mut summary = format!(
        "[ok] {} purge-safe cache volume{} in {} (in_use={}, purgeable={}, reclaimable={}, in_use_size={})",
        volumes.len(),
        if volumes.len() == 1 { "" } else { "s" },
        scope_label,
        in_use_count,
        volumes.len().saturating_sub(in_use_count),
        format_bytes(size_totals.reclaimable_size_bytes),
        format_bytes(size_totals.in_use_size_bytes),
    );
    if size_totals.unknown_size_count > 0 {
        summary.push_str(&format!(
            "; {} size{} unavailable",
            size_totals.unknown_size_count,
            if size_totals.unknown_size_count == 1 {
                ""
            } else {
                "s"
            }
        ));
    }
    let mut lines = vec![summary];
    for (index, (project, caches)) in grouped.into_iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
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

pub fn volume_list_report(
    scope_label: &str,
    filter_label: Option<&str>,
    volumes: &[ContainerVolumeGlobalEntry],
) -> ContainerCommandReport {
    let matched_count = volumes.iter().filter(|volume| volume.orphaned).count();
    let in_use_count = volumes.iter().filter(|volume| volume.in_use).count();
    let mut json = json!({
        "schema": "effigy.container.volume-list.v1",
        "schema_version": 1,
        "ok": true,
        "scope": scope_label,
        "filter": filter_label,
        "volume_count": volumes.len(),
        "in_use_count": in_use_count,
        "inactive_count": volumes.len().saturating_sub(in_use_count),
        "volumes": volumes.iter().map(|volume| {
            json!({
                "name": volume.name,
                "backend": volume.backend,
                "profile": volume.profile,
                "project_name": volume.project_name,
                "repo_root": volume.repo_root,
                "service": volume.service,
                "mount_target": volume.mount_target,
                "persist": volume.persist,
                "classification": match volume.persist {
                    Some(true) => Some("persistent"),
                    Some(false) => Some("ephemeral"),
                    None => None,
                },
                "size_bytes": volume.size_bytes,
                "size_available": volume.size_bytes.is_some(),
                "in_use": volume.in_use,
                "orphaned": volume.orphaned,
                "orphan_reason": volume.orphan_reason,
            })
        }).collect::<Vec<_>>(),
    });
    match filter_label {
        Some("dormant") => {
            json["dormant_count"] = json!(matched_count);
        }
        _ => {
            json["orphan_count"] = json!(matched_count);
        }
    }

    if volumes.is_empty() {
        return ContainerCommandReport {
            json,
            success_text: match filter_label {
                Some("orphans") => {
                    format!("[info] no orphaned Effigy-managed volumes found in {scope_label}")
                }
                Some("dormant") => {
                    format!("[info] no dormant Effigy-managed volumes found in {scope_label}")
                }
                _ => {
                    format!("[info] no Effigy-managed volumes found in {scope_label}")
                }
            },
        };
    }

    let mut grouped = BTreeMap::<String, Vec<&ContainerVolumeGlobalEntry>>::new();
    for volume in volumes {
        let heading = volume.repo_root.clone().unwrap_or_else(|| {
            volume
                .project_name
                .clone()
                .unwrap_or_else(|| "unknown-owner".to_owned())
        });
        grouped.entry(heading).or_default().push(volume);
    }

    let mut lines = vec![format!(
        "[ok] {} Effigy-managed volume{} in {} (in_use={}, inactive={}, {}={})",
        volumes.len(),
        if volumes.len() == 1 { "" } else { "s" },
        scope_label,
        in_use_count,
        volumes.len().saturating_sub(in_use_count),
        match filter_label {
            Some("dormant") => "dormant",
            _ => "orphans",
        },
        matched_count
    )];
    for (index, (heading, entries)) in grouped.into_iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        lines.push(format!("{heading}:"));
        for volume in entries {
            let size = volume
                .size_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "unavailable".to_owned());
            let project = volume.project_name.as_deref().unwrap_or("unknown");
            let service = volume.service.as_deref().unwrap_or("unknown");
            let target = volume.mount_target.as_deref().unwrap_or("unavailable");
            let classification = match volume.persist {
                Some(true) => "persistent",
                Some(false) => "ephemeral",
                None => "unknown",
            };
            let ownership = if volume.orphaned {
                format!(
                    "orphaned:{}",
                    volume.orphan_reason.as_deref().unwrap_or("unknown")
                )
            } else if volume.in_use {
                "in-use".to_owned()
            } else {
                "inactive".to_owned()
            };
            lines.push(format!(
                "- {} [{}:{}] {} {} (size={}, {})",
                volume.name, project, service, volume.backend, classification, size, ownership
            ));
            lines.push(format!("  target={target}, profile={}", volume.profile));
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
    let removed_size_bytes: u64 = entries
        .iter()
        .filter(|entry| entry.removed)
        .filter_map(|entry| entry.size_bytes)
        .sum();
    let skipped_size_bytes: u64 = entries
        .iter()
        .filter(|entry| !entry.removed)
        .filter_map(|entry| entry.size_bytes)
        .sum();
    let json = json!({
        "schema": "effigy.container.cache-prune.v1",
        "schema_version": 1,
        "ok": true,
        "scope": scope_label,
        "removed_count": removed_count,
        "removed_size_bytes": removed_size_bytes,
        "skipped_count": skipped_count,
        "skipped_size_bytes": skipped_size_bytes,
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
        "[ok] removed {} cache volume{} ({}) and skipped {} ({}) for {}",
        removed_count,
        if removed_count == 1 { "" } else { "s" },
        format_bytes(removed_size_bytes),
        skipped_count,
        format_bytes(skipped_size_bytes),
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

pub fn volume_prune_report(
    scope_label: &str,
    filter_label: &str,
    entries: &[ContainerVolumePruneEntry],
) -> ContainerCommandReport {
    let removed_count = entries.iter().filter(|entry| entry.removed).count();
    let skipped_count = entries.len().saturating_sub(removed_count);
    let removed_size_bytes: u64 = entries
        .iter()
        .filter(|entry| entry.removed)
        .filter_map(|entry| entry.size_bytes)
        .sum();
    let skipped_size_bytes: u64 = entries
        .iter()
        .filter(|entry| !entry.removed)
        .filter_map(|entry| entry.size_bytes)
        .sum();
    let json = json!({
        "schema": "effigy.container.volume-prune.v1",
        "schema_version": 1,
        "ok": true,
        "scope": scope_label,
        "filter": filter_label,
        "removed_count": removed_count,
        "removed_size_bytes": removed_size_bytes,
        "skipped_count": skipped_count,
        "skipped_size_bytes": skipped_size_bytes,
        "volumes": entries.iter().map(|entry| {
            json!({
                "name": entry.name,
                "backend": entry.backend,
                "profile": entry.profile,
                "project_name": entry.project_name,
                "size_bytes": entry.size_bytes,
                "removed": entry.removed,
                "in_use": entry.in_use,
                "orphan_reason": entry.orphan_reason,
            })
        }).collect::<Vec<_>>(),
    });

    if entries.is_empty() {
        return ContainerCommandReport {
            json,
            success_text: format!(
                "[info] no {filter_label} Effigy-managed volumes matched {scope_label}"
            ),
        };
    }

    let mut lines = vec![format!(
        "[ok] removed {} volume{} ({}) and skipped {} ({}) for {} ({})",
        removed_count,
        if removed_count == 1 { "" } else { "s" },
        format_bytes(removed_size_bytes),
        skipped_count,
        format_bytes(skipped_size_bytes),
        scope_label,
        filter_label
    )];
    for entry in entries {
        let size = entry
            .size_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "unavailable".to_owned());
        let project = entry.project_name.as_deref().unwrap_or("unknown");
        let status = if entry.removed {
            "removed".to_owned()
        } else if entry.in_use {
            "skipped: in-use".to_owned()
        } else {
            format!(
                "skipped:{}",
                entry.orphan_reason.as_deref().unwrap_or("unmatched")
            )
        };
        lines.push(format!(
            "- {} (size={}, backend={}, profile={}, project={}, {})",
            entry.name, size, entry.backend, entry.profile, project, status
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

pub fn hosts_report(
    checkout: &std::path::Path,
    host_map: &crate::EffectiveHostMap,
) -> ContainerCommandReport {
    let kind = match host_map.kind {
        crate::HostScopeKind::Primary => "primary",
        crate::HostScopeKind::Worktree => "worktree",
        crate::HostScopeKind::EphemeralClone => "ephemeral-clone",
    };
    let routes = host_map
        .routes
        .iter()
        .map(|route| {
            json!({
                "kind": match route.kind {
                    crate::HostRouteKind::Http => "http",
                    crate::HostRouteKind::Tcp => "tcp",
                },
                "declared": route.declared,
                "effective": route.effective,
                "tls": route.tls,
                "service": route.service,
                "origin": route.origin,
            })
        })
        .collect::<Vec<_>>();
    let json = json!({
        "schema": "effigy.container.hosts.v1",
        "schema_version": 1,
        "ok": true,
        "checkout": checkout.display().to_string(),
        "scope": {
            "kind": kind,
            "token": host_map.token,
            "host_key": host_map.host_key,
            "shared_runtime_identity": host_map.shared_runtime_identity,
        },
        "base_domain": {
            "declared": host_map.declared_base,
            "effective": host_map.effective_base,
            "inferred": host_map.base_domain_inferred,
            "warning": host_map.base_domain_warning,
        },
        "cookie_domain": host_map.cookie_domain(),
        "relying_party_id": host_map.relying_party_id(),
        "routes": routes,
    });
    let mut lines = Vec::new();
    match host_map.effective_base.as_deref() {
        Some(base) if host_map.kind != crate::HostScopeKind::Primary => {
            let label = match host_map.kind {
                crate::HostScopeKind::EphemeralClone => "ephemeral clone",
                _ => "worktree",
            };
            lines.push(format!(
                "[ok] effective hosts for {label} scope {}",
                host_map.host_key.as_deref().unwrap_or("unknown")
            ));
            if let Some(declared) = host_map.declared_base.as_deref() {
                lines.push(format!("base: {declared} -> {base}"));
            }
        }
        Some(base) => {
            lines.push(format!(
                "[ok] effective hosts for primary checkout `{base}`"
            ));
        }
        None => lines.push("[ok] no declared gateway hosts".to_owned()),
    }
    if host_map.base_domain_inferred {
        lines.push(
            "base_domain_inferred: true (declare `[containers.<name>.dns] base_domain` in the bundle)"
                .to_owned(),
        );
    }
    if let Some(warning) = host_map.base_domain_warning.as_deref() {
        lines.push(format!("[warn] {warning}"));
    }
    for route in &host_map.routes {
        let kind = match route.kind {
            crate::HostRouteKind::Http => {
                if route.tls {
                    "https"
                } else {
                    "http"
                }
            }
            crate::HostRouteKind::Tcp => "tcp",
        };
        let origin = route.origin.as_deref().unwrap_or(route.effective.as_str());
        lines.push(format!("{kind} {origin} (declared {})", route.declared));
    }
    if let Some(cookie) = host_map.cookie_domain() {
        lines.push(format!("cookie_domain: {cookie}"));
        lines.push(format!("relying_party_id: {cookie}"));
    }
    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

pub fn retire_report(
    checkout: Option<&str>,
    outcome: &crate::RetireOutcome,
) -> ContainerCommandReport {
    let ok = outcome.ok();
    let json = json!({
        "schema": "effigy.container.retire.v1",
        "schema_version": 2,
        "ok": ok,
        "idempotent": outcome.idempotent(),
        "checkout": checkout,
        "removed": outcome.removed.iter().map(resource_json).collect::<Vec<_>>(),
        "kept": outcome
            .kept
            .iter()
            .map(|kept| {
                let mut value = resource_json(&kept.resource);
                value["reason"] = json!(kept.reason);
                value
            })
            .collect::<Vec<_>>(),
        "remaining": outcome
            .remaining()
            .iter()
            .map(|failed| {
                let mut value = resource_json(&failed.resource);
                value["error"] = json!(failed.error);
                value
            })
            .collect::<Vec<_>>(),
        "gateway": {
            "routes": outcome.gateway.routes,
            "loopbacks": outcome.gateway.loopbacks,
        },
    });
    let mut lines = Vec::new();
    if let Some(checkout) = checkout {
        lines.push(format!("checkout: {checkout}"));
    }
    if ok && outcome.removed.is_empty() {
        lines.push("[ok] no owned runtime resource is left".to_owned());
    }
    for removed in &outcome.removed {
        lines.push(format!(
            "removed {} {} ({})",
            removed.kind.label(),
            removed.name,
            removed.profile
        ));
    }
    for kept in &outcome.kept {
        lines.push(format!(
            "kept {} {}: {}",
            kept.resource.kind.label(),
            kept.resource.name,
            kept.reason
        ));
    }
    if !outcome.gateway.routes.is_empty() {
        lines.push(format!(
            "released {} route(s): {}",
            outcome.gateway.routes.len(),
            outcome.gateway.routes.join(", ")
        ));
    }
    if outcome.gateway.loopbacks > 0 {
        lines.push(format!(
            "released {} loopback address(es)",
            outcome.gateway.loopbacks
        ));
    }
    for failed in outcome.remaining() {
        lines.push(format!(
            "[error] {} {} is still there: {}",
            failed.resource.kind.label(),
            failed.resource.name,
            failed.error
        ));
    }
    if ok && !outcome.removed.is_empty() {
        lines.push(format!(
            "[ok] retired {} owned runtime resource(s)",
            outcome.removed.len()
        ));
    }
    ContainerCommandReport {
        json,
        success_text: lines.join("\n"),
    }
}

fn resource_json(resource: &crate::OwnedResource) -> JsonValue {
    json!({
        "kind": resource.kind.label(),
        "name": resource.name,
        "profile": resource.profile,
        "persist": resource.persist,
    })
}

struct CacheSizeTotals {
    reclaimable_size_bytes: u64,
    in_use_size_bytes: u64,
    unknown_size_count: usize,
    size_complete: bool,
}

fn cache_size_totals<I>(entries: I) -> CacheSizeTotals
where
    I: IntoIterator<Item = (bool, Option<u64>)>,
{
    let mut reclaimable_size_bytes = 0;
    let mut in_use_size_bytes = 0;
    let mut unknown_size_count = 0;
    for (in_use, size_bytes) in entries {
        match size_bytes {
            Some(bytes) if in_use => in_use_size_bytes += bytes,
            Some(bytes) => reclaimable_size_bytes += bytes,
            None => unknown_size_count += 1,
        }
    }
    CacheSizeTotals {
        reclaimable_size_bytes,
        in_use_size_bytes,
        unknown_size_count,
        size_complete: unknown_size_count == 0,
    }
}

fn format_size_total(bytes: u64, unknown_size_count: usize) -> String {
    if unknown_size_count == 0 {
        format_bytes(bytes)
    } else {
        format!(
            "{} ({} size{} unavailable)",
            format_bytes(bytes),
            unknown_size_count,
            if unknown_size_count == 1 { "" } else { "s" }
        )
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

#[cfg(test)]
mod retire_tests {
    use super::retire_report;
    use crate::{FailedResource, KeptResource, OwnedResource, ResourceKind, RetireOutcome};

    fn resource(kind: ResourceKind, name: &str) -> OwnedResource {
        OwnedResource {
            kind,
            name: name.to_owned(),
            profile: "effigy".to_owned(),
            persist: false,
        }
    }

    #[test]
    fn a_clean_report_is_ok_and_idempotent() {
        let report = retire_report(Some("/w/one"), &RetireOutcome::default());
        assert_eq!(report.json["ok"], true);
        assert_eq!(report.json["idempotent"], true);
        assert_eq!(report.json["checkout"], "/w/one");
        assert!(report.success_text.contains("no owned runtime resource"));
    }

    #[test]
    fn removed_kept_and_remaining_are_named() {
        let mut outcome = RetireOutcome::default();
        outcome
            .removed
            .push(resource(ResourceKind::Container, "web-1"));
        outcome.kept.push(KeptResource {
            resource: resource(ResourceKind::Volume, "db-data"),
            reason: "named data volume",
        });
        outcome.failed.push(FailedResource {
            resource: resource(ResourceKind::Volume, "stuck"),
            error: "volume is in use".to_owned(),
        });
        let report = retire_report(Some("/w/one"), &outcome);
        assert_eq!(report.json["ok"], false, "a leftover is never clean");
        assert_eq!(report.json["removed"][0]["name"], "web-1");
        assert_eq!(report.json["kept"][0]["name"], "db-data");
        assert_eq!(report.json["kept"][0]["reason"], "named data volume");
        assert_eq!(report.json["remaining"][0]["name"], "stuck");
        assert_eq!(report.json["remaining"][0]["error"], "volume is in use");
        assert!(report.success_text.contains("removed container web-1"));
        assert!(report.success_text.contains("kept volume db-data"));
        assert!(report.success_text.contains("volume stuck is still there"));
    }

    #[test]
    fn gateway_release_is_reported() {
        let mut outcome = RetireOutcome::default();
        outcome.gateway.routes = vec!["app.test".to_owned()];
        outcome.gateway.loopbacks = 2;
        let report = retire_report(Some("/w/one"), &outcome);
        assert_eq!(report.json["ok"], true);
        assert_eq!(report.json["gateway"]["routes"][0], "app.test");
        assert_eq!(report.json["gateway"]["loopbacks"], 2);
        assert!(report.success_text.contains("released 1 route(s)"));
        assert!(report
            .success_text
            .contains("released 2 loopback address(es)"));
    }
}

#[cfg(test)]
mod hosts_report_tests {
    use super::hosts_report;
    use crate::{build_host_map, EffectiveDnsRoute};

    fn route(domain: &str) -> EffectiveDnsRoute {
        EffectiveDnsRoute {
            domain: domain.to_owned(),
            declared_domain: domain.to_owned(),
            tls: true,
            port: None,
            service: None,
            target_host: None,
        }
    }

    #[test]
    fn an_inferred_base_is_reported_with_its_warning() {
        let map = build_host_map(
            &[route("s3.acme.test"), route("other.example")],
            &[],
            &[],
            None,
            false,
            None,
        );
        let report = hosts_report(std::path::Path::new("/repo"), &map);
        assert_eq!(report.json["base_domain"]["inferred"], true);
        let warning = report.json["base_domain"]["warning"]
            .as_str()
            .expect("the warning is reported");
        assert!(warning.contains("other.example"), "{warning}");
        assert!(report.success_text.contains("base_domain_inferred: true"));
        assert!(
            report.success_text.contains("[warn]"),
            "{}",
            report.success_text
        );
    }

    #[test]
    fn a_declared_base_is_not_reported_as_inferred() {
        let map = build_host_map(
            &[route("s3.acme.test")],
            &[],
            &[],
            None,
            false,
            Some("acme.test"),
        );
        let report = hosts_report(std::path::Path::new("/repo"), &map);
        assert_eq!(report.json["base_domain"]["inferred"], false);
        assert!(report.json["base_domain"]["warning"].is_null());
        assert!(!report.success_text.contains("base_domain_inferred"));
    }
}
