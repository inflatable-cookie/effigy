//! Container attached-session shaping extracted from
//! `src/runner/container_command.rs`.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

const CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH: &str = "/usr/local/bin/effigy";

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
    for service in [policy.primary_service.clone()] {
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

pub struct ManagedLifecycleRequest<'a> {
    pub repo_root: &'a Path,
    pub container_name: Option<&'a str>,
    pub owner_task: &'a str,
    pub health_wait: bool,
    pub health_wait_timeout_secs: u64,
    pub ready_message: Option<&'a str>,
    pub dns_route_lines: &'a [String],
    pub readiness_probe_urls: &'a [String],
    pub setup_commands: &'a [String],
    pub executable: &'a str,
    pub secrets_required: bool,
}

pub fn managed_lifecycle_command(request: ManagedLifecycleRequest<'_>) -> String {
    let ManagedLifecycleRequest {
        repo_root,
        container_name,
        owner_task,
        health_wait,
        health_wait_timeout_secs,
        ready_message,
        dns_route_lines,
        readiness_probe_urls,
        setup_commands,
        executable,
        secrets_required,
    } = request;
    let repo = shell_quote(&repo_root.display().to_string());
    let lifecycle_owner_task = owner_task;
    let owner_task = shell_quote(owner_task);
    let selector = container_name
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default");
    let lifecycle_state = managed_lifecycle_state_path(
        repo_root,
        selector.unwrap_or("default"),
        lifecycle_owner_task,
    );
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let up = effigy_container_command(executable, selector, "up --detach", &repo);
    let up = if secrets_required {
        format!("env EFFIGY_SECRETS_REQUIRED=1 {up}")
    } else {
        up
    };
    let status = effigy_container_command(executable, selector, "status", &repo);
    let down = effigy_container_command(executable, selector, "down", &repo);
    let label = selector.unwrap_or("default");
    let readiness_status = if health_wait {
        "waiting for readiness via detached container startup"
    } else {
        "startup does not declare managed readiness waiting"
    };
    let ready_banner = ready_message
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("container `{label}` is ready"));
    let dns_routes_section = managed_lifecycle_dns_routes_section(dns_route_lines);
    let readiness_wait = managed_lifecycle_readiness_wait(
        health_wait,
        readiness_probe_urls,
        health_wait_timeout_secs,
        "managed lifecycle readiness wait timed out",
    );
    let setup_sequence = managed_lifecycle_setup_sequence(setup_commands);
    let idle_wait = managed_lifecycle_idle_wait_command();
    format!(
        "sh -lc {script}",
        script = shell_quote(&format!(
            "state_path={lifecycle_state}; parent_pid=$PPID; mkdir -p \"$(dirname \"$state_path\")\"; printf '%s\\n' starting > \"$state_path\"; started=0; cleanup() {{ if [ \"$started\" = 1 ]; then printf '%s\\n' stopped > \"$state_path\"; {down} >/dev/null 2>&1 || true; else printf '%s\\n' failed > \"$state_path\"; fi; }}; trap 'cleanup' EXIT; trap 'exit 0' INT TERM; printf 'managed lifecycle: %s\\n' {readiness_status}; if ! {up}; then printf '%s\\n' 'managed lifecycle failed during container startup' 1>&2; exit 1; fi; started=1; printf '%s\\n' started > \"$state_path\"; {setup_sequence}{readiness_wait}printf '%s\\n' ready > \"$state_path\"; printf 'managed ready: %s\\n' {ready_banner}; printf 'Managed Container Lifecycle\\n\\n'; printf 'container: %s\\n' {label}; printf 'owner_task: %s\\n' {owner_task}; printf 'readiness: %s\\n' {readiness_status}; {dns_routes_section}printf 'ready_message: %s\\n\\n' {ready_banner}; {status} || true; printf '\\n[info] lifecycle owner is idle; use `effigy container {label} status` to refresh.\\n'; {idle_wait}",
            label = shell_quote(label),
            readiness_status = shell_quote(readiness_status),
            ready_banner = shell_quote(&ready_banner),
            dns_routes_section = dns_routes_section,
            readiness_wait = readiness_wait,
            setup_sequence = setup_sequence,
            idle_wait = idle_wait,
        )),
    )
}

