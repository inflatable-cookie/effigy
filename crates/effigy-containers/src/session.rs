//! Container attached-session shaping extracted from
//! `src/runner/container_command.rs`.

use std::io::IsTerminal;
use std::path::Path;

use crate::{
    compose::{on_task_exit_label, shutdown_label},
    EffectiveContainerPolicy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachedSessionMode {
    Tui,
    Stream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerSessionProcessPlan {
    pub name: String,
    pub run: String,
    pub shutdown_on_exit: bool,
}

pub fn resolve_attached_session_mode() -> AttachedSessionMode {
    let stream_override = std::env::var("EFFIGY_CONTAINER_STREAM").ok();
    if stream_override
        .as_deref()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
    {
        return AttachedSessionMode::Stream;
    }

    let tui_override = std::env::var("EFFIGY_CONTAINER_TUI").ok();
    match tui_override.as_deref() {
        Some("1") => AttachedSessionMode::Tui,
        Some(value) if value.eq_ignore_ascii_case("true") => AttachedSessionMode::Tui,
        Some("0") => AttachedSessionMode::Stream,
        Some(value) if value.eq_ignore_ascii_case("false") => AttachedSessionMode::Stream,
        _ if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() => {
            AttachedSessionMode::Tui
        }
        _ => AttachedSessionMode::Stream,
    }
}

pub fn render_stream_session_overview(
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    health: Option<&'static str>,
    owner_task: Option<&str>,
) -> String {
    let mut lines = vec![
        format!("[container] {}", policy.name),
        format!("driver: {}", driver_label(policy.driver)),
        format!("profile: {}", policy.profile),
        format!("compose_file: {}", policy.compose_file_display),
        format!("project_name: {}", policy.project_name),
        format!("primary_service: {}", policy.primary_service),
        format!("owner_task: {}", owner_task.unwrap_or("<direct-command>")),
        format!(
            "shutdown_on_exit: {}",
            on_task_exit_label(policy.on_task_exit)
        ),
        format!("shutdown_mode: {}", shutdown_label(policy.shutdown)),
        format!("colima_started: {}", yes_no(colima_started)),
    ];
    if let Some(health) = health {
        lines.push(format!("health: {health}"));
    }
    if !policy.declared_ports.is_empty() {
        lines.push(format!("ports: {}", policy.declared_ports.join(", ")));
    }
    if !policy.ui_tabs.is_empty() {
        lines.push(format!("tabs: {}", policy.ui_tabs.join(", ")));
    }
    lines.join("\n")
}

pub fn render_attached_session_closeout(
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    termination_reason: &str,
    shutdown_applied: bool,
) -> String {
    let mut lines = Vec::new();
    if colima_started {
        lines.push(format!("[ok] started Colima profile `{}`", policy.profile));
    }
    lines.push(format!(
        "[ok] attached container session for `{}` finished ({termination_reason})",
        policy.name
    ));
    if shutdown_applied {
        lines.push(format!(
            "[ok] applied `{}` shutdown policy for `{}`",
            shutdown_label(policy.shutdown),
            policy.name
        ));
    } else {
        lines.push(format!(
            "[info] leaving container `{}` running because `on_task_exit = \"leave-running\"`",
            policy.name
        ));
    }
    lines.push(format!(
        "[next] inspect state with `effigy container {} status`",
        policy.name
    ));
    lines.join("\n")
}

pub fn attached_session_tab_order(policy: &EffectiveContainerPolicy) -> Vec<String> {
    let mut tabs = vec!["overview".to_owned()];
    let mut seen = std::collections::BTreeSet::<String>::new();
    seen.insert("overview".to_owned());
    for tab in &policy.ui_tabs {
        if seen.insert(tab.clone()) {
            tabs.push(tab.clone());
        }
    }
    if seen.insert(policy.primary_service.clone()) {
        tabs.push(policy.primary_service.clone());
    }
    tabs
}

pub fn attached_session_process_plans(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    owner_task: Option<&str>,
    executable: &str,
) -> Vec<ContainerSessionProcessPlan> {
    let mut plans = Vec::<ContainerSessionProcessPlan>::new();
    let overview = attached_overview_command(repo_root, policy, owner_task, executable);
    plans.push(ContainerSessionProcessPlan {
        name: "overview".to_owned(),
        run: overview,
        shutdown_on_exit: false,
    });

    let mut added = std::collections::BTreeSet::<String>::new();
    added.insert("overview".to_owned());
    let mut service_tabs = policy
        .ui_tabs
        .iter()
        .filter(|tab| tab.as_str() != "overview")
        .cloned()
        .collect::<Vec<_>>();
    if service_tabs.is_empty() {
        service_tabs.push(policy.primary_service.clone());
    }
    if !service_tabs
        .iter()
        .any(|tab| tab == &policy.primary_service)
    {
        service_tabs.insert(0, policy.primary_service.clone());
    }

    for service in service_tabs {
        if !added.insert(service.clone()) {
            continue;
        }
        plans.push(ContainerSessionProcessPlan {
            name: service.clone(),
            run: attached_logs_command(repo_root, policy, &service, executable),
            shutdown_on_exit: service == policy.primary_service,
        });
    }
    plans
}

pub fn resolve_effigy_invocation_prefix() -> Result<String, std::io::Error> {
    if let Ok(explicit) = std::env::var("EFFIGY_EXECUTABLE") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(shell_quote(trimmed));
        }
    }

    let executable = std::env::current_exe()?;
    let is_test_harness = executable
        .parent()
        .and_then(|parent| parent.file_name())
        .is_some_and(|name| name == "deps");
    if is_test_harness {
        let manifest_path = shell_quote(&format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")));
        return Ok(format!(
            "cargo run --quiet --manifest-path {manifest_path} --bin effigy --"
        ));
    }
    Ok(shell_quote(&executable.display().to_string()))
}

fn attached_overview_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    owner_task: Option<&str>,
    executable: &str,
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    let session_label = shell_quote(&policy.name);
    let task_label = shell_quote(owner_task.unwrap_or("<direct-command>"));
    let owner_exit = shell_quote(on_task_exit_label(policy.on_task_exit));
    let shutdown = shell_quote(shutdown_label(policy.shutdown));
    format!(
        "sh -lc {script}",
        script = shell_quote(&format!(
            "while true; do printf '\\033[2J\\033[H'; printf 'Container Session\\n\\n'; printf 'container: %s\\n' {session_label}; printf 'owner_task: %s\\n' {task_label}; printf 'primary_service: %s\\n' {primary}; printf 'shutdown_on_exit: %s\\n' {owner_exit}; printf 'shutdown_mode: %s\\n\\n' {shutdown}; {executable} container {name} status --repo {repo}; sleep 2; done",
            primary = shell_quote(&policy.primary_service),
            name = shell_quote(&policy.name),
        )),
    )
}

fn attached_logs_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    executable: &str,
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    format!(
        "{executable} container {name} logs --follow --service {service} --repo {repo}",
        name = shell_quote(&policy.name),
        service = shell_quote(service),
    )
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }
    if value
        .bytes()
        .all(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'/' | b':' | b'.' | b'_' | b'-'))
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn driver_label(driver: effigy_manifest::ManifestContainerDriver) -> &'static str {
    match driver {
        effigy_manifest::ManifestContainerDriver::Colima => "colima",
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
