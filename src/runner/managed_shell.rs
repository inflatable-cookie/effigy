use std::path::Path;

use effigy_containers::compose::{
    compose_args, compose_invocation_for_repo, resolve_host_cli_program,
};
use effigy_containers::session::{
    container_exec_command, managed_lifecycle_dns_routes_section,
    managed_lifecycle_idle_wait_command, managed_lifecycle_readiness_wait,
    managed_lifecycle_setup_sequence, managed_lifecycle_state_path,
    MANAGED_EXEC_READINESS_TIMEOUT_SECS,
};
use effigy_containers::EffectiveContainerPolicy;
use effigy_core::shell::shell_quote;

pub(crate) fn managed_readiness_probe_urls(
    policy: Option<&EffectiveContainerPolicy>,
) -> Vec<String> {
    let Some(policy) = policy else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    policy
        .dns_routes
        .iter()
        .filter_map(|route| {
            let domain = route.domain.trim();
            if domain.is_empty() {
                return None;
            }
            let scheme = if route.tls { "https" } else { "http" };
            let url = format!("{scheme}://{domain}");
            seen.insert(url.clone()).then_some(url)
        })
        .collect()
}

pub(crate) fn render_inline_compose_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[std::ffi::OsString],
) -> String {
    let (program, resolved_args) = compose_invocation_for_repo(repo_root, policy, args);
    let resolved_program = match program {
        value if value.contains('/') => value.to_owned(),
        value => resolve_host_cli_program(value)
            .to_string_lossy()
            .into_owned(),
    };
    format!(
        "cd {} && {} {}",
        shell_quote(&repo_root.display().to_string()),
        shell_quote(&resolved_program),
        format_os_args(&resolved_args),
    )
}

pub(crate) struct InlineManagedLifecycle<'a> {
    pub(crate) owner_task: &'a str,
    pub(crate) health_wait: bool,
    pub(crate) ready_message: Option<&'a str>,
    pub(crate) dns_route_lines: &'a [String],
    pub(crate) readiness_probe_urls: &'a [String],
    pub(crate) setup_commands: &'a [String],
}

pub(crate) fn render_inline_managed_lifecycle_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    lifecycle: InlineManagedLifecycle<'_>,
) -> String {
    let InlineManagedLifecycle {
        owner_task,
        health_wait,
        ready_message,
        dns_route_lines,
        readiness_probe_urls,
        setup_commands,
    } = lifecycle;
    let lifecycle_state = managed_lifecycle_state_path(repo_root, &policy.name, owner_task);
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let up = render_inline_compose_command(repo_root, policy, &compose_args(policy, ["up", "-d"]));
    let ps = render_inline_compose_command(repo_root, policy, &compose_args(policy, ["ps"]));
    let down = render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(policy, ["down", "--remove-orphans"]),
    );
    let readiness_status = if health_wait {
        "waiting for readiness via detached container startup"
    } else {
        "startup does not declare managed readiness waiting"
    };
    let ready_banner = ready_message
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("container `{}` is ready", policy.name));
    let dns_routes_section = managed_lifecycle_dns_routes_section(dns_route_lines);
    let readiness_wait = managed_lifecycle_readiness_wait(
        health_wait,
        readiness_probe_urls,
        "managed lifecycle readiness wait timed out",
    );
    let setup_sequence = managed_lifecycle_setup_sequence(setup_commands);
    let idle_wait = managed_lifecycle_idle_wait_command();
    format!(
        "sh -lc {}",
        shell_quote(&format!(
            "state_path={lifecycle_state}; parent_pid=$PPID; mkdir -p \"$(dirname \"$state_path\")\"; printf '%s\\n' starting > \"$state_path\"; started=0; cleanup() {{ if [ \"$started\" = 1 ]; then printf '%s\\n' stopped > \"$state_path\"; {down} >/dev/null 2>&1 || true; else printf '%s\\n' failed > \"$state_path\"; fi; }}; trap 'cleanup' EXIT INT TERM; printf 'managed lifecycle: %s\\n' {readiness_status}; if ! {up}; then printf '%s\\n' 'managed lifecycle failed during container startup' 1>&2; exit 1; fi; started=1; {setup_sequence}{readiness_wait}printf '%s\\n' ready > \"$state_path\"; printf 'managed ready: %s\\n' {ready_banner}; printf 'Managed Container Lifecycle\\n\\n'; printf 'container: %s\\n' {label}; printf 'owner_task: %s\\n' {owner_task}; printf 'readiness: %s\\n' {readiness_status}; {dns_routes_section}printf 'ready_message: %s\\n\\n' {ready_banner}; {ps} || true; printf '\\n[info] lifecycle owner is idle; use compose status to refresh.\\n'; {idle_wait}",
            label = shell_quote(&policy.name),
            owner_task = shell_quote(owner_task),
            readiness_status = shell_quote(readiness_status),
            ready_banner = shell_quote(&ready_banner),
            dns_routes_section = dns_routes_section,
            readiness_wait = readiness_wait,
            setup_sequence = setup_sequence,
            idle_wait = idle_wait,
        ))
    )
}

