use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_containers::exec::{
    list_running_compose_containers_for_profile, RunningComposeContainer,
};
use effigy_containers::{EffectiveContainerPolicy, EffectiveDnsRoute};
use effigy_gateway::registration::{deregister_route, register_route, RouteRegistration};
use serde_yaml::Value as YamlValue;

use crate::runner::error::RunnerError;
use crate::runner::gateway_command::{
    ensure_gateway_tls_cert, gateway_dir, remove_gateway_tls_cert,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct RegisteredGatewayRoute {
    pub(in crate::runner) domain: String,
    pub(in crate::runner) target: String,
    pub(in crate::runner) tls: bool,
    pub(in crate::runner) service: Option<String>,
}

pub(in crate::runner) fn register_gateway_routes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    let routes = resolve_gateway_routes(policy)?;
    validate_gateway_routes_against_runtime(repo_root, policy, &routes)?;
    for route in &routes {
        if route.tls {
            ensure_gateway_tls_cert(&route.domain)?;
        }
    }
    let route_table_path = gateway_route_table_path()?;
    for route in &routes {
        register_gateway_route_at(&route_table_path, repo_root, route)?;
    }
    Ok(routes)
}

pub(in crate::runner) fn deregister_gateway_routes_for_container(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let routes = resolve_gateway_routes(policy)?;
    if routes.is_empty() {
        return Ok(Vec::new());
    }
    let route_table_path = gateway_route_table_path()?;
    for route in &routes {
        if route.tls {
            remove_gateway_tls_cert(&route.domain)?;
        }
        deregister_gateway_route_at(&route_table_path, &route.domain)?;
    }
    Ok(routes.into_iter().map(|route| route.domain).collect())
}

fn register_gateway_route_at(
    route_table_path: &Path,
    repo_root: &Path,
    route: &RegisteredGatewayRoute,
) -> Result<(), RunnerError> {
    register_route(
        route_table_path,
        &RouteRegistration {
            domain: route.domain.clone(),
            target: route.target.clone(),
            dns_ip: None,
            tls: route.tls,
            project_path: repo_root.display().to_string(),
            source: effigy_gateway::routes::RouteSource::Container,
        },
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn deregister_gateway_route_at(route_table_path: &Path, domain: &str) -> Result<(), RunnerError> {
    deregister_route(route_table_path, domain)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn resolve_gateway_routes(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    let mut routes = Vec::new();
    for dns_route in &policy.dns_routes {
        let host_port = selected_host_port_for_route(policy, dns_route)?;
        routes.push(RegisteredGatewayRoute {
            domain: dns_route.domain.clone(),
            target: format!("127.0.0.1:{host_port}"),
            tls: dns_route.tls,
            service: dns_route.service.clone(),
        });
    }
    Ok(routes)
}

fn validate_gateway_routes_against_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
) -> Result<(), RunnerError> {
    if routes.is_empty() {
        return Ok(());
    }
    let rows = list_running_compose_containers_for_profile(&policy.profile)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    validate_gateway_routes_against_rows(repo_root, policy, routes, &rows)?;
    validate_gateway_routes_against_host_listeners(policy, routes)
}

fn validate_gateway_routes_against_rows(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
    rows: &[RunningComposeContainer],
) -> Result<(), RunnerError> {
    for route in routes {
        let target_port = parse_target_host_port(&route.target)?;
        if rows
            .iter()
            .filter(|row| row_matches_policy_project(row, repo_root, policy))
            .filter(|row| row_matches_route_service(row, route))
            .any(|row| row_publishes_host_port(row, target_port))
        {
            continue;
        }
        return Err(RunnerError::task_invocation(format!(
            "container `{}` selected gateway target `{}` for domain `{}` but no running container in project `{}`{} publishes host port {}; gateway registration refuses to target an unrelated runtime binding",
            policy.name,
            route.target,
            route.domain,
            policy.project_name,
            route
                .service
                .as_deref()
                .map(|service| format!(" service `{service}`"))
                .unwrap_or_default(),
            target_port
        )));
    }
    Ok(())
}

fn validate_gateway_routes_against_host_listeners(
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
) -> Result<(), RunnerError> {
    for route in routes {
        let target_port = parse_target_host_port(&route.target)?;
        let Some(listener_command) = non_runtime_listener_for_port(target_port)? else {
            continue;
        };
        return Err(RunnerError::task_invocation(format!(
            "container `{}` selected gateway target `{}` for domain `{}` but host port {} is already held by `{}`; gateway registration refuses to target an unrelated host listener",
            policy.name, route.target, route.domain, target_port, listener_command
        )));
    }
    Ok(())
}

fn parse_target_host_port(target: &str) -> Result<u16, RunnerError> {
    let port = target
        .rsplit(':')
        .next()
        .unwrap_or_default()
        .trim()
        .parse::<u16>()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "gateway route target `{target}` does not end in a valid host port: {error}"
            ))
        })?;
    Ok(port)
}

