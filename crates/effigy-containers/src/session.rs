//! Container attached-session shaping extracted from
//! `src/runner/container_command.rs`.

use std::io::IsTerminal;
use std::path::Path;

const MANAGED_EXEC_READINESS_TIMEOUT_SECS: u64 = 30;

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

pub fn managed_lifecycle_command(
    repo_root: &Path,
    container_name: Option<&str>,
    owner_task: &str,
    health_wait: bool,
    ready_message: Option<&str>,
    setup_commands: &[String],
    executable: &str,
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    let lifecycle_owner_task = owner_task;
    let owner_task = shell_quote(owner_task);
    let selector = container_name
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default");
    let lifecycle_state = managed_lifecycle_state_path(repo_root, selector, lifecycle_owner_task);
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let up = effigy_container_command(executable, selector, "up --detach", &repo);
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
    let setup_sequence = render_managed_lifecycle_setup_sequence(setup_commands);
    let idle_wait = managed_lifecycle_idle_wait_command();
    format!(
        "sh -lc {script}",
        script = shell_quote(&format!(
            "state_path={lifecycle_state}; parent_pid=$PPID; mkdir -p \"$(dirname \"$state_path\")\"; printf '%s\\n' starting > \"$state_path\"; started=0; cleanup() {{ if [ \"$started\" = 1 ]; then printf '%s\\n' stopped > \"$state_path\"; {down} >/dev/null 2>&1 || true; else printf '%s\\n' failed > \"$state_path\"; fi; }}; trap 'cleanup' EXIT INT TERM; printf 'managed lifecycle: %s\\n' {readiness_status}; if ! {up}; then printf '%s\\n' 'managed lifecycle failed during container startup' 1>&2; exit 1; fi; started=1; {setup_sequence}printf '%s\\n' ready > \"$state_path\"; printf 'managed ready: %s\\n' {ready_banner}; printf 'Managed Container Lifecycle\\n\\n'; printf 'container: %s\\n' {label}; printf 'owner_task: %s\\n' {owner_task}; printf 'readiness: %s\\n' {readiness_status}; printf 'ready_message: %s\\n\\n' {ready_banner}; {status} || true; printf '\\n[info] lifecycle owner is idle; use `effigy container {label} status` to refresh.\\n'; {idle_wait}",
            label = shell_quote(label),
            readiness_status = shell_quote(readiness_status),
            ready_banner = shell_quote(&ready_banner),
            setup_sequence = setup_sequence,
            idle_wait = idle_wait,
        )),
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
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    let selector = container_name
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default");
    let lifecycle_state = managed_lifecycle_state_path(repo_root, selector, owner_task);
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
            "state_path={lifecycle_state}; deadline=$(( $(date +%s) + {timeout_secs} )); while true; do if {readiness_probe}; then exec {attach}; fi; if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = failed ]; then printf '%s\\n' 'managed lifecycle failed before shell became available' 1>&2; exit 1; fi; if [ \"$(date +%s)\" -ge \"$deadline\" ]; then printf '%s\\n' 'managed shell timed out waiting for container exec readiness' 1>&2; exit 1; fi; sleep 1; done",
            timeout_secs = MANAGED_EXEC_READINESS_TIMEOUT_SECS,
        )),
    )
}

fn managed_lifecycle_idle_wait_command() -> &'static str {
    "while kill -0 \"$parent_pid\" >/dev/null 2>&1; do sleep 1; done"
}

