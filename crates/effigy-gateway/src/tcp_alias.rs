use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::error::GatewayError;
use crate::routes::RouteTable;

const RECONCILE_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesiredTcpAlias {
    domains: Vec<String>,
    bind_ip: Ipv4Addr,
    bind_port: u16,
    upstream: SocketAddr,
}

struct ActiveTcpAlias {
    handle: tokio::task::JoinHandle<()>,
    desired: DesiredTcpAlias,
}

impl ActiveTcpAlias {
    async fn stop(self) {
        self.handle.abort();
        let _ = self.handle.await;
    }
}

#[derive(Clone, Copy)]
enum LoopbackAction {
    Add,
    Remove,
}

type ChangeLoopback = Arc<dyn Fn(LoopbackAction, Ipv4Addr) -> io::Result<()> + Send + Sync>;

#[derive(Default)]
struct LoopbackAliases {
    managed_loopback_ips: BTreeSet<Ipv4Addr>,
    change: Option<ChangeLoopback>,
}

impl LoopbackAliases {
    fn standard() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            change: Some(Arc::new(|action, ip| match action {
                LoopbackAction::Add => crate::bind_broker::ensure_loopback_alias(ip),
                LoopbackAction::Remove => crate::bind_broker::remove_loopback_alias(ip),
            })),
            ..Self::default()
        }
    }

    async fn ensure(&mut self, ip: Ipv4Addr) -> io::Result<()> {
        let Some(change) = &self.change else {
            return Ok(());
        };
        if crate::loopback::in_pool(ip) && !self.managed_loopback_ips.contains(&ip) {
            let change = Arc::clone(change);
            tokio::task::spawn_blocking(move || change(LoopbackAction::Add, ip)).await??;
            self.managed_loopback_ips.insert(ip);
        }
        Ok(())
    }

    async fn remove_unused(&mut self, active: &HashMap<String, ActiveTcpAlias>) {
        let Some(change) = &self.change else {
            return;
        };
        let active_ips: BTreeSet<_> = active.values().map(|alias| alias.desired.bind_ip).collect();
        for ip in std::mem::take(&mut self.managed_loopback_ips) {
            if active_ips.contains(&ip) {
                self.managed_loopback_ips.insert(ip);
                continue;
            }
            let change = Arc::clone(change);
            let result = tokio::task::spawn_blocking(move || change(LoopbackAction::Remove, ip))
                .await
                .map_err(io::Error::other)
                .and_then(|result| result);
            if let Err(error) = result {
                warn!(%ip, %error, "could not remove TCP alias loopback address");
            }
        }
    }
}

/// Run TCP aliases for a private gateway without changing the host interface.
pub async fn run_tcp_alias_manager(
    route_table: Arc<RwLock<RouteTable>>,
    shutdown: watch::Receiver<bool>,
) -> Result<(), GatewayError> {
    run_manager(
        route_table,
        shutdown,
        HashMap::new(),
        LoopbackAliases::default(),
    )
    .await
}

pub(crate) async fn run_tcp_alias_manager_ready(
    route_table: Arc<RwLock<RouteTable>>,
    shutdown: watch::Receiver<bool>,
    ready: tokio::sync::oneshot::Sender<Result<(), String>>,
) -> Result<(), GatewayError> {
    let mut loopback = LoopbackAliases::standard();
    let desired = desired_tcp_aliases(&crate::locks::read_tolerant(&route_table));
    let prepared = async {
        let mut listeners = Vec::new();
        for (key, alias) in desired {
            if *shutdown.borrow() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "TCP alias startup stopped",
                )
                .into());
            }
            let listener = prepare_tcp_alias(&alias, &mut loopback, TcpListener::bind).await?;
            listeners.push((key, alias, listener));
        }
        Ok::<_, GatewayError>(listeners)
    }
    .await;
    let listeners = match prepared {
        Ok(listeners) => listeners,
        Err(error) => {
            // The prepared listeners have already been dropped.
            let _ = ready.send(Err(error.to_string()));
            loopback.remove_unused(&HashMap::new()).await;
            return Err(error);
        }
    };
    let active = listeners
        .into_iter()
        .map(|(key, alias, listener)| (key, start_tcp_alias(listener, alias)))
        .collect();
    let _ = ready.send(Ok(()));
    run_manager(route_table, shutdown, active, loopback).await
}

