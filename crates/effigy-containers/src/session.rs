//! Container attached-session shaping extracted from
//! `src/runner/container_command.rs`.

use std::io::IsTerminal;
use std::path::Path;

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

/// A managed shell tab's interactive attach. The run's supervisor starts
/// it only once the environment is ready, so it carries no readiness loop.
pub fn managed_shell_command(
    repo_root: &Path,
    container_name: Option<&str>,
    service: Option<&str>,
    executable: &str,
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    let selector = container_name
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default");
    let service_flag = service
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!(" --service {}", shell_quote(value)))
        .unwrap_or_default();
    format!(
        "exec {}",
        effigy_container_command(executable, selector, &format!("shell{service_flag}"), &repo)
    )
}

pub struct ManagedExecRequest<'a> {
    pub repo_root: &'a Path,
    pub container_name: Option<&'a str>,
    pub process_cwd: &'a Path,
    pub container_repo_root: Option<&'a Path>,
    pub setup_command: Option<&'a str>,
    pub executable: &'a str,
    pub command: &'a str,
    /// Container-side path of the marker the remote session leader writes,
    /// and the marker's run nonce (or the placeholder the supervisor
    /// replaces). `None` attaches without recording a group.
    pub exec_group: Option<(&'a Path, &'a str)>,
}

/// A managed exec workload: it attaches through the environment's exec
/// surface once the run has observed the environment ready. The remote
/// leader records itself and its nonce so the run's settle can end it.
pub fn managed_exec_command(request: ManagedExecRequest<'_>) -> String {
    let ManagedExecRequest {
        repo_root,
        container_name,
        process_cwd,
        container_repo_root,
        setup_command,
        executable,
        command,
        exec_group,
    } = request;
    let repo = shell_quote(&repo_root.display().to_string());
    let cwd = shell_quote(&process_cwd.display().to_string());
    let routed_command =
        container_exec_command(command, repo_root, process_cwd, container_repo_root);
    let command = shell_quote(&match exec_group {
        Some((file, nonce)) => render_tracked_exec_command(&routed_command, file, nonce),
        None => routed_command,
    });
    let selector = container_name
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default");
    let attach = effigy_container_command(
        executable,
        selector,
        &format!("shell --command {command}"),
        &repo,
    );
    let setup = setup_command
        .map(|setup_command| {
            let routed_setup = shell_quote(&container_exec_command(
                setup_command,
                repo_root,
                process_cwd,
                container_repo_root,
            ));
            format!(
                "{} && ",
                effigy_container_command(
                    executable,
                    selector,
                    &format!("shell --command {routed_setup}"),
                    &repo,
                )
            )
        })
        .unwrap_or_default();
    format!("cd {cwd} && {setup}exec {attach}")
}

/// Wrap a routed exec command so the remote side records the session leader
/// it creates, with this run's `nonce`, under `exec_group_file`, and so the
/// leader owns its own process group when `setsid` exists. The run's settle
/// reads the marker through the shared checkout and ends that group through
/// the exec adapter, because killing only the host exec clients never
/// reaches the remote side. A marker without this nonce is never evidence.
///
/// SEAM(v015-supervisor): the in-band registration of contract 052 §5.10
/// replaces this bind-mount marker once a Compose `RemoteExecAdapter` is
/// qualified on real engines.
pub fn render_tracked_exec_command(
    routed_command: &str,
    exec_group_file: &Path,
    nonce: &str,
) -> String {
    let group_dir = exec_group_file
        .parent()
        .map(|parent| shell_quote(&parent.display().to_string()))
        .unwrap_or_else(|| ".".to_owned());
    let quoted_group_file = shell_quote(&exec_group_file.display().to_string());
    let nonce = shell_quote(nonce);
    let record = format!("printf '%s %s\\n' \"$$\" {nonce} > {quoted_group_file} 2>/dev/null");
    let payload = shell_quote(&format!("{record}; {routed_command}"));
    format!(
        "mkdir -p {group_dir} 2>/dev/null; if command -v setsid >/dev/null 2>&1; then setsid sh -c {payload}; else {record}; {routed_command}; fi"
    )
}

