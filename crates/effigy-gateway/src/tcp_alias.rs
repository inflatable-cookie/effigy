use std::collections::{BTreeMap, HashMap};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tracing::{debug, error, info};

use crate::error::GatewayError;
use crate::routes::RouteTable;

const RECONCILE_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesiredTcpAlias {
    domain: String,
    bind_ip: Ipv4Addr,
    bind_port: u16,
    upstream: SocketAddr,
}

#[derive(Debug)]
struct ActiveTcpAlias {
    shutdown: watch::Sender<bool>,
    handle: tokio::task::JoinHandle<()>,
    desired: DesiredTcpAlias,
}

pub async fn run_tcp_alias_manager(
    route_table: Arc<RwLock<RouteTable>>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), GatewayError> {
    let mut active = HashMap::<String, ActiveTcpAlias>::new();

    loop {
        reconcile_tcp_aliases(&route_table, &mut active);

        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = tokio::time::sleep(RECONCILE_INTERVAL) => {}
        }
    }

    for alias in active.into_values() {
        let _ = alias.shutdown.send(true);
        alias.handle.abort();
    }

    info!("TCP alias manager stopped");
    Ok(())
}

fn reconcile_tcp_aliases(
    route_table: &Arc<RwLock<RouteTable>>,
    active: &mut HashMap<String, ActiveTcpAlias>,
) {
    let desired = desired_tcp_aliases(&route_table.read().expect("route table lock poisoned"));

    let stale_domains = active
        .iter()
        .filter_map(|(domain, alias)| {
            desired
                .get(domain)
                .filter(|candidate| **candidate == alias.desired)
                .is_none()
                .then_some(domain.clone())
        })
        .collect::<Vec<_>>();
    for domain in stale_domains {
        if let Some(alias) = active.remove(&domain) {
            let _ = alias.shutdown.send(true);
            alias.handle.abort();
        }
    }

    for (domain, desired_alias) in desired {
        let restart = active
            .get(&domain)
            .is_some_and(|alias| alias.handle.is_finished());
        if active.contains_key(&domain) && !restart {
            continue;
        }
        if let Some(alias) = active.remove(&domain) {
            let _ = alias.shutdown.send(true);
            alias.handle.abort();
        }
        let (alias_shutdown_tx, alias_shutdown_rx) = watch::channel(false);
        let desired_clone = desired_alias.clone();
        let domain_for_task = domain.clone();
        let handle = tokio::spawn(async move {
            if let Err(error) = run_single_tcp_alias(desired_clone, alias_shutdown_rx).await {
                error!(error = %error, domain = %domain_for_task, "TCP alias listener failed");
            }
        });
        active.insert(
            domain,
            ActiveTcpAlias {
                shutdown: alias_shutdown_tx,
                handle,
                desired: desired_alias,
            },
        );
    }
}

fn desired_tcp_aliases(route_table: &RouteTable) -> BTreeMap<String, DesiredTcpAlias> {
    route_table
        .all_routes()
        .into_iter()
        .filter_map(|route| {
            let domain = route.domain.clone();
            Some((
                domain.clone(),
                DesiredTcpAlias {
                    domain,
                    bind_ip: route.dns_ip?,
                    bind_port: route.tcp_port?,
                    upstream: parse_tcp_target(route.tcp_target.as_deref()?).ok()?,
                },
            ))
        })
        .collect::<BTreeMap<_, _>>()
}

fn parse_tcp_target(raw: &str) -> Result<SocketAddr, GatewayError> {
    raw.parse::<SocketAddr>()
        .map_err(|error| GatewayError::TcpAliasTargetInvalid {
            target: raw.to_owned(),
            reason: error.to_string(),
        })
}

async fn run_single_tcp_alias(
    desired: DesiredTcpAlias,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), GatewayError> {
    let bind_addr = SocketAddr::V4(SocketAddrV4::new(desired.bind_ip, desired.bind_port));
    let listener =
        TcpListener::bind(bind_addr)
            .await
            .map_err(|error| GatewayError::TcpAliasBindError {
                addr: bind_addr.to_string(),
                reason: error.to_string(),
            })?;
    info!(
        domain = %desired.domain,
        bind = %bind_addr,
        upstream = %desired.upstream,
        "TCP alias listener started"
    );

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _peer)) => {
                        let upstream = desired.upstream;
                        let domain = desired.domain.clone();
                        tokio::spawn(async move {
                            if let Err(error) = proxy_tcp_alias_connection(stream, upstream).await {
                                debug!(
                                    error = %error,
                                    domain = %domain,
                                    upstream = %upstream,
                                    "TCP alias connection failed"
                                );
                            }
                        });
                    }
                    Err(error) => {
                        return Err(GatewayError::TcpAliasBindError {
                            addr: bind_addr.to_string(),
                            reason: error.to_string(),
                        });
                    }
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn proxy_tcp_alias_connection(
    inbound: TcpStream,
    upstream: SocketAddr,
) -> Result<(), GatewayError> {
    let mut inbound = inbound;
    let mut outbound =
        TcpStream::connect(upstream)
            .await
            .map_err(|error| GatewayError::TcpAliasConnectError {
                target: upstream.to_string(),
                reason: error.to_string(),
            })?;
    copy_bidirectional(&mut inbound, &mut outbound)
        .await
        .map_err(|error| GatewayError::TcpAliasIoError {
            target: upstream.to_string(),
            reason: error.to_string(),
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::{Route, RouteSource, RouteTable};
    use chrono::Utc;

    #[test]
    fn extracts_desired_tcp_aliases_from_route_table() {
        let mut table = RouteTable::new();
        table.upsert(Route {
            domain: "db.app.test".to_owned(),
            target: None,
            dns_ip: Some(Ipv4Addr::new(127, 1, 0, 7)),
            tcp_port: Some(5432),
            tcp_target: Some("127.0.0.1:19432".to_owned()),
            source: RouteSource::Container,
            project: "/tmp/app".to_owned(),
            tls: false,
            registered: Utc::now(),
        });

        let desired = desired_tcp_aliases(&table);
        let alias = desired.get("db.app.test").expect("tcp alias");
        assert_eq!(alias.bind_ip, Ipv4Addr::new(127, 1, 0, 7));
        assert_eq!(alias.bind_port, 5432);
        assert_eq!(alias.upstream, "127.0.0.1:19432".parse().unwrap());
    }
}