fn row_matches_policy_project(
    row: &RunningComposeContainer,
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> bool {
    let repo_root = repo_root.to_string_lossy();
    row.project_name.as_deref() == Some(policy.project_name.as_str())
        && row
            .working_dir
            .as_deref()
            .is_none_or(|working_dir| working_dir == repo_root.as_ref())
}

fn row_publishes_host_port(row: &RunningComposeContainer, target_port: u16) -> bool {
    row.ports.iter().any(|port| {
        parse_published_host_port_range(port)
            .ok()
            .is_some_and(|(start, end)| (start..=end).contains(&target_port))
    })
}

fn row_matches_route_service(
    row: &RunningComposeContainer,
    route: &RegisteredGatewayRoute,
) -> bool {
    route
        .service
        .as_deref()
        .is_none_or(|service| row.service.as_deref() == Some(service))
}

fn parse_published_host_port_range(raw: &str) -> Result<(u16, u16), RunnerError> {
    let Some(published) = raw.split("->").next() else {
        return Err(RunnerError::task_invocation(format!(
            "runtime port mapping `{raw}` is missing a published-port segment"
        )));
    };
    let candidate = published.rsplit(':').next().unwrap_or_default().trim();
    let (start, end) = parse_port_range(candidate).map_err(|error| {
        RunnerError::task_invocation(format!(
            "runtime port mapping `{raw}` does not expose a valid published host port: {error}"
        ))
    })?;
    Ok((start, end))
}

fn parse_port_range(raw: &str) -> Result<(u16, u16), String> {
    let raw = raw.trim();
    if let Some((start, end)) = raw.split_once('-') {
        let start = start
            .trim()
            .parse::<u16>()
            .map_err(|error| error.to_string())?;
        let end = end
            .trim()
            .parse::<u16>()
            .map_err(|error| error.to_string())?;
        if start > end {
            return Err("range start exceeds range end".to_owned());
        }
        return Ok((start, end));
    }
    let port = raw.parse::<u16>().map_err(|error| error.to_string())?;
    Ok((port, port))
}

fn non_runtime_listener_for_port(port: u16) -> Result<Option<String>, RunnerError> {
    let output = Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
        .output();
    let Ok(output) = output else {
        return Ok(None);
    };
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
    {
        let mut fields = line.split_whitespace();
        let command = fields.next().unwrap_or_default().trim();
        let pid = fields.next().and_then(|raw| raw.parse::<u32>().ok());
        if command.is_empty() {
            continue;
        }
        let full_command = pid
            .and_then(full_process_command)
            .unwrap_or_else(|| command.to_owned());
        if !listener_command_looks_runtime_managed(&full_command) {
            return Ok(Some(command.to_owned()));
        }
    }
    Ok(None)
}

fn full_process_command(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let rendered = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

fn listener_command_looks_runtime_managed(command: &str) -> bool {
    let lowered = command.to_ascii_lowercase();
    [
        "colima",
        "limactl",
        "docker",
        "docker-proxy",
        "containerd",
        "rootlesskit",
        "slirp4netns",
        "gvproxy",
        "vpnkit",
        "qemu",
        "lima",
        "podman",
        "orb",
        ".colima/_lima/",
        "ssh.sock",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn selected_host_port_for_route(
    policy: &EffectiveContainerPolicy,
    dns_route: &EffectiveDnsRoute,
) -> Result<u16, RunnerError> {
    if let Some(port) = dns_route.port {
        return if policy.ports_declared_explicitly {
            selected_declared_host_port(policy, port)
        } else if let Some(service) = dns_route.service.as_deref() {
            selected_effective_container_port_for_service(policy, service, port)
        } else {
            selected_effective_container_port(policy, port)
        };
    }
    if let Some(service) = dns_route.service.as_deref() {
        return first_effective_http_host_port_for_service(policy, service);
    }
    if !policy.ports_declared_explicitly {
        return first_effective_http_host_port(policy);
    }
    let Some(raw) = policy.declared_ports.first() else {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` declares gateway DNS routes but no `host.ports`; declare an explicit host HTTP port before enabling gateway registration",
            policy.name
        )));
    };
    let host = raw.split(':').next().unwrap_or_default().trim();
    host.parse::<u16>().map_err(|error| {
        RunnerError::task_invocation(format!(
            "container `{}` has invalid host port mapping `{raw}` for gateway registration: {error}",
            policy.name
        ))
    })
}

fn selected_declared_host_port(
    policy: &EffectiveContainerPolicy,
    selected_port: u16,
) -> Result<u16, RunnerError> {
    if policy
        .declared_ports
        .iter()
        .any(|raw| parse_host_port(policy, raw).ok() == Some(selected_port))
    {
        return Ok(selected_port);
    }
    Err(RunnerError::task_invocation(format!(
            "container `{}` declares a gateway DNS route on host port {selected_port} but `host.ports` does not expose that host port",
            policy.name
        )))
}

fn selected_effective_container_port(
    policy: &EffectiveContainerPolicy,
    selected_port: u16,
) -> Result<u16, RunnerError> {
    for raw in &policy.declared_ports {
        let binding = parse_port_binding(policy, raw)?;
        if binding.1 == selected_port {
            return Ok(binding.0);
        }
    }
    Err(RunnerError::task_invocation(format!(
            "container `{}` declares a gateway DNS route for container port {selected_port} but the generated compose does not expose that container port",
            policy.name
        )))
}

fn first_effective_http_host_port(policy: &EffectiveContainerPolicy) -> Result<u16, RunnerError> {
    let mut first_binding: Option<u16> = None;
    for raw in &policy.declared_ports {
        let (host, container) = parse_port_binding(policy, raw)?;
        if first_binding.is_none() {
            first_binding = Some(host);
        }
        if matches!(container, 80 | 3000 | 8025 | 9000 | 9001 | 9200) {
            return Ok(host);
        }
    }
    first_binding.ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "container `{}` declares gateway DNS routes but no effective published ports are available for gateway registration",
            policy.name
        ))
    })
}

fn selected_effective_container_port_for_service(
    policy: &EffectiveContainerPolicy,
    service: &str,
    selected_port: u16,
) -> Result<u16, RunnerError> {
    let bindings = service_port_bindings(policy, service)?;
    if bindings.is_empty() {
        return selected_effective_container_port(policy, selected_port);
    }
    for (host, container) in bindings {
        if container == selected_port {
            return Ok(host);
        }
    }
    Err(RunnerError::task_invocation(format!(
        "container `{}` declares a gateway DNS route for service `{service}` on container port {selected_port} but the generated compose does not expose that port for the selected service",
        policy.name
    )))
}

fn first_effective_http_host_port_for_service(
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<u16, RunnerError> {
    let bindings = service_port_bindings(policy, service)?;
    if bindings.is_empty() {
        return first_effective_http_host_port(policy);
    }
    let mut first_binding: Option<u16> = None;
    for (host, container) in bindings {
        if first_binding.is_none() {
            first_binding = Some(host);
        }
        if matches!(container, 80 | 3000 | 8025 | 9000 | 9001 | 9200) {
            return Ok(host);
        }
    }
    first_binding.ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "container `{}` declares a gateway DNS route for service `{service}` but no effective published ports are available for that service",
            policy.name
        ))
    })
}

fn service_port_bindings(
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<Vec<(u16, u16)>, RunnerError> {
    for compose_file in &policy.compose_files {
        let content = match std::fs::read_to_string(compose_file) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let parsed: YamlValue = match serde_yaml::from_str(&content) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        let Some(services) = parsed
            .as_mapping()
            .and_then(|root| root.get(YamlValue::String("services".to_owned())))
            .and_then(YamlValue::as_mapping)
        else {
            continue;
        };
        let Some(service_entry) = services
            .get(YamlValue::String(service.to_owned()))
            .and_then(YamlValue::as_mapping)
        else {
            continue;
        };
        let Some(ports) = service_entry
            .get(YamlValue::String("ports".to_owned()))
            .and_then(YamlValue::as_sequence)
        else {
            return Ok(Vec::new());
        };
        let mut bindings = Vec::new();
        for entry in ports {
            let Some(raw) = entry.as_str() else {
                continue;
            };
            bindings.push(parse_port_binding(policy, raw)?);
        }
        return Ok(bindings);
    }
    Ok(Vec::new())
}

fn parse_host_port(policy: &EffectiveContainerPolicy, raw: &str) -> Result<u16, RunnerError> {
    parse_port_binding(policy, raw).map(|binding| binding.0)
}

fn parse_port_binding(
    policy: &EffectiveContainerPolicy,
    raw: &str,
) -> Result<(u16, u16), RunnerError> {
    let mut parts = raw.split(':');
    let host = parts.next().unwrap_or_default().trim();
    let container = parts.next().unwrap_or_default().trim();
    if host.is_empty() || container.is_empty() || parts.next().is_some() {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` has invalid host port mapping `{raw}` for gateway registration",
            policy.name
        )));
    }
    let host_port = host.parse::<u16>().map_err(|error| {
        RunnerError::task_invocation(format!(
            "container `{}` has invalid host port mapping `{raw}` for gateway registration: {error}",
            policy.name
        ))
    })?;
    let container_port = container.parse::<u16>().map_err(|error| {
        RunnerError::task_invocation(format!(
            "container `{}` has invalid container port mapping `{raw}` for gateway registration: {error}",
            policy.name
        ))
    })?;
    Ok((host_port, container_port))
}

fn gateway_route_table_path() -> Result<PathBuf, RunnerError> {
    Ok(gateway_dir()?.join("routes.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_containers::EffectiveComposeSource;
    use effigy_gateway::routes::RouteTable;
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("/tmp/docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo-web-dev".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: Some("clientname.test".to_owned()),
            dns_tls: true,
            dns_port: None,
            dns_routes: vec![effigy_containers::EffectiveDnsRoute {
                domain: "clientname.test".to_owned(),
                tls: true,
                port: None,
                service: None,
            }],
            declared_ports: vec!["8080:80".to_owned()],
            ports_declared_explicitly: true,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
        }
    }

    #[test]
    fn resolves_gateway_route_from_first_declared_host_port() {
        let routes = resolve_gateway_routes(&test_policy()).expect("routes");
        let route = routes.first().expect("some route");
        assert_eq!(route.domain, "clientname.test");
        assert_eq!(route.target, "127.0.0.1:8080");
        assert!(route.tls);
        assert_eq!(route.service, None);
    }

    #[test]
    fn skips_gateway_route_when_dns_is_not_configured() {
        let mut policy = test_policy();
        policy.dns_domain = None;
        policy.dns_routes.clear();
        assert!(resolve_gateway_routes(&policy).expect("routes").is_empty());
    }

    #[test]
    fn errors_when_dns_is_configured_without_host_ports() {
        let mut policy = test_policy();
        policy.declared_ports.clear();
        let error = resolve_gateway_routes(&policy).expect_err("should fail");
        assert!(error.to_string().contains("no `host.ports`"));
    }

    #[test]
    fn uses_explicit_dns_port_when_present() {
        let mut policy = test_policy();
        policy.dns_port = Some(9001);
        policy.dns_routes[0].port = Some(9001);
        policy.declared_ports = vec!["5432:5432".to_owned(), "9001:9001".to_owned()];

        let routes = resolve_gateway_routes(&policy).expect("routes");
        let route = routes.first().expect("some route");
        assert_eq!(route.target, "127.0.0.1:9001");
    }

    #[test]
    fn errors_when_explicit_dns_port_is_not_declared() {
        let mut policy = test_policy();
        policy.dns_port = Some(9001);
        policy.dns_routes[0].port = Some(9001);

        let error = resolve_gateway_routes(&policy).expect_err("should fail");
        assert!(error.to_string().contains("host port 9001"));
    }

    #[test]
    fn uses_effective_container_port_when_ports_are_auto_allocated() {
        let mut policy = test_policy();
        policy.ports_declared_explicitly = false;
        policy.dns_port = Some(8025);
        policy.dns_routes[0].port = Some(8025);
        policy.declared_ports = vec!["8126:1025".to_owned(), "8125:8025".to_owned()];

        let routes = resolve_gateway_routes(&policy).expect("routes");
        let route = routes.first().expect("some route");
        assert_eq!(route.target, "127.0.0.1:8125");
    }

    #[test]
    fn uses_service_specific_effective_port_when_multiple_services_publish_same_container_port() {
        let dir = std::env::temp_dir().join(format!(
            "effigy-gateway-service-route-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir tempdir");
        let compose_file = dir.join("docker-compose.yml");
        std::fs::write(
            &compose_file,
            r#"
services:
  pma:
    ports:
      - "18900:80"
  web:
    ports:
      - "18901:80"
"#,
        )
        .expect("write compose");

        let mut policy = test_policy();
        policy.compose_source = EffectiveComposeSource::Generated;
        policy.compose_files = vec![compose_file];
        policy.ports_declared_explicitly = false;
        policy.declared_ports = vec!["18900:80".to_owned(), "18901:80".to_owned()];
        policy.dns_routes[0].service = Some("web".to_owned());

        let routes = resolve_gateway_routes(&policy).expect("routes");
        let route = routes.first().expect("some route");
        assert_eq!(route.target, "127.0.0.1:18901");
    }

    #[test]
    fn register_and_deregister_gateway_route_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "effigy-gateway-registration-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir tempdir");
        let route_table_path = dir.join("routes.json");
        let repo_root = dir.join("repo");
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        let routes = resolve_gateway_routes(&test_policy()).expect("routes");
        let route = routes.first().expect("some route");

        register_gateway_route_at(&route_table_path, &repo_root, route).expect("register");
        let table = RouteTable::load(&route_table_path).expect("load registered route table");
        let registered = table.lookup("clientname.test").expect("registered route");
        assert_eq!(registered.target, "127.0.0.1:8080");
        assert!(registered.tls);

        deregister_gateway_route_at(&route_table_path, "clientname.test").expect("deregister");
        let table = RouteTable::load(&route_table_path).expect("load deregistered route table");
        assert!(table.lookup("clientname.test").is_none());
    }

    #[test]
    fn resolves_multiple_gateway_routes_for_one_container() {
        let mut policy = test_policy();
        policy
            .dns_routes
            .push(effigy_containers::EffectiveDnsRoute {
                domain: "admin.clientname.test".to_owned(),
                tls: false,
                port: Some(9001),
                service: Some("admin".to_owned()),
            });
        policy.declared_ports = vec!["8080:80".to_owned(), "9001:9001".to_owned()];

        let routes = resolve_gateway_routes(&policy).expect("routes");
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].domain, "clientname.test");
        assert_eq!(routes[0].target, "127.0.0.1:8080");
        assert_eq!(routes[1].domain, "admin.clientname.test");
        assert_eq!(routes[1].target, "127.0.0.1:9001");
        assert_eq!(routes[1].service.as_deref(), Some("admin"));
    }

    #[test]
    fn validates_gateway_route_against_matching_runtime_port() {
        let policy = test_policy();
        let repo_root = PathBuf::from("/tmp/repo");
        let routes = resolve_gateway_routes(&policy).expect("routes");
        let rows = vec![RunningComposeContainer {
            container_name: "demo-web-dev-app-1".to_owned(),
            status: "Up 10 seconds".to_owned(),
            ports: vec![
                "0.0.0.0:8080->80/tcp".to_owned(),
                ":::8080->80/tcp".to_owned(),
            ],
            project_name: Some("demo-web-dev".to_owned()),
            working_dir: Some("/tmp/repo".to_owned()),
            service: Some("app".to_owned()),
        }];

        validate_gateway_routes_against_rows(&repo_root, &policy, &routes, &rows)
            .expect("matching published port should validate");
    }

    #[test]
    fn validates_gateway_route_against_matching_runtime_service_when_declared() {
        let mut policy = test_policy();
        policy.dns_routes[0].service = Some("app".to_owned());
        let repo_root = PathBuf::from("/tmp/repo");
        let routes = resolve_gateway_routes(&policy).expect("routes");
        let rows = vec![RunningComposeContainer {
            container_name: "demo-web-dev-app-1".to_owned(),
            status: "Up 10 seconds".to_owned(),
            ports: vec!["0.0.0.0:8080->80/tcp".to_owned()],
            project_name: Some("demo-web-dev".to_owned()),
            working_dir: Some("/tmp/repo".to_owned()),
            service: Some("app".to_owned()),
        }];

        validate_gateway_routes_against_rows(&repo_root, &policy, &routes, &rows)
            .expect("matching service should validate");
    }

    #[test]
    fn rejects_gateway_route_when_declared_service_does_not_match_runtime_service() {
        let mut policy = test_policy();
        policy.dns_routes[0].service = Some("admin".to_owned());
        let repo_root = PathBuf::from("/tmp/repo");
        let routes = resolve_gateway_routes(&policy).expect("routes");
        let rows = vec![RunningComposeContainer {
            container_name: "demo-web-dev-app-1".to_owned(),
            status: "Up 10 seconds".to_owned(),
            ports: vec!["0.0.0.0:8080->80/tcp".to_owned()],
            project_name: Some("demo-web-dev".to_owned()),
            working_dir: Some("/tmp/repo".to_owned()),
            service: Some("app".to_owned()),
        }];

        let error = validate_gateway_routes_against_rows(&repo_root, &policy, &routes, &rows)
            .expect_err("mismatched service should fail");
        assert!(
            error
                .to_string()
                .contains("project `demo-web-dev` service `admin` publishes host port 8080"),
            "got: {error}"
        );
    }

    #[test]
    fn rejects_gateway_route_when_runtime_does_not_publish_selected_port() {
        let policy = test_policy();
        let repo_root = PathBuf::from("/tmp/repo");
        let routes = resolve_gateway_routes(&policy).expect("routes");
        let rows = vec![RunningComposeContainer {
            container_name: "demo-web-dev-app-1".to_owned(),
            status: "Up 10 seconds".to_owned(),
            ports: vec!["0.0.0.0:9090->80/tcp".to_owned()],
            project_name: Some("demo-web-dev".to_owned()),
            working_dir: Some("/tmp/repo".to_owned()),
            service: Some("app".to_owned()),
        }];

        let error = validate_gateway_routes_against_rows(&repo_root, &policy, &routes, &rows)
            .expect_err("mismatched published port should fail");
        assert!(
            error
                .to_string()
                .contains("gateway registration refuses to target an unrelated runtime binding"),
            "got: {error}"
        );
    }

    #[test]
    fn parse_published_host_port_supports_ipv6_and_ipv4_bindings() {
        assert_eq!(
            parse_published_host_port_range("0.0.0.0:8080->80/tcp").expect("ipv4 host port"),
            (8080, 8080)
        );
        assert_eq!(
            parse_published_host_port_range(":::8080->80/tcp").expect("ipv6 host port"),
            (8080, 8080)
        );
        assert_eq!(
            parse_published_host_port_range("0.0.0.0:41001-41003->41001-41003/tcp")
                .expect("host port range"),
            (41001, 41003)
        );
    }

    #[test]
    fn listener_command_runtime_detection_is_narrow() {
        assert!(listener_command_looks_runtime_managed("docker-proxy"));
        assert!(listener_command_looks_runtime_managed("colima"));
        assert!(!listener_command_looks_runtime_managed("Python"));
    }
}
