//! The gateway's TCP aliases inside a container.
//!
//! A container reaching a routed TCP service by name resolves it in its own
//! `/etc/hosts`; these are the pairs, the script that writes them, and the
//! installation through the environment's own verified binding.

use std::path::Path;

use effigy_containers::{ContainerAction, EffectiveContainerPolicy};
use effigy_core::shell::shell_quote;
use serde_json::json;

use crate::runner::container_command::gateway_registration::{
    resolve_gateway_tcp_alias_routes_for_container, RegisteredGatewayRoute,
};
use crate::runner::container_command::RunnerError;

pub(in crate::runner::container_command) fn annotate_registered_gateway_routes(
    report: &mut effigy_containers::ContainerCommandReport,
    routes: &[RegisteredGatewayRoute],
) {
    if routes.is_empty() {
        return;
    }
    if let Some(json_object) = report.json.as_object_mut() {
        json_object.insert(
            "gateway_routes".to_owned(),
            json!(routes
                .iter()
                .map(|route| json!({
                    "action": "registered",
                    "domain": route.domain,
                    "target": route.target,
                    "dns_ip": route.dns_ip.map(|value| value.to_string()),
                    "tls": route.tls,
                }))
                .collect::<Vec<_>>()),
        );
    }
    for route in routes {
        report.success_text.push('\n');
        match route.target.as_deref() {
            Some(target) => report.success_text.push_str(&format!(
                "[gateway] registered {} -> {}",
                route.domain, target
            )),
            None => report.success_text.push_str(&format!(
                "[gateway] registered {} -> dns {}",
                route.domain,
                route
                    .dns_ip
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "default".to_owned())
            )),
        }
    }
}

pub(in crate::runner::container_command) fn install_primary_service_tcp_alias_hosts(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<Vec<String>, RunnerError> {
    let pairs = tcp_alias_host_pairs(policy, routes);
    if pairs.is_empty() {
        return Ok(Vec::new());
    }

    let script = render_tcp_alias_hosts_script(&pairs);
    let tail = vec![
        "exec",
        "-T",
        "--user",
        "root",
        policy.primary_service.as_str(),
        "sh",
        "-lc",
        script.as_str(),
    ];
    let plan = effigy_runtime::container_manager::compose_invocation_plan(
        repo_root,
        policy,
        tail,
        ContainerAction::Exec,
        "docker compose exec tcp alias hosts",
    )
    .map_err(RunnerError::from)?;
    let output = crate::runner::exec_command::run_verified_compose_exec_plan(
        policy, &plan, binding, true, None, None,
    )?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(
            "verified TCP alias installation failed",
        ));
    }

    Ok(pairs
        .into_iter()
        .map(|(domain, host)| format!("{domain} -> {host}"))
        .collect())
}

pub(in crate::runner) fn reconcile_primary_service_tcp_alias_hosts(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
    deadline: Option<std::time::Instant>,
) -> Result<Vec<String>, RunnerError> {
    let routes = resolve_gateway_tcp_alias_routes_for_container(repo_root, policy, deadline)?;
    install_primary_service_tcp_alias_hosts(repo_root, policy, &routes, binding)
}

pub(super) fn tcp_alias_host_pairs(
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
) -> Vec<(String, String)> {
    let shared_hosts = policy
        .shared_services
        .iter()
        .map(|shared| (shared.service_name.as_str(), shared.host.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut pairs = std::collections::BTreeSet::new();
    for route in routes {
        let Some(service) = route.service.as_deref() else {
            continue;
        };
        if route.tcp_port.is_none() {
            continue;
        }
        let host = shared_hosts.get(service).copied().unwrap_or(service);
        pairs.insert((route.domain.clone(), host.to_owned()));
    }
    pairs.into_iter().collect()
}

pub(super) fn render_tcp_alias_hosts_script(pairs: &[(String, String)]) -> String {
    // This script runs inside the service container, so its `mktemp` writes to
    // the guest's own temp directory; it deliberately leaves TMPDIR alone.
    let mut script = String::from(
        r#"set -eu
patch_alias() {
  alias="$1"
  service="$2"
  ip="$(getent hosts "$service" | awk 'NR == 1 { print $1 }')"
  if [ -z "$ip" ]; then
    printf '[effigy] could not resolve service `%s` for alias `%s`\n' "$service" "$alias" >&2
    return 0
  fi
  tmp="$(mktemp)"
  awk -v alias="$alias" '{
    keep=1
    for (i = 2; i <= NF; i++) {
      if ($i == alias) {
        keep=0
      }
    }
    if (keep) {
      print
    }
  }' /etc/hosts > "$tmp"
  cat "$tmp" > /etc/hosts
  rm -f "$tmp"
  printf '%s %s\n' "$ip" "$alias" >> /etc/hosts
}
"#,
    );
    for (domain, service) in pairs {
        script.push_str("patch_alias ");
        script.push_str(&shell_quote(domain));
        script.push(' ');
        script.push_str(&shell_quote(service));
        script.push('\n');
    }
    script
}