async fn run_manager(
    route_table: Arc<RwLock<RouteTable>>,
    mut shutdown: watch::Receiver<bool>,
    mut active: HashMap<String, ActiveTcpAlias>,
    mut loopback: LoopbackAliases,
) -> Result<(), GatewayError> {
    while !*shutdown.borrow() {
        let desired = desired_tcp_aliases(&crate::locks::read_tolerant(&route_table));
        reconcile_tcp_aliases(
            desired,
            &mut active,
            &mut loopback,
            &shutdown,
            TcpListener::bind,
        )
        .await;

        tokio::select! {
            result = shutdown.changed() => {
                if result.is_err() {
                    break;
                }
            }
            _ = tokio::time::sleep(RECONCILE_INTERVAL) => {}
        }
    }

    for (_, alias) in active.drain() {
        alias.stop().await;
    }
    loopback.remove_unused(&active).await;

    info!("TCP alias manager stopped");
    Ok(())
}

async fn reconcile_tcp_aliases<F: Future<Output = io::Result<TcpListener>>>(
    desired: BTreeMap<String, DesiredTcpAlias>,
    active: &mut HashMap<String, ActiveTcpAlias>,
    loopback: &mut LoopbackAliases,
    shutdown: &watch::Receiver<bool>,
    bind: impl Fn(SocketAddr) -> F,
) {
    let stale_keys = active
        .iter()
        .filter_map(|(key, alias)| {
            (alias.handle.is_finished() || desired.get(key) != Some(&alias.desired))
                .then_some(key.clone())
        })
        .collect::<Vec<_>>();
    for key in stale_keys {
        if let Some(alias) = active.remove(&key) {
            alias.stop().await;
        }
    }

    for (key, alias) in desired {
        if *shutdown.borrow() {
            break;
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = active.entry(key) {
            match prepare_tcp_alias(&alias, loopback, &bind).await {
                Ok(listener) => {
                    entry.insert(start_tcp_alias(listener, alias));
                }
                Err(error) => {
                    error!(%error, "TCP alias listener failed");
                }
            }
        }
    }
    loopback.remove_unused(active).await;
}

async fn prepare_tcp_alias<F: Future<Output = io::Result<TcpListener>>>(
    alias: &DesiredTcpAlias,
    loopback: &mut LoopbackAliases,
    bind: impl FnOnce(SocketAddr) -> F,
) -> Result<TcpListener, GatewayError> {
    let addr = SocketAddr::V4(SocketAddrV4::new(alias.bind_ip, alias.bind_port));
    let result = match loopback.ensure(alias.bind_ip).await {
        Ok(()) => bind(addr).await,
        Err(error) => Err(error),
    };
    result.map_err(|error| GatewayError::TcpAliasBindError {
        addr: addr.to_string(),
        reason: error.to_string(),
    })
}

fn start_tcp_alias(listener: TcpListener, desired: DesiredTcpAlias) -> ActiveTcpAlias {
    let alias = desired.clone();
    let handle = tokio::spawn(async move {
        if let Err(error) = run_single_tcp_alias_on(listener, alias).await {
            error!(%error, "TCP alias listener failed");
        }
    });
    ActiveTcpAlias { handle, desired }
}

fn desired_tcp_aliases(route_table: &RouteTable) -> BTreeMap<String, DesiredTcpAlias> {
    let mut desired = BTreeMap::<String, DesiredTcpAlias>::new();
    for route in route_table.all_routes() {
        let Some(bind_ip) = route.dns_ip else {
            continue;
        };
        let Some(bind_port) = route.tcp_port else {
            continue;
        };
        let Some(raw_target) = route.tcp_target.as_deref() else {
            continue;
        };
        let Ok(upstream) = parse_tcp_target(raw_target) else {
            continue;
        };
        let key = format!("{bind_ip}:{bind_port}");
        match desired.entry(key) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(DesiredTcpAlias {
                    domains: vec![route.domain.clone()],
                    bind_ip,
                    bind_port,
                    upstream,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut existing) => {
                if existing.get().upstream == upstream {
                    existing.get_mut().domains.push(route.domain.clone());
                } else {
                    error!(
                        bind = %format!("{bind_ip}:{bind_port}"),
                        existing_domains = ?existing.get().domains,
                        existing_upstream = %existing.get().upstream,
                        conflicting_domain = %route.domain,
                        conflicting_upstream = %upstream,
                        "ignoring conflicting TCP alias route with duplicate bind tuple"
                    );
                }
            }
        }
    }
    desired
}

fn parse_tcp_target(raw: &str) -> Result<SocketAddr, GatewayError> {
    raw.parse::<SocketAddr>()
        .map_err(|error| GatewayError::TcpAliasTargetInvalid {
            target: raw.to_owned(),
            reason: error.to_string(),
        })
}

async fn run_single_tcp_alias_on(
    listener: TcpListener,
    desired: DesiredTcpAlias,
) -> Result<(), GatewayError> {
    let bind_addr = listener.local_addr()?;
    info!(
        domains = ?desired.domains,
        bind = %bind_addr,
        upstream = %desired.upstream,
        "TCP alias listener started"
    );

    loop {
        let (stream, _peer) =
            listener
                .accept()
                .await
                .map_err(|error| GatewayError::TcpAliasBindError {
                    addr: bind_addr.to_string(),
                    reason: error.to_string(),
                })?;
        let upstream = desired.upstream;
        let domains = desired.domains.clone();
        tokio::spawn(async move {
            if let Err(error) = proxy_tcp_alias_connection(stream, upstream).await {
                debug!(
                    error = %error,
                    domains = ?domains,
                    upstream = %upstream,
                    "TCP alias connection failed"
                );
            }
        });
    }
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

    #[tokio::test]
    async fn loopback_aliases_follow_bound_ports_and_close_before_removal() {
        use std::sync::Mutex;

        #[derive(Debug, PartialEq, Eq)]
        enum Event {
            Added(Ipv4Addr),
            Bound(SocketAddr),
            Removed(Ipv4Addr),
        }

        let events = Arc::new(Mutex::new(Vec::new()));
        let reservations = Arc::new(Mutex::new(Vec::<(Ipv4Addr, SocketAddr)>::new()));
        let mut loopback = LoopbackAliases {
            change: Some(Arc::new({
                let events = Arc::clone(&events);
                let reservations = Arc::clone(&reservations);
                move |action, ip| {
                    let event = match action {
                        LoopbackAction::Add => Event::Added(ip),
                        LoopbackAction::Remove => {
                            for (_, addr) in reservations
                                .lock()
                                .unwrap()
                                .iter()
                                .filter(|(owner, _)| *owner == ip)
                            {
                                std::net::TcpListener::bind(addr)
                                    .expect("listener must close before alias removal");
                            }
                            Event::Removed(ip)
                        }
                    };
                    events.lock().unwrap().push(event);
                    Ok(())
                }
            })),
            ..LoopbackAliases::default()
        };
        let bind = |addr: SocketAddr| {
            let reservations = &reservations;
            let events = &events;
            async move {
                let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
                let SocketAddr::V4(requested) = addr else {
                    panic!("fixture expected an IPv4 endpoint");
                };
                reservations
                    .lock()
                    .unwrap()
                    .push((*requested.ip(), listener.local_addr()?));
                events.lock().unwrap().push(Event::Bound(addr));
                Ok(listener)
            }
        };
        let first_ip = crate::loopback::DEFAULT_LOOPBACK_START;
        let second_ip = crate::loopback::DEFAULT_LOOPBACK_END;
        let first = SocketAddrV4::new(first_ip, 5432);
        let second = SocketAddrV4::new(first_ip, 5433);
        let third = SocketAddrV4::new(second_ip, 5432);
        let desired = |addresses: &[SocketAddrV4]| {
            addresses
                .iter()
                .map(|addr| (addr.to_string(), desired_alias(*addr)))
                .collect()
        };
        let mut active = HashMap::new();
        let (stop, shutdown) = watch::channel(false);
        reconcile_tcp_aliases(
            desired(&[first, second, third]),
            &mut active,
            &mut loopback,
            &shutdown,
            bind,
        )
        .await;
        assert_eq!(
            *events.lock().unwrap(),
            [
                Event::Added(first_ip),
                Event::Bound(first.into()),
                Event::Bound(second.into()),
                Event::Added(second_ip),
                Event::Bound(third.into()),
            ]
        );
        events.lock().unwrap().clear();

        reconcile_tcp_aliases(
            desired(&[second, third]),
            &mut active,
            &mut loopback,
            &shutdown,
            bind,
        )
        .await;
        assert!(events.lock().unwrap().is_empty());
        reconcile_tcp_aliases(
            desired(&[third]),
            &mut active,
            &mut loopback,
            &shutdown,
            bind,
        )
        .await;
        assert_eq!(*events.lock().unwrap(), [Event::Removed(first_ip)]);

        stop.send(true).unwrap();
        run_manager(
            Arc::new(RwLock::new(RouteTable::new())),
            shutdown,
            active,
            loopback,
        )
        .await
        .unwrap();
        assert_eq!(
            *events.lock().unwrap(),
            [Event::Removed(first_ip), Event::Removed(second_ip)]
        );
    }

    #[tokio::test]
    async fn failed_loopback_ensure_leaves_address_available_for_retry() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let calls = Arc::new(AtomicUsize::new(0));
        let mut loopback = LoopbackAliases {
            change: Some(Arc::new({
                let calls = Arc::clone(&calls);
                move |_, _| {
                    if calls.fetch_add(1, Ordering::Relaxed) == 0 {
                        return Err(io::Error::other("broker unavailable; run gateway setup"));
                    }
                    Ok(())
                }
            })),
            ..LoopbackAliases::default()
        };
        let ip = crate::loopback::DEFAULT_LOOPBACK_START;
        let alias = desired_alias(SocketAddrV4::new(ip, 5432));
        let error = prepare_tcp_alias(&alias, &mut loopback, |_| async {
            panic!("bind must wait for a successful broker response");
        })
        .await
        .unwrap_err();
        assert!(
            matches!(error, GatewayError::TcpAliasBindError { reason, .. }
            if reason.contains("broker unavailable; run gateway setup"))
        );
        assert!(loopback.managed_loopback_ips.is_empty());

        prepare_tcp_alias(&alias, &mut loopback, |_| {
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        })
        .await
        .unwrap();
        assert_eq!(loopback.managed_loopback_ips, [ip].into());
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        loopback.remove_unused(&HashMap::new()).await;
    }

    fn desired_alias(addr: SocketAddrV4) -> DesiredTcpAlias {
        DesiredTcpAlias {
            domains: vec!["db.app.test".to_owned()],
            bind_ip: *addr.ip(),
            bind_port: addr.port(),
            upstream: "127.0.0.1:19432".parse().unwrap(),
        }
    }

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
            managed_listener: None,
            registered: Utc::now(),
        });

        let desired = desired_tcp_aliases(&table);
        let alias = desired.get("127.1.0.7:5432").expect("tcp alias");
        assert_eq!(alias.bind_ip, Ipv4Addr::new(127, 1, 0, 7));
        assert_eq!(alias.bind_port, 5432);
        assert_eq!(alias.upstream, "127.0.0.1:19432".parse().unwrap());
        assert_eq!(alias.domains, vec!["db.app.test".to_owned()]);
    }

    #[test]
    fn desired_tcp_aliases_deduplicates_domains_with_identical_bind_and_upstream() {
        let mut table = RouteTable::new();
        for domain in ["db.app1.test", "db.app2.test"] {
            table.upsert(Route {
                domain: domain.to_owned(),
                target: None,
                dns_ip: Some(Ipv4Addr::new(127, 1, 0, 7)),
                tcp_port: Some(5432),
                tcp_target: Some("127.0.0.1:19432".to_owned()),
                source: RouteSource::Container,
                project: "/tmp/app".to_owned(),
                tls: false,
                managed_listener: None,
                registered: Utc::now(),
            });
        }

        let desired = desired_tcp_aliases(&table);
        let alias = desired.get("127.1.0.7:5432").expect("tcp alias");
        assert_eq!(desired.len(), 1);
        assert_eq!(
            alias.domains,
            vec!["db.app1.test".to_owned(), "db.app2.test".to_owned()]
        );
    }
}
