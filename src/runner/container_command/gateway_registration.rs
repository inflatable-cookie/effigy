use std::path::{Path, PathBuf};

use effigy_containers::EffectiveContainerPolicy;
use effigy_gateway::registration::{deregister_route, register_route, RouteRegistration};

use crate::runner::error::RunnerError;
use crate::runner::gateway_command::{
    ensure_gateway_tls_cert, gateway_dir, remove_gateway_tls_cert,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct RegisteredGatewayRoute {
    pub(in crate::runner) domain: String,
    pub(in crate::runner) target: String,
    pub(in crate::runner) tls: bool,
}

pub(in crate::runner) fn register_gateway_route_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<RegisteredGatewayRoute>, RunnerError> {
    let Some(route) = resolve_gateway_route(policy)? else {
        return Ok(None);
    };
    if route.tls {
        ensure_gateway_tls_cert(&route.domain)?;
    }
    let route_table_path = gateway_route_table_path()?;
    register_gateway_route_at(&route_table_path, repo_root, &route)?;
    Ok(Some(route))
}

pub(in crate::runner) fn deregister_gateway_route_for_container(
    policy: &EffectiveContainerPolicy,
) -> Result<Option<String>, RunnerError> {
    let Some(domain) = policy.dns_domain.as_deref() else {
        return Ok(None);
    };
    if policy.dns_tls {
        remove_gateway_tls_cert(domain)?;
    }
    let route_table_path = gateway_route_table_path()?;
    deregister_gateway_route_at(&route_table_path, domain)?;
    Ok(Some(domain.to_owned()))
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

fn resolve_gateway_route(
    policy: &EffectiveContainerPolicy,
) -> Result<Option<RegisteredGatewayRoute>, RunnerError> {
    let Some(domain) = policy.dns_domain.as_deref() else {
        return Ok(None);
    };
    let host_port = first_declared_host_port(policy)?;
    Ok(Some(RegisteredGatewayRoute {
        domain: domain.to_owned(),
        target: format!("127.0.0.1:{host_port}"),
        tls: policy.dns_tls,
    }))
}

fn first_declared_host_port(policy: &EffectiveContainerPolicy) -> Result<u16, RunnerError> {
    if let Some(port) = policy.dns_port {
        return selected_declared_host_port(policy, port);
    }
    let Some(raw) = policy.declared_ports.first() else {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` declares `[containers.{}.dns]` but no `host.ports`; declare an explicit host HTTP port before enabling gateway registration",
            policy.name, policy.name
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
        "container `{}` declares `[containers.{}.dns].port = {selected_port}` but `host.ports` does not expose that host port",
        policy.name, policy.name
    )))
}

fn parse_host_port(policy: &EffectiveContainerPolicy, raw: &str) -> Result<u16, RunnerError> {
    let host = raw.split(':').next().unwrap_or_default().trim();
    host.parse::<u16>().map_err(|error| {
        RunnerError::task_invocation(format!(
            "container `{}` has invalid host port mapping `{raw}` for gateway registration: {error}",
            policy.name
        ))
    })
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
            profile: "default".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("/tmp/docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            project_name: "demo-web-dev".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: Some("clientname.test".to_owned()),
            dns_tls: true,
            dns_port: None,
            declared_ports: vec!["8080:80".to_owned()],
            declared_mounts: vec![],
            health_check: None,
            health_timeout_secs: 60,
            ui_tabs: vec![],
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
        }
    }

    #[test]
    fn resolves_gateway_route_from_first_declared_host_port() {
        let route = resolve_gateway_route(&test_policy())
            .expect("route")
            .expect("some route");
        assert_eq!(route.domain, "clientname.test");
        assert_eq!(route.target, "127.0.0.1:8080");
        assert!(route.tls);
    }

    #[test]
    fn skips_gateway_route_when_dns_is_not_configured() {
        let mut policy = test_policy();
        policy.dns_domain = None;
        assert_eq!(resolve_gateway_route(&policy).expect("route"), None);
    }

    #[test]
    fn errors_when_dns_is_configured_without_host_ports() {
        let mut policy = test_policy();
        policy.declared_ports.clear();
        let error = resolve_gateway_route(&policy).expect_err("should fail");
        assert!(error.to_string().contains("no `host.ports`"));
    }

    #[test]
    fn uses_explicit_dns_port_when_present() {
        let mut policy = test_policy();
        policy.dns_port = Some(9001);
        policy.declared_ports = vec!["5432:5432".to_owned(), "9001:9001".to_owned()];

        let route = resolve_gateway_route(&policy)
            .expect("route")
            .expect("some route");
        assert_eq!(route.target, "127.0.0.1:9001");
    }

    #[test]
    fn errors_when_explicit_dns_port_is_not_declared() {
        let mut policy = test_policy();
        policy.dns_port = Some(9001);

        let error = resolve_gateway_route(&policy).expect_err("should fail");
        assert!(error.to_string().contains("dns].port = 9001"));
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
        let route = resolve_gateway_route(&test_policy())
            .expect("route")
            .expect("some route");

        register_gateway_route_at(&route_table_path, &repo_root, &route).expect("register");
        let table = RouteTable::load(&route_table_path).expect("load registered route table");
        let registered = table.lookup("clientname.test").expect("registered route");
        assert_eq!(registered.target, "127.0.0.1:8080");
        assert!(registered.tls);

        deregister_gateway_route_at(&route_table_path, "clientname.test").expect("deregister");
        let table = RouteTable::load(&route_table_path).expect("load deregistered route table");
        assert!(table.lookup("clientname.test").is_none());
    }
}