pub fn managed_lifecycle_dns_routes_section(dns_route_lines: &[String]) -> String {
    if dns_route_lines.is_empty() {
        return "printf 'dns_routes: none\\n\\n'; ".to_owned();
    }
    let mut section = "printf 'dns_routes:\\n'; ".to_owned();
    for line in dns_route_lines {
        section.push_str(&format!("printf '  - %s\\n' {}; ", shell_quote(line)));
    }
    section.push_str("printf '\\n'; ");
    section
}

pub fn managed_lifecycle_readiness_wait(
    health_wait: bool,
    readiness_probe_urls: &[String],
    timeout_secs: u64,
    timeout_message: &str,
) -> String {
    if !health_wait || readiness_probe_urls.is_empty() {
        return String::new();
    }
    let probe_urls = readiness_probe_urls
        .iter()
        .map(|url| shell_quote(url))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "readiness_deadline=$(( $(date +%s) + {timeout_secs} )); while true; do readiness_ok=1; for readiness_url in {probe_urls}; do readiness_code=$(curl -k -s -o /dev/null -w '%{{http_code}}' \"$readiness_url\" || true); case \"$readiness_code\" in 000|502|503|504) readiness_ok=0; break ;; esac; done; if [ \"$readiness_ok\" = 1 ]; then break; fi; if [ \"$(date +%s)\" -ge \"$readiness_deadline\" ]; then printf '%s\\n' {timeout_message} 1>&2; exit 1; fi; sleep 1; done; ",
        probe_urls = probe_urls,
        timeout_secs = timeout_secs,
        timeout_message = shell_quote(timeout_message),
    )
}

pub fn managed_lifecycle_shutdown_command(
    repo_root: &Path,
    container_name: Option<&str>,
    executable: &str,
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    let selector = container_name
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default");
    effigy_container_command(executable, selector, "down", &repo)
}

pub fn managed_shell_command(
    repo_root: &Path,
    container_name: Option<&str>,
    owner_task: &str,
    service: Option<&str>,
    executable: &str,
    exec_readiness_timeout_secs: u64,
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    let selector = container_name
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default");
    let lifecycle_state =
        managed_lifecycle_state_path(repo_root, selector.unwrap_or("default"), owner_task);
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let service_flag = service
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!(" --service {}", shell_quote(value)))
        .unwrap_or_default();
    let readiness_probe = format!(
        "{} >/dev/null 2>&1",
        effigy_container_command_with_extra(
            executable,
            selector,
            &format!("shell{service_flag} --command true"),
            &repo,
        )
    );
    let attach = effigy_container_command_with_extra(
        executable,
        selector,
        &format!("shell{service_flag}"),
        &repo,
    );
    format!(
        "sh -lc {script}",
        script = shell_quote(&format!(
            "state_path={lifecycle_state}; deadline=$(( $(date +%s) + {timeout_secs} )); while true; do if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = starting ]; then sleep 1; continue; fi; if {readiness_probe}; then exec {attach}; fi; if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = failed ]; then printf '%s\\n' 'managed lifecycle failed before shell became available' 1>&2; exit 1; fi; if [ \"$(date +%s)\" -ge \"$deadline\" ]; then printf '%s\\n' 'managed shell timed out waiting for container exec readiness' 1>&2; exit 1; fi; sleep 1; done",
            timeout_secs = exec_readiness_timeout_secs,
        )),
    )
}

pub fn managed_lifecycle_idle_wait_command() -> &'static str {
    "while kill -0 \"$parent_pid\" >/dev/null 2>&1; do sleep 1; done"
}

pub struct ManagedStandardExecRequest<'a> {
    pub repo_root: &'a Path,
    pub container_name: Option<&'a str>,
    pub owner_task: &'a str,
    pub process_cwd: &'a Path,
    pub container_repo_root: Option<&'a Path>,
    pub setup_command: Option<&'a str>,
    pub executable: &'a str,
    pub command: &'a str,
    /// Outer budget for the exec-readiness wait. The caller passes the
    /// managed plan's readiness timeout so concurrent children share the
    /// lifecycle's own startup budget instead of a fixed guess.
    pub exec_readiness_timeout_secs: u64,
}

