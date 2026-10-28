use std::path::Path;

use effigy_containers::compose::{
    compose_args, compose_invocation_for_repo, resolve_host_cli_program,
};
use effigy_containers::session::container_exec_command;
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
        .filter(|route| route.target_host.is_none())
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

/// An inline workspace's interactive shell tab. The run's supervisor starts
/// it only once the environment is ready.
pub(crate) fn render_inline_managed_shell_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: Option<&str>,
) -> String {
    let service_name = service
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(policy.primary_service.as_str());
    let attach = render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(policy, ["exec", service_name, "sh"]),
    );
    format!("exec sh -c {}", shell_quote(&attach))
}

pub(crate) struct InlineManagedExecRequest<'a> {
    pub repo_root: &'a Path,
    pub policy: &'a EffectiveContainerPolicy,
    pub process_cwd: &'a Path,
    pub container_repo_root: Option<&'a Path>,
    pub setup_command: Option<&'a str>,
    pub command: &'a str,
    /// Container-side marker path and run nonce; `None` records no group.
    pub exec_group: Option<(&'a Path, &'a str)>,
    pub env_names: &'a [String],
}

/// An inline workspace's exec workload: Compose exec once the run has
/// observed the environment ready, with the leader recorded for settle.
pub(crate) fn render_inline_managed_exec_command(request: InlineManagedExecRequest<'_>) -> String {
    let InlineManagedExecRequest {
        repo_root,
        policy,
        process_cwd,
        container_repo_root,
        setup_command,
        command,
        exec_group,
        env_names,
    } = request;
    let cwd = shell_quote(&process_cwd.display().to_string());
    let rewritten = container_exec_command(command, repo_root, process_cwd, container_repo_root);
    let tracked = match exec_group {
        Some((file, nonce)) => {
            effigy_containers::session::render_tracked_exec_command(&rewritten, file, nonce)
        }
        None => rewritten,
    };
    let mut tail = vec!["exec".into(), "-T".into()];
    for name in env_names {
        tail.extend([std::ffi::OsString::from("-e"), name.into()]);
    }
    tail.extend(
        [
            policy.primary_service.as_str(),
            "sh",
            "-lc",
            tracked.as_str(),
        ]
        .map(std::ffi::OsString::from),
    );
    let args = effigy_containers::compose::normalize_compose_command_args(policy, &tail);
    let (program, args) = compose_invocation_for_repo(repo_root, policy, &args);
    let attach = if env_names.is_empty() {
        render_inline_compose_command(repo_root, policy, &args)
    } else {
        let program = resolve_host_cli_program(program);
        crate::runner::managed_secrets::relay(&program, &args, env_names)
    };
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
    format!("cd {cwd} && {setup_sequence}{{ {attach}\n}}")
}

fn format_os_args(args: &[std::ffi::OsString]) -> String {
    args.iter()
        .map(|arg| shell_quote(&arg.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ")
}
