//! Report shaping for `effigy container` subcommands.
//!
//! Each subcommand shapes a JSON payload and a human-readable success
//! string from the same inputs. The runner keeps the IO (docker/colima
//! subprocess calls, signal handling) and delegates the presentation
//! contract to this module.

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
) -> ContainerCommandReport {
    let json = json!({
        "schema": "effigy.container.reset.v1",
        "schema_version": 1,
        "ok": true,
        "container": policy.name,
        "profile": policy.profile,
        "colima_running": colima_running,
    });
    let success_text = if colima_running {
        format!(
            "[ok] reset container environment `{}` and removed compose-managed volumes",
            policy.name
        )
    } else {
        format!(
            "[ok] skipped reset for `{}` because Colima profile `{}` is not running",
            policy.name, policy.profile
        )
    };
    ContainerCommandReport { json, success_text }
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