pub fn managed_standard_exec_command(request: ManagedStandardExecRequest<'_>) -> String {
    let ManagedStandardExecRequest {
        repo_root,
        container_name,
        owner_task,
        process_cwd,
        container_repo_root,
        setup_command,
        executable,
        command,
        exec_readiness_timeout_secs,
    } = request;
    let repo = shell_quote(&repo_root.display().to_string());
    let cwd = shell_quote(&process_cwd.display().to_string());
    let routed_command =
        container_exec_command(command, repo_root, process_cwd, container_repo_root);
    let command = shell_quote(&routed_command);
    let selector = container_name
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default");
    let lifecycle_state =
        managed_lifecycle_state_path(repo_root, selector.unwrap_or("default"), owner_task);
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let readiness_probe = format!(
        "{} >/dev/null 2>&1",
        effigy_container_command_with_extra(executable, selector, "shell --command true", &repo,)
    );
    let attach = effigy_container_command_with_extra(
        executable,
        selector,
        &format!("shell --command {command}"),
        &repo,
    );
    let setup_sequence = setup_command
        .map(|setup_command| {
            let routed_setup = shell_quote(&container_exec_command(
                setup_command,
                repo_root,
                process_cwd,
                container_repo_root,
            ));
            format!(
                "{} && ",
                effigy_container_command_with_extra(
                    executable,
                    selector,
                    &format!("shell --command {routed_setup}"),
                    &repo,
                )
            )
        })
        .unwrap_or_default();
    format!(
        "sh -lc {script}",
        script = shell_quote(&format!(
            "cd {cwd} && state_path={lifecycle_state}; deadline=$(( $(date +%s) + {timeout_secs} )); while true; do if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = starting ]; then sleep 1; continue; fi; if {readiness_probe}; then {setup_sequence}exec {attach}; fi; if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = failed ]; then printf '%s\\n' 'managed lifecycle failed before exec surface became available' 1>&2; exit 1; fi; if [ \"$(date +%s)\" -ge \"$deadline\" ]; then printf '%s\\n' 'managed exec timed out waiting for container exec readiness' 1>&2; exit 1; fi; sleep 1; done",
            timeout_secs = exec_readiness_timeout_secs,
            setup_sequence = setup_sequence,
        )),
    )
}

pub fn managed_lifecycle_state_path(
    repo_root: &Path,
    container_label: &str,
    owner_task: &str,
) -> PathBuf {
    let sanitized_task = sanitize_state_key(owner_task);
    let sanitized_container = sanitize_state_key(container_label);
    repo_root
        .join(".effigy/runtime/managed-lifecycle")
        .join(format!("{sanitized_task}-{sanitized_container}.state"))
}

fn sanitize_state_key(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn rewrite_command_for_container(
    command: &str,
    repo_root: &Path,
    container_repo_root: &Path,
) -> String {
    let rewritten = command.replace(
        &repo_root.display().to_string(),
        &container_repo_root.display().to_string(),
    );
    rewrite_effigy_invocation_for_container(&rewritten)
}

fn rewrite_effigy_invocation_for_container(command: &str) -> String {
    let Ok(host_invocation) = resolve_effigy_invocation_prefix() else {
        return command.to_owned();
    };
    if host_invocation == shell_quote(CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH) {
        return command.to_owned();
    }
    let mut rewritten = command.replace(
        &host_invocation,
        &shell_quote(CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH),
    );
    if let Some(raw_host_path) = host_invocation
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    {
        rewritten = rewritten.replace(raw_host_path, CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH);
    }
    rewritten
}

pub fn container_exec_command(
    command: &str,
    repo_root: &Path,
    process_cwd: &Path,
    container_repo_root: Option<&Path>,
) -> String {
    let Some(container_repo_root) = container_repo_root else {
        return command.to_owned();
    };
    let container_cwd =
        container_repo_root.join(process_cwd.strip_prefix(repo_root).unwrap_or(process_cwd));
    let container_local_bin = container_cwd.join("node_modules/.bin");
    let rewritten_command = rewrite_command_for_container(command, repo_root, container_repo_root);
    format!(
        "cd {} && export PATH={}:$PATH; {}",
        shell_quote(&container_cwd.display().to_string()),
        shell_quote(&container_local_bin.display().to_string()),
        rewritten_command
    )
}

pub fn managed_lifecycle_setup_sequence(setup_commands: &[String]) -> String {
    if setup_commands.is_empty() {
        return String::new();
    }
    setup_commands
        .iter()
        .map(|command| format!("if ! {command}; then printf '%s\\n' 'managed lifecycle failed during container setup' 1>&2; exit 1; fi; ", command = command))
        .collect()
}

pub fn managed_gateway_command(executable: &str) -> String {
    format!("env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 {executable} gateway up")
}

pub fn resolve_effigy_invocation_prefix() -> Result<String, std::io::Error> {
    effigy_core::effigy_invocation::resolve_effigy_invocation_prefix(&format!(
        "{}/../../Cargo.toml",
        env!("CARGO_MANIFEST_DIR")
    ))
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
            "printf 'Container Session\\n\\n'; printf 'container: %s\\n' {session_label}; printf 'owner_task: %s\\n' {task_label}; printf 'primary_service: %s\\n' {primary}; printf 'shutdown_on_exit: %s\\n' {owner_exit}; printf 'shutdown_mode: %s\\n\\n' {shutdown}; {executable} container {name} status --repo {repo}",
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
        "{} {executable} container {name} logs --follow --service {service} --repo {repo}",
        effigy_internal_env_prefix(),
        name = shell_quote(&policy.name),
        service = shell_quote(service),
    )
}