/// One `container <name> shell --command` invocation against the same exec
/// surface managed exec workloads attach through.
pub fn managed_container_shell_command(
    executable: &str,
    container_name: Option<&str>,
    inner_command: &str,
    repo_root: &Path,
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    effigy_container_command(
        executable,
        container_name,
        &format!("shell --command {}", shell_quote(inner_command)),
        &repo,
    )
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
        managed_container_shell_command, managed_exec_command, managed_shell_command,
        resolve_effigy_invocation_prefix, ManagedExecRequest,
    };
    use std::path::Path;

    fn request<'a>(
        repo_root: &'a Path,
        process_cwd: &'a Path,
        setup_command: Option<&'a str>,
        command: &'a str,
    ) -> ManagedExecRequest<'a> {
        ManagedExecRequest {
            repo_root,
            container_name: Some("web"),
            process_cwd,
            container_repo_root: Some(Path::new("/workspace-root/repo")),
            setup_command,
            executable: "effigy",
            command,
            exec_group: Some((
                Path::new("/workspace-root/repo/.effigy/runtime/managed-exec/dev-default/api.pgid"),
                "run-1:api:1",
            )),
        }
    }

    /// The run observes the environment ready before any exec workload
    /// starts, so the attach has no lifecycle state file and no wait loop.
    #[test]
    fn managed_exec_command_attaches_without_a_readiness_loop() {
        let rendered = managed_exec_command(request(
            Path::new("/tmp/repo"),
            Path::new("/tmp/repo/acme-api"),
            None,
            "printf api-ok",
        ));
        assert!(
            rendered.starts_with("cd /tmp/repo/acme-api && exec "),
            "{rendered}"
        );
        for retired in [
            "state_path",
            "deadline",
            "while true",
            "sleep 1",
            "managed-lifecycle",
        ] {
            assert!(!rendered.contains(retired), "{retired}: {rendered}");
        }
    }

    #[test]
    fn managed_exec_command_records_the_remote_session_leader_with_its_nonce() {
        let rendered = managed_exec_command(request(
            Path::new("/tmp/repo"),
            Path::new("/tmp/repo/acme-api"),
            None,
            "printf api-ok",
        ));
        let attach = rendered
            .find("shell --command")
            .map(|index| &rendered[index..])
            .expect("attach command");
        assert!(
            attach.contains("mkdir -p /workspace-root/repo/.effigy/runtime/managed-exec/dev-default 2>/dev/null; if command -v setsid >/dev/null 2>&1; then setsid sh -c"),
            "attach must run under its own recorded session: {attach}"
        );
        assert!(
            attach.contains("run-1:api:1") && attach.contains("/managed-exec/dev-default/api.pgid"),
            "attach must record the session leader and its nonce: {attach}"
        );
        assert!(
            attach.contains("cd /workspace-root/repo/acme-api && export PATH=/workspace-root/repo/acme-api/node_modules/.bin:$PATH; printf api-ok"),
            "the routed command still runs inside the recorded leader: {attach}"
        );
    }

    /// The marker carries the leader and the nonce, separated by a space.
    #[test]
    fn tracked_exec_marker_writes_the_leader_and_nonce() {
        let dir = tempfile::tempdir().expect("marker dir");
        let marker = dir.path().join("deep/api.pgid");
        let script = super::render_tracked_exec_command("true", &marker, "run-1:api:1");
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&script)
            .status()
            .expect("tracked exec");
        assert!(status.success());
        let written = std::fs::read_to_string(&marker).expect("marker");
        let mut fields = written.split_whitespace();
        assert!(fields
            .next()
            .and_then(|pid| pid.parse::<u32>().ok())
            .is_some());
        assert_eq!(fields.next(), Some("run-1:api:1"));
    }

    #[test]
    fn managed_container_shell_command_targets_the_same_exec_surface() {
        let rendered = managed_container_shell_command(
            "effigy",
            Some("web"),
            "kill -TERM -- -4242",
            Path::new("/tmp/repo"),
        );

        assert_eq!(
            rendered,
            "env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy container web shell --command 'kill -TERM -- -4242' --repo /tmp/repo"
        );
    }

    #[test]
    fn managed_exec_command_rewrites_host_repo_paths_for_container_commands() {
        let rendered = managed_exec_command(request(
            Path::new("/Users/tom/repo"),
            Path::new("/Users/tom/repo/acme-admin"),
            None,
            "(cd '/Users/tom/repo/acme-admin' && svelte-kit sync)",
        ));
        assert!(
            rendered
                .contains("cd /workspace-root/repo/acme-admin && export PATH=/workspace-root/repo/acme-admin/node_modules/.bin:$PATH;"),
            "got: {rendered}"
        );
        assert!(rendered.contains("svelte-kit sync"), "got: {rendered}");
    }

    #[test]
    fn managed_exec_command_rewrites_host_effigy_invocation_for_container_commands() {
        let host_effigy =
            resolve_effigy_invocation_prefix().expect("resolve host effigy invocation");
        let setup = format!(
            "env EFFIGY_INTERNAL_SUPPRESS_HEADER='1' {host_effigy} script run --file '/Users/tom/repo/.effigy/cache/setup.rhai'"
        );
        let rendered = managed_exec_command(request(
            Path::new("/Users/tom/repo"),
            Path::new("/Users/tom/repo/acme-admin"),
            Some(&setup),
            "bun run dev",
        ));
        assert!(
            rendered.contains("/usr/local/bin/effigy"),
            "got: {rendered}"
        );
        assert!(!rendered.contains(&host_effigy), "got: {rendered}");
    }

    #[test]
    fn managed_exec_command_runs_process_setup_before_attach() {
        let rendered = managed_exec_command(request(
            Path::new("/tmp/repo"),
            Path::new("/tmp/repo/acme-front"),
            Some("printf setup-ok; "),
            "bun run dev",
        ));
        let setup_index = rendered.find("printf setup-ok").expect("setup command");
        let attach_index = rendered
            .rfind("shell --command")
            .expect("attach command should be present");
        assert!(setup_index < attach_index, "got: {rendered}");
    }

    #[test]
    fn managed_shell_command_attaches_directly() {
        let rendered = managed_shell_command(
            Path::new("/tmp/repo"),
            Some("web"),
            Some("workspace"),
            "effigy",
        );
        assert_eq!(
            rendered,
            "exec env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 effigy container web shell --service workspace --repo /tmp/repo"
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