pub fn managed_standard_exec_command(
    repo_root: &Path,
    container_name: Option<&str>,
    owner_task: &str,
    process_cwd: &Path,
    container_repo_root: Option<&Path>,
    setup_command: Option<&str>,
    executable: &str,
    command: &str,
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    let cwd = shell_quote(&process_cwd.display().to_string());
    let command = shell_quote(&container_exec_command(
        command,
        repo_root,
        process_cwd,
        container_repo_root,
    ));
    let selector = container_name
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default");
    let lifecycle_state = managed_lifecycle_state_path(repo_root, selector, owner_task);
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
    let setup_sequence = setup_command.unwrap_or("");
    format!(
        "sh -lc {script}",
        script = shell_quote(&format!(
            "cd {cwd} && state_path={lifecycle_state}; deadline=$(( $(date +%s) + {timeout_secs} )); while true; do if {readiness_probe}; then {setup_sequence}exec {attach}; fi; if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = failed ]; then printf '%s\\n' 'managed lifecycle failed before exec surface became available' 1>&2; exit 1; fi; if [ \"$(date +%s)\" -ge \"$deadline\" ]; then printf '%s\\n' 'managed exec timed out waiting for container exec readiness' 1>&2; exit 1; fi; sleep 1; done",
            timeout_secs = MANAGED_EXEC_READINESS_TIMEOUT_SECS,
            setup_sequence = setup_sequence,
        )),
    )
}

fn managed_lifecycle_state_path(
    repo_root: &Path,
    container_name: Option<&str>,
    owner_task: &str,
) -> std::path::PathBuf {
    let container_label = container_name.unwrap_or("default");
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
    command.replace(
        &repo_root.display().to_string(),
        &container_repo_root.display().to_string(),
    )
}

fn container_exec_command(
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

fn render_managed_lifecycle_setup_sequence(setup_commands: &[String]) -> String {
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
    if let Some(explicit) = effigy_core::executable_override::current() {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(shell_quote(trimmed));
        }
    }

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
    use super::{managed_lifecycle_command, managed_shell_command, managed_standard_exec_command};
    use std::path::Path;

    #[test]
    fn managed_lifecycle_command_renders_one_shot_snapshot_without_screen_clear_loop() {
        let rendered = managed_lifecycle_command(
            Path::new("/tmp/repo"),
            Some("web"),
            "dev",
            true,
            Some("http://project.test"),
            &[],
            "effigy",
        );

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
        assert!(
            rendered.contains("while kill -0 \"$parent_pid\" >/dev/null 2>&1; do sleep 1; done"),
            "got: {rendered}"
        );
    }

    #[test]
    fn managed_lifecycle_command_runs_setup_before_ready_projection() {
        let rendered = managed_lifecycle_command(
            Path::new("/tmp/repo"),
            Some("web"),
            "dev",
            true,
            Some("http://project.test"),
            &[String::from(
                "effigy exec --repo /tmp/repo -- sh -lc 'cd /workspace/app && bun install'",
            )],
            "effigy",
        );

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
    fn managed_standard_exec_command_waits_for_exec_surface_before_launch() {
        let rendered = managed_standard_exec_command(
            Path::new("/tmp/repo"),
            Some("web"),
            "dev",
            Path::new("/tmp/repo/acme-api"),
            Some(Path::new("/workspace-root/repo")),
            None,
            "effigy",
            "printf api-ok",
        );

        assert!(
            rendered.contains("cd /tmp/repo/acme-api && state_path=/tmp/repo/.effigy/runtime/managed-lifecycle/dev-web.state; deadline=$(( $(date +%s) + 30 )); while true; do if env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy container web shell --command true --repo /tmp/repo >/dev/null 2>&1; then exec env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy container web shell --command"),
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
        let rendered = managed_standard_exec_command(
            Path::new("/Users/tom/repo"),
            Some("web"),
            "dev",
            Path::new("/Users/tom/repo/acme-admin"),
            Some(Path::new("/workspace-root/repo")),
            None,
            "effigy",
            "(cd '/Users/tom/repo/acme-admin' && svelte-kit sync)",
        );

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
    fn managed_standard_exec_command_runs_process_setup_before_attach() {
        let rendered = managed_standard_exec_command(
            Path::new("/tmp/repo"),
            Some("web"),
            "dev",
            Path::new("/tmp/repo/acme-front"),
            Some(Path::new("/workspace-root/repo")),
            Some("printf setup-ok; "),
            "effigy",
            "bun run dev",
        );

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
    fn managed_gateway_command_suppresses_header_output() {
        let rendered = super::managed_gateway_command("effigy");

        assert_eq!(
            rendered,
            "env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy gateway up"
        );
    }
}