fn effigy_container_command(
    executable: &str,
    container_name: Option<&str>,
    subcommand: &str,
    repo: &str,
) -> String {
    effigy_container_command_with_extra(executable, container_name, subcommand, repo)
}

fn effigy_container_command_with_extra(
    executable: &str,
    container_name: Option<&str>,
    subcommand: &str,
    repo: &str,
) -> String {
    match container_name {
        Some(name) => format!(
            "{} {executable} container {name} {subcommand} --repo {repo}",
            effigy_internal_env_prefix(),
            name = shell_quote(name),
        ),
        None => format!(
            "{} {executable} container {subcommand} --repo {repo}",
            effigy_internal_env_prefix(),
        ),
    }
}

fn effigy_internal_env_prefix() -> &'static str {
    "env EFFIGY_INTERNAL_SUPPRESS_HEADER=1"
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

#[cfg(test)]
mod tests {
    use super::{
        managed_lifecycle_command, managed_shell_command, managed_standard_exec_command,
        resolve_effigy_invocation_prefix, ManagedLifecycleRequest, ManagedStandardExecRequest,
    };
    use std::path::Path;

    #[test]
    fn managed_lifecycle_command_renders_one_shot_snapshot_without_screen_clear_loop() {
        let rendered = managed_lifecycle_command(ManagedLifecycleRequest {
            repo_root: Path::new("/tmp/repo"),
            container_name: Some("web"),
            owner_task: "dev",
            health_wait: true,
            health_wait_timeout_secs: 60,
            ready_message: Some("http://project.test"),
            dns_route_lines: &[],
            readiness_probe_urls: &[],
            setup_commands: &[],
            executable: "effigy",
            secrets_required: false,
        });

        assert!(!rendered.contains("\\033[2J\\033[H"), "got: {rendered}");
        assert!(!rendered.contains("sleep 0.2"), "got: {rendered}");
        assert!(
            rendered.contains("lifecycle owner is idle"),
            "got: {rendered}"
        );
        assert!(
            rendered.contains(
                "env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy container web status --repo"
            ),
            "got: {rendered}"
        );
        assert!(
            rendered.contains("env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy container web up --detach --repo /tmp/repo"),
            "got: {rendered}"
        );
        assert!(
            rendered.contains(".effigy/runtime/managed-lifecycle/dev-web.state"),
            "got: {rendered}"
        );
        assert!(rendered.contains("parent_pid=$PPID"), "got: {rendered}");
        let up_index = rendered.find("up --detach").expect("lifecycle up");
        // Single quotes are re-quoted by shell quoting, so match the
        // quote-free state-write markers.
        let started_index = rendered
            .find("started > ")
            .expect("started marker after up");
        let ready_index = rendered.find("ready > ").expect("ready marker");
        assert!(up_index < started_index, "got: {rendered}");
        assert!(started_index < ready_index, "got: {rendered}");
        assert!(
            rendered.contains("while kill -0 \"$parent_pid\" >/dev/null 2>&1; do sleep 1; done"),
            "got: {rendered}"
        );
    }

    #[test]
    fn managed_lifecycle_command_runs_setup_before_ready_projection() {
        let rendered = managed_lifecycle_command(ManagedLifecycleRequest {
            repo_root: Path::new("/tmp/repo"),
            container_name: Some("web"),
            owner_task: "dev",
            health_wait: true,
            health_wait_timeout_secs: 60,
            ready_message: Some("http://project.test"),
            dns_route_lines: &[],
            readiness_probe_urls: &[],
            setup_commands: &[String::from(
                "effigy exec --repo /tmp/repo -- sh -lc 'cd /workspace/app && bun install'",
            )],
            executable: "effigy",
            secrets_required: false,
        });

        assert!(
            rendered.contains("managed lifecycle failed during container setup"),
            "got: {rendered}"
        );
        let setup_index = rendered
            .find("bun install")
            .expect("setup command should be present");
        let ready_index = rendered
            .find("managed ready:")
            .expect("ready banner should be present");
        assert!(setup_index < ready_index, "got: {rendered}");
    }

