//! HTTP/1 connections are isolated by the complete published route identity.
//! Managed sockets are connected and verified by a background worker before
//! request code can acquire them. A socket stays attached to that verified
//! peer even if the listening address is subsequently reused.
use super::*;
use crate::routes::Route;
use std::collections::{HashMap, HashSet};
use std::io;
use std::time::Instant;
use tokio::sync::Notify;

type Sender = hyper::client::conn::http1::SendRequest<Incoming>;
const RECHECK: Duration = Duration::from_secs(1);
const MAX_AGE: Duration = Duration::from_secs(3);
const IDLE_TTL: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
struct PreparedState {
    socket: Option<TcpStream>,
    checked: Option<Instant>,
    refused: Option<String>,
    demand: bool,
}
#[derive(Debug, Default)]
struct Prepared {
    state: RwLock<PreparedState>,
    changed: Notify,
}

#[derive(Debug)]
pub(super) struct Upstreams {
    idle: RwLock<HashMap<String, Vec<(Instant, Sender)>>>,
    managed: RwLock<HashMap<String, Arc<Prepared>>>,
    connect_timeout: Duration,
    table: Arc<RwLock<RouteTable>>,
}

fn key(route: &Route, revision: u64) -> io::Result<String> {
    serde_json::to_string(route)
        .map(|route| format!("{revision}:{route}"))
        .map_err(io::Error::other)
}
fn refused(reason: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, reason.into())
}