pub(crate) fn render_handoff_managed_lifecycle_command(
    repo_root: &Path,
    container_label: &str,
    owner_task: &str,
    health_wait: bool,
    ready_message: Option<&str>,
    dns_route_lines: &[String],
    setup_commands: &[String],
) -> String {
    let lifecycle_state = managed_lifecycle_state_path(repo_root, container_label, owner_task);
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let readiness_status = if health_wait {
        "workspace container is already running in handoff mode"
    } else {
        "running inside workspace container handoff"
    };
    let ready_banner = ready_message
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("workspace container `{container_label}` is ready"));
    let dns_routes_section = managed_lifecycle_dns_routes_section(dns_route_lines);
    let setup_sequence = managed_lifecycle_setup_sequence(setup_commands);
    let idle_wait = managed_lifecycle_idle_wait_command();
    format!(
        "sh -lc {}",
        shell_quote(&format!(
            "state_path={lifecycle_state}; parent_pid=$PPID; mkdir -p \"$(dirname \"$state_path\")\"; printf '%s\\n' starting > \"$state_path\"; cleanup() {{ printf '%s\\n' stopped > \"$state_path\"; }}; trap 'cleanup' EXIT INT TERM; printf 'managed lifecycle: %s\\n' {readiness_status}; {setup_sequence}printf '%s\\n' ready > \"$state_path\"; printf 'managed ready: %s\\n' {ready_banner}; printf 'Managed Container Lifecycle\\n\\n'; printf 'container: %s\\n' {label}; printf 'owner_task: %s\\n' {owner_task}; printf 'readiness: %s\\n' {readiness_status}; {dns_routes_section}printf 'ready_message: %s\\n\\n' {ready_banner}; printf '[info] lifecycle owner is idle; workspace container handoff is already active.\\n'; {idle_wait}",
            label = shell_quote(container_label),
            owner_task = shell_quote(owner_task),
            readiness_status = shell_quote(readiness_status),
            ready_banner = shell_quote(&ready_banner),
            dns_routes_section = dns_routes_section,
            setup_sequence = setup_sequence,
            idle_wait = idle_wait,
        ))
    )
}

pub(crate) fn render_inline_managed_shell_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    owner_task: &str,
    service: Option<&str>,
) -> String {
    let lifecycle_state = managed_lifecycle_state_path(repo_root, &policy.name, owner_task);
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let service_name = service
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(policy.primary_service.as_str());
    let readiness_probe = render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(policy, ["exec", service_name, "sh", "-lc", "true"]),
    );
    let attach = render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(policy, ["exec", service_name, "sh"]),
    );
    format!(
        "sh -lc {}",
        shell_quote(&format!(
            "state_path={lifecycle_state}; while true; do if {readiness_probe} >/dev/null 2>&1; then {attach}; exit $?; fi; if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = failed ]; then printf '%s\\n' 'managed lifecycle failed before shell became available' 1>&2; exit 1; fi; sleep 1; done"
        ))
    )
}

pub(crate) fn render_inline_managed_standard_exec_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    owner_task: &str,
    process_cwd: &Path,
    container_repo_root: Option<&Path>,
    setup_command: Option<&str>,
    command: &str,
) -> String {
    let cwd = shell_quote(&process_cwd.display().to_string());
    let lifecycle_state = managed_lifecycle_state_path(repo_root, &policy.name, owner_task);
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let probe = render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(
            policy,
            [
                "exec",
                "-T",
                policy.primary_service.as_str(),
                "sh",
                "-lc",
                "true",
            ],
        ),
    );
    let rewritten = container_exec_command(command, repo_root, process_cwd, container_repo_root);
    let attach = render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(
            policy,
            [
                "exec",
                "-T",
                policy.primary_service.as_str(),
                "sh",
                "-lc",
                rewritten.as_str(),
            ],
        ),
    );
    let setup_sequence = setup_command
        .map(|setup_command| {
            let rewritten =
                container_exec_command(setup_command, repo_root, process_cwd, container_repo_root);
            let setup = render_inline_compose_command(
                repo_root,
                policy,
                &compose_args(
                    policy,
                    [
                        "exec",
                        "-T",
                        policy.primary_service.as_str(),
                        "sh",
                        "-lc",
                        rewritten.as_str(),
                    ],
                ),
            );
            format!("{setup} && ")
        })
        .unwrap_or_default();
    format!(
        "sh -lc {}",
        shell_quote(&format!(
            "cd {cwd} && state_path={lifecycle_state}; deadline=$(( $(date +%s) + {timeout_secs} )); while true; do if {probe} >/dev/null 2>&1; then {setup_sequence}{attach}; exit $?; fi; if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = failed ]; then printf '%s\\n' 'managed lifecycle failed before exec surface became available' 1>&2; exit 1; fi; if [ \"$(date +%s)\" -ge \"$deadline\" ]; then printf '%s\\n' 'managed exec timed out waiting for container exec readiness' 1>&2; exit 1; fi; sleep 1; done",
            timeout_secs = MANAGED_EXEC_READINESS_TIMEOUT_SECS,
            setup_sequence = setup_sequence,
        ))
    )
}

fn format_os_args(args: &[std::ffi::OsString]) -> String {
    args.iter()
        .map(|arg| shell_quote(&arg.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ")
}