    #[test]
    fn managed_lifecycle_command_waits_for_probe_urls_before_ready() {
        let rendered = managed_lifecycle_command(ManagedLifecycleRequest {
            repo_root: Path::new("/tmp/repo"),
            container_name: Some("web"),
            owner_task: "dev",
            health_wait: true,
            health_wait_timeout_secs: 17,
            ready_message: Some("routes: http://project.test"),
            dns_route_lines: &["http://project.test -> app".to_owned()],
            readiness_probe_urls: &["http://project.test".to_owned()],
            setup_commands: &[],
            executable: "effigy",
            secrets_required: false,
        });

        assert!(
            rendered.contains("curl -k -s -o /dev/null"),
            "got: {rendered}"
        );
        assert!(rendered.contains("http://project.test"), "got: {rendered}");
        assert!(rendered.contains("+ 17"), "got: {rendered}");
        let probe_index = rendered.find("curl -k -s -o /dev/null").expect("probe");
        let ready_index = rendered.find("managed ready:").expect("ready banner");
        assert!(probe_index < ready_index, "got: {rendered}");
    }

    #[test]
    fn managed_lifecycle_command_can_require_container_secret_injection() {
        let rendered = managed_lifecycle_command(ManagedLifecycleRequest {
            repo_root: Path::new("/tmp/repo"),
            container_name: Some("web"),
            owner_task: "dev",
            health_wait: false,
            health_wait_timeout_secs: 60,
            ready_message: None,
            dns_route_lines: &[],
            readiness_probe_urls: &[],
            setup_commands: &[],
            executable: "effigy",
            secrets_required: true,
        });

        assert!(
            rendered.contains("env EFFIGY_SECRETS_REQUIRED=1 env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy container web up --detach --repo /tmp/repo"),
            "got: {rendered}"
        );
    }

    #[test]
    fn managed_standard_exec_command_waits_for_exec_surface_before_launch() {
        let rendered = managed_standard_exec_command(ManagedStandardExecRequest {
            repo_root: Path::new("/tmp/repo"),
            container_name: Some("web"),
            owner_task: "dev",
            process_cwd: Path::new("/tmp/repo/acme-api"),
            container_repo_root: Some(Path::new("/workspace-root/repo")),
            setup_command: None,
            executable: "effigy",
            command: "printf api-ok",
            exec_readiness_timeout_secs: 45,
        });

        assert!(
            rendered.contains("cd /tmp/repo/acme-api && state_path=/tmp/repo/.effigy/runtime/managed-lifecycle/dev-web.state; deadline=$(( $(date +%s) + 45 )); while true; do if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = starting ]; then sleep 1; continue; fi; if env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy container web shell --command true --repo /tmp/repo >/dev/null 2>&1; then exec env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy container web shell --command"),
            "got: {rendered}"
        );
        assert!(
            rendered.contains("managed lifecycle failed before exec surface became available"),
            "got: {rendered}"
        );
        assert!(
            rendered.contains(
                "cd /workspace-root/repo/acme-api && export PATH=/workspace-root/repo/acme-api/node_modules/.bin:$PATH; printf api-ok"
            ),
            "got: {rendered}"
        );
        assert!(rendered.contains("printf api-ok"), "got: {rendered}");
    }

    #[test]
    fn managed_standard_exec_command_rewrites_host_repo_paths_for_container_commands() {
        let rendered = managed_standard_exec_command(ManagedStandardExecRequest {
            repo_root: Path::new("/Users/tom/repo"),
            container_name: Some("web"),
            owner_task: "dev",
            process_cwd: Path::new("/Users/tom/repo/acme-admin"),
            container_repo_root: Some(Path::new("/workspace-root/repo")),
            setup_command: None,
            executable: "effigy",
            command: "(cd '/Users/tom/repo/acme-admin' && svelte-kit sync)",
            exec_readiness_timeout_secs: 45,
        });

        assert!(
            rendered.contains("/workspace-root/repo/acme-admin"),
            "got: {rendered}"
        );
        assert!(
            rendered
                .contains("cd /workspace-root/repo/acme-admin && export PATH=/workspace-root/repo/acme-admin/node_modules/.bin:$PATH;"),
            "got: {rendered}"
        );
        assert!(rendered.contains("svelte-kit sync"), "got: {rendered}");
    }