impl Upstreams {
    pub(super) fn start(
        table: Arc<RwLock<RouteTable>>,
        connect_timeout: Duration,
    ) -> (Arc<Self>, tokio::task::JoinHandle<()>) {
        let pool = Arc::new(Self {
            idle: RwLock::new(HashMap::new()),
            managed: RwLock::new(HashMap::new()),
            connect_timeout,
            table: table.clone(),
        });
        let weak = Arc::downgrade(&pool);
        let worker = tokio::spawn(async move {
            // Dropping the manager aborts every child, including on server
            // cancellation. Workers hold no pool reference across a sleep.
            let mut workers = tokio::task::JoinSet::new();
            loop {
                let Some(pool) = weak.upgrade() else { break };
                let (routes, revision) = {
                    let table = crate::locks::read_tolerant(&table);
                    (
                        table
                            .all_routes()
                            .into_iter()
                            .cloned()
                            .collect::<Vec<Route>>(),
                        table.revision,
                    )
                };
                let active: HashSet<String> = routes
                    .iter()
                    .filter_map(|r| key(r, revision).ok())
                    .collect();
                crate::locks::write_tolerant(&pool.idle).retain(|k, entries| {
                    entries.retain(|(at, sender)| at.elapsed() < IDLE_TTL && !sender.is_closed());
                    active.contains(k) && !entries.is_empty()
                });
                crate::locks::write_tolerant(&pool.managed).retain(|k, _| active.contains(k));
                for route in routes {
                    let Some(owner) = route.managed_listener.clone() else {
                        continue;
                    };
                    let Ok(route_key) = key(&route, revision) else {
                        continue;
                    };
                    let prepared = {
                        let mut managed = crate::locks::write_tolerant(&pool.managed);
                        if managed.contains_key(&route_key) {
                            continue;
                        }
                        let prepared = Arc::new(Prepared::default());
                        crate::locks::write_tolerant(&prepared.state).demand = true;
                        managed.insert(route_key.clone(), prepared.clone());
                        prepared
                    };
                    let weak_pool = Arc::downgrade(&pool);
                    let weak_prepared = Arc::downgrade(&prepared);
                    workers.spawn(async move {
                        let mut last_check = None::<Instant>;
                        loop {
                            let (Some(pool), Some(prepared)) =
                                (weak_pool.upgrade(), weak_prepared.upgrade())
                            else {
                                break;
                            };
                            if !crate::locks::read_tolerant(&pool.managed).contains_key(&route_key)
                            {
                                break;
                            }
                            let demand = crate::locks::read_tolerant(&prepared.state).demand;
                            if demand || last_check.is_none_or(|at| at.elapsed() >= RECHECK) {
                                let socket = if demand {
                                    Some(connect(&owner.address, pool.connect_timeout).await)
                                } else {
                                    None
                                };
                                let verify_owner = owner.clone();
                                let verified = tokio::task::spawn_blocking(move || {
                                    crate::managed_listener::verify_managed_listener_owner(
                                        &verify_owner,
                                    )
                                })
                                .await
                                .map_err(io::Error::other)
                                .and_then(|result| result);
                                last_check = Some(Instant::now());
                                let mut state = crate::locks::write_tolerant(&prepared.state);
                                match verified {
                                    Ok(()) => {
                                        state.checked = last_check;
                                        state.refused = None;
                                        if let Some(socket) = socket {
                                            match socket {
                                                Ok(socket) => {
                                                    state.socket = Some(socket);
                                                    state.demand = false;
                                                }
                                                Err(error) => {
                                                    state.refused = Some(error.to_string());
                                                    state.demand = false;
                                                }
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        state.socket = None;
                                        state.checked = None;
                                        state.refused = Some(error.to_string());
                                        state.demand = false;
                                    }
                                }
                                prepared.changed.notify_waiters();
                            }
                            drop(prepared);
                            drop(pool);
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    });
                }
                while workers.try_join_next().is_some() {}
                drop(pool);
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        });
        (pool, worker)
    }

    async fn prepared(&self, route: &Route, revision: u64) -> io::Result<Arc<Prepared>> {
        let route_key = key(route, revision)?;
        tokio::time::timeout(self.connect_timeout, async {
            loop {
                let prepared = crate::locks::read_tolerant(&self.managed)
                    .get(&route_key)
                    .cloned();
                if let Some(prepared) = prepared {
                    return Ok(prepared);
                }
                {
                    let table = crate::locks::read_tolerant(&self.table);
                    if table.revision != revision || table.lookup(&route.domain) != Some(route) {
                        return Err(refused("route publication was withdrawn or replaced"));
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| refused("route ownership preparation timed out"))?
    }

    async fn allowed(&self, route: &Route, revision: u64) -> io::Result<()> {
        let Some(owner) = route.managed_listener.as_ref() else {
            return Ok(());
        };
        let prepared = self.prepared(route, revision).await?;
        tokio::time::timeout(self.connect_timeout, async {
            loop {
                let changed = prepared.changed.notified();
                {
                    let state = crate::locks::read_tolerant(&prepared.state);
                    if let Some(reason) = &state.refused {
                        return Err(refused(reason));
                    }
                    if state.checked.is_some_and(|at| at.elapsed() < MAX_AGE) {
                        // These are direct per-PID kernel reads and the bounded
                        // supervisor revocation record, never ancestry/socket
                        // inventories or a subprocess.
                        return crate::managed_listener::verify_cached_listener_identity(owner)
                            .map_err(|error| refused(error.to_string()));
                    }
                }
                changed.await;
            }
        })
        .await
        .map_err(|_| refused("route ownership check expired"))?
    }

    pub(super) async fn socket(&self, route: &Route, revision: u64) -> io::Result<TcpStream> {
        if route.managed_listener.is_none() {
            return connect(
                route
                    .target
                    .as_deref()
                    .ok_or_else(|| io::Error::other("missing target"))?,
                self.connect_timeout,
            )
            .await;
        }
        let prepared = self.prepared(route, revision).await?;
        tokio::time::timeout(self.connect_timeout, async {
            loop {
                let changed = prepared.changed.notified();
                self.allowed(route, revision).await?;
                {
                    let mut state = crate::locks::write_tolerant(&prepared.state);
                    if let Some(socket) = state.socket.take() {
                        // An upstream may close a prepared socket before the
                        // first request (header/idle timeout). Never hand that
                        // dead connection to the first browser request.
                        if socket
                            .try_read(&mut [0_u8; 1])
                            .is_err_and(|error| error.kind() == io::ErrorKind::WouldBlock)
                        {
                            return Ok(socket);
                        }
                        // EOF, reset or unsolicited HTTP bytes: prepare afresh.
                    }
                    state.demand = true;
                }
                changed.await;
            }
        })
        .await
        .map_err(|_| refused("verified upstream connection timed out"))?
    }

    pub(super) async fn sender(
        &self,
        route: &Route,
        revision: u64,
    ) -> Result<Sender, Box<dyn StdError + Send + Sync>> {
        self.allowed(route, revision).await?;
        let route_key = key(route, revision)?;
        let sender = crate::locks::write_tolerant(&self.idle)
            .get_mut(&route_key)
            .and_then(|entries| {
                while let Some((at, sender)) = entries.pop() {
                    if at.elapsed() < IDLE_TTL && !sender.is_closed() {
                        return Some(sender);
                    }
                }
                None
            });
        if let Some(mut sender) = sender {
            if sender.ready().await.is_ok() {
                return Ok(sender);
            }
        }
        let stream = self.socket(route, revision).await?;
        let (mut sender, driver) =
            hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        tokio::spawn(async move {
            let _ = driver.await;
        });
        sender.ready().await?;
        self.allowed(route, revision).await?;
        Ok(sender)
    }

    pub(super) fn recycle(self: &Arc<Self>, route: &Route, revision: u64, mut sender: Sender) {
        let Ok(route_key) = key(route, revision) else {
            return;
        };
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            // Hyper becomes ready only once the streamed response finishes;
            // Connection: close, failed bodies and upgrades close the sender.
            if tokio::time::timeout(IDLE_TTL, sender.ready())
                .await
                .is_ok_and(|r| r.is_ok())
            {
                if let Some(pool) = weak.upgrade() {
                    let mut idle = crate::locks::write_tolerant(&pool.idle);
                    let entries = idle.entry(route_key).or_default();
                    if entries.len() < 8 {
                        entries.push((Instant::now(), sender));
                    }
                }
            }
        });
    }
}

async fn connect(target: &str, timeout: Duration) -> io::Result<TcpStream> {
    let stream = tokio::time::timeout(timeout, TcpStream::connect(target))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "upstream connect timed out"))??;
    stream.set_nodelay(true)?;
    Ok(stream)
}
