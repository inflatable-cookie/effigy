//! HTTP/1 connections are isolated by the complete published route identity.
//! Every route, managed listener or not, is forwarded to its published target.
use super::*;
use crate::routes::Route;
use std::collections::{HashMap, HashSet};
use std::io;
use std::time::Instant;

type Sender = hyper::client::conn::http1::SendRequest<Incoming>;
const IDLE_TTL: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub(super) struct Upstreams {
    idle: RwLock<HashMap<String, Vec<(Instant, Sender)>>>,
    connect_timeout: Duration,
    table: Arc<RwLock<RouteTable>>,
}

fn key(route: &Route, revision: u64) -> io::Result<String> {
    serde_json::to_string(route)
        .map(|route| format!("{revision}:{route}"))
        .map_err(io::Error::other)
}

impl Upstreams {
    pub(super) fn start(
        table: Arc<RwLock<RouteTable>>,
        connect_timeout: Duration,
    ) -> (Arc<Self>, tokio::task::JoinHandle<()>) {
        let pool = Arc::new(Self {
            idle: RwLock::new(HashMap::new()),
            connect_timeout,
            table,
        });
        let weak = Arc::downgrade(&pool);
        let worker = tokio::spawn(async move {
            // Dropping the manager aborts the sweep. It holds no pool reference
            // across a sleep.
            loop {
                let Some(pool) = weak.upgrade() else { break };
                let (routes, revision) = {
                    let table = crate::locks::read_tolerant(&pool.table);
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
                    .filter_map(|route| key(route, revision).ok())
                    .collect();
                crate::locks::write_tolerant(&pool.idle).retain(|k, entries| {
                    entries.retain(|(at, sender)| at.elapsed() < IDLE_TTL && !sender.is_closed());
                    active.contains(k) && !entries.is_empty()
                });
                drop(pool);
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        });
        (pool, worker)
    }

    pub(super) async fn socket(&self, route: &Route, _revision: u64) -> io::Result<TcpStream> {
        connect(
            route
                .target
                .as_deref()
                .ok_or_else(|| io::Error::other("missing target"))?,
            self.connect_timeout,
        )
        .await
    }

    pub(super) async fn sender(
        &self,
        route: &Route,
        revision: u64,
    ) -> Result<Sender, Box<dyn StdError + Send + Sync>> {
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
        let (sender, driver) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        tokio::spawn(async move {
            let _ = driver.await;
        });
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