    #[test]
    fn managed_standard_exec_command_rewrites_host_effigy_invocation_for_container_commands() {
        let host_effigy =
            resolve_effigy_invocation_prefix().expect("resolve host effigy invocation");
        let rendered = managed_standard_exec_command(ManagedStandardExecRequest {
            repo_root: Path::new("/Users/tom/repo"),
            container_name: Some("web"),
            owner_task: "dev",
            process_cwd: Path::new("/Users/tom/repo/acme-admin"),
            container_repo_root: Some(Path::new("/workspace-root/repo")),
            setup_command: Some(
                &format!(
                    "env EFFIGY_INTERNAL_SUPPRESS_HEADER='1' {host_effigy} script run --file '/Users/tom/repo/.effigy/cache/setup.rhai'"
                ),
            ),
            executable: "effigy",
            command: "bun run dev",
            exec_readiness_timeout_secs: 45,
        });

        assert!(
            rendered.contains("/usr/local/bin/effigy script run --file")
                || rendered.contains("shell --command 'cd /workspace-root/repo/acme-admin")
                    && rendered.contains("/usr/local/bin/effigy"),
            "got: {rendered}"
        );
        assert!(!rendered.contains(&host_effigy), "got: {rendered}");
    }

    #[test]
    fn managed_standard_exec_command_runs_process_setup_before_attach() {
        let rendered = managed_standard_exec_command(ManagedStandardExecRequest {
            repo_root: Path::new("/tmp/repo"),
            container_name: Some("web"),
            owner_task: "dev",
            process_cwd: Path::new("/tmp/repo/acme-front"),
            container_repo_root: Some(Path::new("/workspace-root/repo")),
            setup_command: Some("printf setup-ok; "),
            executable: "effigy",
            command: "bun run dev",
            exec_readiness_timeout_secs: 45,
        });

        let setup_index = rendered.find("printf setup-ok").expect("setup command");
        let attach_index = rendered
            .rfind("shell --command")
            .expect("attach command should be present");
        assert!(setup_index < attach_index, "got: {rendered}");
    }

    #[test]
    fn managed_shell_command_exits_when_lifecycle_fails() {
        let rendered = managed_shell_command(
            Path::new("/tmp/repo"),
            Some("web"),
            "dev",
            Some("workspace"),
            "effigy",
            45,
        );

        assert!(
            rendered
                .contains("state_path=/tmp/repo/.effigy/runtime/managed-lifecycle/dev-web.state"),
            "got: {rendered}"
        );
        assert!(
            rendered.contains("managed lifecycle failed before shell became available"),
            "got: {rendered}"
        );
        assert!(
            rendered.contains("env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy container web shell --service workspace"),
            "got: {rendered}"
        );
    }

    #[test]
    fn managed_standard_exec_command_waits_while_lifecycle_starts() {
        let rendered = managed_standard_exec_command(ManagedStandardExecRequest {
            repo_root: Path::new("/tmp/repo"),
            container_name: Some("web"),
            owner_task: "dev",
            process_cwd: Path::new("/tmp/repo/acme-api"),
            container_repo_root: Some(Path::new("/workspace-root/repo")),
            setup_command: None,
            executable: "effigy",
            command: "printf api-ok",
            exec_readiness_timeout_secs: 60,
        });

        assert!(
            rendered.contains(
                "while true; do if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = starting ]; then sleep 1; continue; fi; if env"
            ),
            "got: {rendered}"
        );
        assert!(rendered.contains("+ 60 )); while true;"), "got: {rendered}");
    }

    #[test]
    fn managed_shell_command_waits_while_lifecycle_starts() {
        let rendered = managed_shell_command(
            Path::new("/tmp/repo"),
            Some("web"),
            "dev",
            Some("workspace"),
            "effigy",
            60,
        );

        assert!(
            rendered.contains(
                "while true; do if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = starting ]; then sleep 1; continue; fi; if env"
            ),
            "got: {rendered}"
        );
        assert!(rendered.contains("+ 60 )); while true;"), "got: {rendered}");
    }

    #[test]
    fn managed_gateway_command_suppresses_header_output() {
        let rendered = super::managed_gateway_command("effigy");

        assert_eq!(
            rendered,
            "env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy gateway up"
        );
    }

    fn wait_for_child(
        child: &mut std::process::Child,
        timeout_secs: u64,
        what: &str,
    ) -> std::process::ExitStatus {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        loop {
            if let Some(status) = child.try_wait().expect("poll child") {
                return status;
            }
            assert!(std::time::Instant::now() < deadline, "timed out: {what}");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// Stub-`effigy` harness for executing the rendered managed scripts.
    ///
    /// The stub emulates `effigy container <name> <subcommand>`: `up` takes
    /// a beat (container startup), `shell --command true` probes readiness,
    /// any other `shell --command` attaches the workload. Probe/attach
    /// markers prove what the script attempted and when. `STUB_MODE` tunes
    /// behavior: `probe-fail` makes every probe fail so the readiness
    /// timeout is observable.
    struct ScriptHarness {
        dir: std::path::PathBuf,
        stub: std::path::PathBuf,
        log: std::path::PathBuf,
    }

    impl ScriptHarness {
        fn setup(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "effigy-managed-script-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("time")
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).expect("mkdir harness");
            let log = dir.join("stub.log");
            let stub = dir.join("effigy-stub.sh");
            std::fs::write(
                &stub,
                format!(
                    "#!/bin/sh\nlog='{log}'\nmode=\"${{STUB_MODE:-ok}}\"\nprintf '%s\\n' \"$*\" >> \"$log\"\ncase \"$*\" in\n  *up*) sleep 1; exit 0 ;;\n  *shell*)\n    if [ \"$mode\" = probe-fail ]; then exit 1; fi\n    case \"$*\" in\n      *'--command true'*) printf probe >> \"$log\" ;;\n      *) printf attach >> \"$log\" ;;\n    esac\n    exit 0 ;;\n  *) exit 0 ;;\nesac\n",
                    log = log.display()
                ),
            )
            .expect("write stub");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&stub).expect("stat stub").permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&stub, perms).expect("chmod stub");
            }
            Self { dir, stub, log }
        }

        fn stub_arg(&self) -> String {
            self.stub.display().to_string()
        }

        fn state_path(&self) -> std::path::PathBuf {
            super::managed_lifecycle_state_path(&self.dir, "web", "dev")
        }

        fn invocations(&self) -> String {
            std::fs::read_to_string(&self.log).unwrap_or_default()
        }

        fn cleanup(&self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Spawn a shell for the script-behavior tests with a fixed shell and PATH.
    ///
    /// The plain parallel cargo harness shares process env, and an unguarded
    /// concurrent PATH write in another test would otherwise make a PATH `sh`
    /// lookup fail with ENOENT. The stub effigy is invoked by absolute path
    /// and the harness dir is unique per test, so nothing else in the
    /// environment matters.
    fn script_shell() -> std::process::Command {
        let mut command = std::process::Command::new("/bin/sh");
        command.env("PATH", "/usr/bin:/bin");
        command
    }

    fn wait_for_state(path: &std::path::Path, expected: &str, timeout_secs: u64) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        loop {
            let current = std::fs::read_to_string(path).unwrap_or_default();
            if current.trim() == expected {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for lifecycle state `{expected}` (got `{}`)",
                current.trim()
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    #[test]
    fn managed_lifecycle_script_marks_started_after_up_before_ready() {
        let harness = ScriptHarness::setup("lifecycle-started");
        let rendered = managed_lifecycle_command(ManagedLifecycleRequest {
            repo_root: &harness.dir,
            container_name: Some("web"),
            owner_task: "dev",
            health_wait: false,
            health_wait_timeout_secs: 60,
            ready_message: None,
            dns_route_lines: &[],
            readiness_probe_urls: &[],
            setup_commands: &[String::from("sleep 2")],
            executable: &harness.stub_arg(),
            secrets_required: false,
        });
        let mut child = script_shell()
            .arg("-c")
            .arg(&rendered)
            .spawn()
            .expect("spawn lifecycle script");
        wait_for_state(&harness.state_path(), "starting", 10);
        // `up` takes a beat in the stub: `started` must land after the
        // startup completes and before the ready banner.
        wait_for_state(&harness.state_path(), "started", 10);
        wait_for_state(&harness.state_path(), "ready", 10);
        let up_at = harness.invocations().find("up --detach").expect("up ran");
        let status_at = harness.invocations().find("status").expect("status ran");
        assert!(up_at < status_at);
        child.kill().expect("stop lifecycle script");
        let _ = child.wait();
        harness.cleanup();
    }

    #[test]
    fn managed_standard_exec_script_waits_while_lifecycle_starts() {
        let harness = ScriptHarness::setup("exec-waits-starting");
        std::fs::create_dir_all(harness.state_path().parent().expect("state parent"))
            .expect("mkdir state parent");
        std::fs::write(harness.state_path(), "starting\n").expect("seed starting");
        let rendered = managed_standard_exec_command(ManagedStandardExecRequest {
            repo_root: &harness.dir,
            container_name: Some("web"),
            owner_task: "dev",
            process_cwd: &harness.dir,
            container_repo_root: None,
            setup_command: None,
            executable: &harness.stub_arg(),
            command: "printf child-ok",
            exec_readiness_timeout_secs: 30,
        });
        let mut child = script_shell()
            .arg("-c")
            .arg(&rendered)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn child script");
        // While the lifecycle reports `starting` the child must not probe
        // or attach: no stub invocation may happen.
        std::thread::sleep(std::time::Duration::from_secs(2));
        assert!(
            !harness.log.exists(),
            "child probed during startup: {}",
            harness.invocations()
        );
        std::fs::write(harness.state_path(), "started\n").expect("seed started");
        let status = wait_for_child(&mut child, 10, "child should attach");
        assert!(status.success(), "child failed after startup");
        let log = harness.invocations();
        assert!(
            log.contains("probe"),
            "expected readiness probe, got: {log}"
        );
        assert!(log.contains("attach"), "expected attach, got: {log}");
        harness.cleanup();
    }

    #[test]
    fn managed_standard_exec_script_exits_when_lifecycle_fails() {
        let harness = ScriptHarness::setup("exec-lifecycle-failed");
        std::fs::create_dir_all(harness.state_path().parent().expect("state parent"))
            .expect("mkdir state parent");
        std::fs::write(harness.state_path(), "failed\n").expect("seed failed");
        let rendered = managed_standard_exec_command(ManagedStandardExecRequest {
            repo_root: &harness.dir,
            container_name: Some("web"),
            owner_task: "dev",
            process_cwd: &harness.dir,
            container_repo_root: None,
            setup_command: None,
            executable: &harness.stub_arg(),
            command: "printf child-ok",
            exec_readiness_timeout_secs: 30,
        });
        let output = script_shell()
            .arg("-c")
            .arg(&rendered)
            // Probes fail against the failed lifecycle's stopped stack, so
            // the loop reaches the lifecycle-failure backstop.
            .env("STUB_MODE", "probe-fail")
            .output()
            .expect("run child script");
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("managed lifecycle failed before exec surface became available"),
            "got: {stderr}"
        );
        harness.cleanup();
    }

    #[test]
    fn managed_standard_exec_script_times_out_with_configured_budget() {
        let harness = ScriptHarness::setup("exec-timeout");
        let rendered = managed_standard_exec_command(ManagedStandardExecRequest {
            repo_root: &harness.dir,
            container_name: Some("web"),
            owner_task: "dev",
            process_cwd: &harness.dir,
            container_repo_root: None,
            setup_command: None,
            executable: &harness.stub_arg(),
            command: "printf child-ok",
            exec_readiness_timeout_secs: 2,
        });
        let output = script_shell()
            .arg("-c")
            .arg(&rendered)
            .env("STUB_MODE", "probe-fail")
            .output()
            .expect("run child script");
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("managed exec timed out waiting for container exec readiness"),
            "got: {stderr}"
        );
        harness.cleanup();
    }

    #[test]
    fn managed_shell_script_waits_while_lifecycle_starts() {
        let harness = ScriptHarness::setup("shell-waits-starting");
        std::fs::create_dir_all(harness.state_path().parent().expect("state parent"))
            .expect("mkdir state parent");
        std::fs::write(harness.state_path(), "starting\n").expect("seed starting");
        let rendered = managed_shell_command(
            &harness.dir,
            Some("web"),
            "dev",
            None,
            &harness.stub_arg(),
            30,
        );
        let mut child = script_shell()
            .arg("-c")
            .arg(&rendered)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn shell script");
        std::thread::sleep(std::time::Duration::from_secs(2));
        assert!(
            !harness.log.exists(),
            "shell probed during startup: {}",
            harness.invocations()
        );
        std::fs::write(harness.state_path(), "started\n").expect("seed started");
        let status = wait_for_child(&mut child, 10, "shell should attach");
        assert!(status.success(), "shell failed after startup");
        harness.cleanup();
    }
}
