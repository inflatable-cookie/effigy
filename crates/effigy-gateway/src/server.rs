//! Gateway server lifecycle — coordinates DNS, proxy, and route watching.
//!
//! The gateway runs as a background process with three concurrent tasks:
//!
//! 1. DNS resolver (UDP)
//! 2. HTTP reverse proxy (TCP)
//! 3. Route table file watcher
//!
//! All three tasks share the route table via `Arc<RwLock<RouteTable>>`.
//! The file watcher reloads the table when the JSON file changes. Shutdown
//! is coordinated via a `tokio::sync::watch` channel.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::watch;
use tracing::{debug, error, info};

use crate::dns::{run_dns_server, DnsCache, DnsConfig};
use crate::error::GatewayError;
use crate::proxy::{run_proxy_server, run_tls_proxy_server, ProxyConfig};
use crate::routes::{LiveRouteTable, RouteTable};
use crate::stats::GatewayStats;
use crate::tls::{
    server_config_from_resolver, sync_sni_resolver_from_dir, SniCertResolver, TlsConfig,
};

const IDLE_SHUTDOWN_DELAY: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdleShutdownAction {
    None,
    Arm,
    Cancel,
}

/// Configuration for the full gateway.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// DNS resolver configuration.
    pub dns: DnsConfig,

    /// HTTP proxy configuration.
    pub proxy: ProxyConfig,

    /// TLS configuration. If Some, HTTPS proxy is enabled.
    pub tls: Option<TlsConfig>,

    /// Path to the route table JSON file.
    pub route_table_path: PathBuf,

    /// Path to the PID file for lifecycle management.
    pub pid_file_path: PathBuf,
}

impl GatewayConfig {
    /// Create a gateway config using the standard effigy paths.
    ///
    /// `gateway_dir` is typically `~/.effigy/gateway/`.
    pub fn standard(gateway_dir: PathBuf) -> Self {
        Self {
            dns: DnsConfig::default(),
            proxy: ProxyConfig {
                tls_bind_addr: Some(SocketAddr::from(([127, 0, 0, 1], 443))),
                ..ProxyConfig::default()
            },
            tls: Some(TlsConfig::new(gateway_dir.join("certs"))),
            route_table_path: gateway_dir.join("routes.json"),
            pid_file_path: gateway_dir.join("gateway.pid"),
        }
    }

    /// Use custom bind addresses.
    pub fn with_addrs(mut self, dns_addr: SocketAddr, proxy_addr: SocketAddr) -> Self {
        self.dns.bind_addr = dns_addr;
        self.proxy.bind_addr = proxy_addr;
        self
    }

    /// Use a custom TLD.
    pub fn with_tld(mut self, tld: String) -> Self {
        self.dns.tld = tld;
        self
    }

    /// Enable HTTPS with TLS certificates from the given directory.
    ///
    /// Also sets the HTTPS bind address on the proxy config.
    pub fn with_tls(mut self, certs_dir: PathBuf, https_addr: SocketAddr) -> Self {
        self.tls = Some(TlsConfig::new(certs_dir));
        self.proxy.tls_bind_addr = Some(https_addr);
        self
    }
}

/// Status of a running gateway.
#[derive(Debug, Clone)]
pub struct GatewayStatus {
    /// Process ID of the running gateway.
    pub pid: u32,

    /// DNS resolver bind address.
    pub dns_addr: SocketAddr,

    /// HTTP proxy bind address.
    pub proxy_addr: SocketAddr,

    /// Number of registered routes.
    pub route_count: usize,

    /// All registered routes.
    pub routes: Vec<crate::routes::Route>,
}

/// Write a PID file for lifecycle management.
pub fn write_pid_file(path: &Path) -> Result<(), GatewayError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, std::process::id().to_string())?;
    Ok(())
}

/// Read the PID from a PID file.
pub fn read_pid_file(path: &Path) -> Result<u32, GatewayError> {
    if !path.exists() {
        return Err(GatewayError::NotRunning);
    }
    let content = std::fs::read_to_string(path)?;
    content
        .trim()
        .parse::<u32>()
        .map_err(|_| GatewayError::NotRunning)
}

/// Remove the PID file.
pub fn remove_pid_file(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Check whether a process with the given PID is running.
#[cfg(unix)]
pub fn process_is_running(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "pid="])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub fn process_is_running(_pid: u32) -> bool {
    // On non-Unix, conservatively assume running.
    true
}

/// Get the status of the gateway, if running.
pub fn get_status(config: &GatewayConfig) -> Result<GatewayStatus, GatewayError> {
    let pid = read_pid_file(&config.pid_file_path)?;

    if !process_is_running(pid) {
        remove_pid_file(&config.pid_file_path);
        return Err(GatewayError::NotRunning);
    }

    let table = RouteTable::load(&config.route_table_path)?;

    Ok(GatewayStatus {
        pid,
        dns_addr: config.dns.bind_addr,
        proxy_addr: config.proxy.bind_addr,
        route_count: table.len(),
        routes: table.all_routes().into_iter().cloned().collect(),
    })
}

/// Run the gateway server.
///
/// This function blocks until a shutdown signal is received (SIGTERM/SIGINT).
/// It starts the DNS resolver, HTTP proxy, and route table file watcher
/// concurrently.
pub async fn run_gateway(config: GatewayConfig) -> Result<(), GatewayError> {
    // Check if already running.
    if let Ok(pid) = read_pid_file(&config.pid_file_path) {
        if process_is_running(pid) {
            return Err(GatewayError::AlreadyRunning { pid });
        }
        // Stale PID file — clean up.
        remove_pid_file(&config.pid_file_path);
    }

    // Write PID file.
    write_pid_file(&config.pid_file_path)?;

    // Load the route table.
    let live_table = LiveRouteTable::new(config.route_table_path.clone())?;
    let shared_table = live_table.shared_table();

    // Create stats tracker.
    let stats = Arc::new(GatewayStats::new());

    // Create DNS lookup cache (shared with file watcher for invalidation).
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));

    // Create shutdown channel.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Set up OS signal handler.
    let signal_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("received shutdown signal");
        let _ = signal_tx.send(true);
    });

    // Set up file watcher for route table.
    // When routes change, the watcher reloads the table and clears the
    // DNS cache so new routes are picked up immediately.
    let watcher_table = Arc::clone(&shared_table);
    let watcher_cache = Arc::clone(&dns_cache);
    let watcher_path = config.route_table_path.clone();
    let idle_shutdown_generation = Arc::new(AtomicU64::new(0));
    let _watcher = setup_file_watcher(
        &watcher_path,
        watcher_table,
        watcher_cache,
        idle_shutdown_generation,
        shutdown_tx.clone(),
    )?;

    let has_tls = config.proxy.tls_bind_addr.is_some() && config.tls.is_some();

    info!(
        dns = %config.dns.bind_addr,
        proxy = %config.proxy.bind_addr,
        tls = has_tls,
        tld = %config.dns.tld,
        "gateway starting"
    );

    // Run DNS and proxy concurrently.
    let dns_handle = tokio::spawn(run_dns_server(
        config.dns.clone(),
        Arc::clone(&shared_table),
        Arc::clone(&stats),
        Arc::clone(&dns_cache),
        shutdown_rx.clone(),
    ));

    let proxy_handle = tokio::spawn(run_proxy_server(
        config.proxy.clone(),
        Arc::clone(&shared_table),
        Arc::clone(&stats),
        shutdown_rx.clone(),
    ));

    // Optionally start the HTTPS proxy.
    let (_tls_watcher, tls_handle) =
        if let (Some(tls_addr), Some(tls_config)) = (config.proxy.tls_bind_addr, &config.tls) {
            let certs_dir = tls_config.certs_dir.clone();
            std::fs::create_dir_all(&certs_dir)?;
            let resolver = Arc::new(SniCertResolver::new());
            let cert_count = sync_sni_resolver_from_dir(&resolver, &certs_dir)?;
            if cert_count == 0 {
                info!(
                    "HTTPS enabled but no certificates found in {} — \
                     run `effigy gateway setup-tls` and start a TLS-enabled route",
                    certs_dir.display()
                );
            } else {
                info!(certs = cert_count, "loaded TLS certificates for HTTPS");
            }
            let tls_watcher = setup_tls_watcher(&certs_dir, Arc::clone(&resolver))?;
            let server_config = Arc::new(server_config_from_resolver(resolver));
            let handle = tokio::spawn(run_tls_proxy_server(
                tls_addr,
                server_config,
                Arc::clone(&shared_table),
                Arc::clone(&stats),
                config.proxy.clone(),
                shutdown_rx,
            ));
            (Some(tls_watcher), Some(handle))
        } else {
            (None, None)
        };

    // Wait for any server task to finish (typically all stop on shutdown).
    tokio::select! {
        result = dns_handle => handle_server_task_result(result, "DNS server")?,
        result = proxy_handle => handle_server_task_result(result, "proxy server")?,
        result = async {
            if let Some(handle) = tls_handle {
                handle.await
            } else {
                // No TLS handle — never resolves, so the other branches win.
                std::future::pending().await
            }
        } => handle_server_task_result(result, "HTTPS server")?,
    }

    // Clean up PID file.
    remove_pid_file(&config.pid_file_path);

    info!("gateway stopped");
    Ok(())
}

/// Set up a filesystem watcher that reloads the route table when it changes.
fn setup_file_watcher(
    path: &Path,
    table: Arc<RwLock<RouteTable>>,
    dns_cache: Arc<DnsCache>,
    idle_shutdown_generation: Arc<AtomicU64>,
    shutdown_tx: watch::Sender<bool>,
) -> Result<RecommendedWatcher, GatewayError> {
    let watched_path = path.to_path_buf();

    let mut watcher =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(ev) => {
                use notify::EventKind;
                match ev.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        debug!(path = %watched_path.display(), "route table changed, reloading");
                        if let Err(error) = reload_route_table_and_maybe_schedule_shutdown(
                            &watched_path,
                            &table,
                            &dns_cache,
                            &idle_shutdown_generation,
                            &shutdown_tx,
                        ) {
                            error!(error = %error, "failed to reload route table");
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => {
                error!(error = %e, "file watcher error");
            }
        })
        .map_err(|e| GatewayError::WatcherError(e.to_string()))?;

    // Watch the parent directory (the file might not exist yet).
    let watch_dir = path.parent().unwrap_or(path.as_ref());
    if watch_dir.exists() {
        watcher
            .watch(watch_dir, RecursiveMode::NonRecursive)
            .map_err(|e| GatewayError::WatcherError(e.to_string()))?;
    }

    Ok(watcher)
}

fn reload_route_table_and_maybe_schedule_shutdown(
    path: &Path,
    table: &Arc<RwLock<RouteTable>>,
    dns_cache: &Arc<DnsCache>,
    idle_shutdown_generation: &Arc<AtomicU64>,
    shutdown_tx: &watch::Sender<bool>,
) -> Result<(), GatewayError> {
    let new_table = RouteTable::load(path)?;
    let action = apply_reloaded_route_table(table, new_table, dns_cache);
    debug!("route table reloaded, DNS cache cleared");
    match action {
        IdleShutdownAction::Arm => {
            let generation = idle_shutdown_generation.fetch_add(1, Ordering::SeqCst) + 1;
            schedule_idle_shutdown(
                Arc::clone(table),
                Arc::clone(idle_shutdown_generation),
                shutdown_tx.clone(),
                generation,
                IDLE_SHUTDOWN_DELAY,
            );
        }
        IdleShutdownAction::Cancel => {
            idle_shutdown_generation.fetch_add(1, Ordering::SeqCst);
            debug!("route table became non-empty; cancelled pending idle shutdown");
        }
        IdleShutdownAction::None => {}
    }
    Ok(())
}

fn apply_reloaded_route_table(
    table: &Arc<RwLock<RouteTable>>,
    new_table: RouteTable,
    dns_cache: &Arc<DnsCache>,
) -> IdleShutdownAction {
    let action = {
        let mut guard = table.write().expect("route table lock poisoned");
        let was_empty = guard.is_empty();
        let is_empty = new_table.is_empty();
        *guard = new_table;
        match (was_empty, is_empty) {
            (false, true) => IdleShutdownAction::Arm,
            (true, false) => IdleShutdownAction::Cancel,
            _ => IdleShutdownAction::None,
        }
    };
    dns_cache.clear();
    action
}

fn schedule_idle_shutdown(
    table: Arc<RwLock<RouteTable>>,
    idle_shutdown_generation: Arc<AtomicU64>,
    shutdown_tx: watch::Sender<bool>,
    generation: u64,
    delay: Duration,
) {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if idle_shutdown_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        if table.read().expect("route table lock poisoned").is_empty() {
            info!("route table stayed empty through idle timeout; stopping gateway");
            let _ = shutdown_tx.send(true);
        }
    });
}

fn setup_tls_watcher(
    certs_dir: &Path,
    resolver: Arc<SniCertResolver>,
) -> Result<RecommendedWatcher, GatewayError> {
    let watched_dir = certs_dir.to_path_buf();

    let mut watcher =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(ev) => {
                use notify::EventKind;
                match ev.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        debug!(path = %watched_dir.display(), "TLS cert directory changed, reloading");
                        match sync_sni_resolver_from_dir(&resolver, &watched_dir) {
                            Ok(count) => {
                                debug!(certs = count, "TLS certs reloaded");
                            }
                            Err(error) => {
                                error!(error = %error, "failed to reload TLS certs");
                            }
                        }
                    }
                    _ => {}
                }
            }
            Err(error) => {
                error!(error = %error, "TLS watcher error");
            }
        })
        .map_err(|e| GatewayError::WatcherError(e.to_string()))?;

    std::fs::create_dir_all(certs_dir)?;
    watcher
        .watch(certs_dir, RecursiveMode::NonRecursive)
        .map_err(|e| GatewayError::WatcherError(e.to_string()))?;

    Ok(watcher)
}

fn handle_server_task_result(
    result: Result<Result<(), GatewayError>, tokio::task::JoinError>,
    label: &str,
) -> Result<(), GatewayError> {
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            error!(error = %error, "{label} failed");
            Err(error)
        }
        Err(error) => {
            error!(error = %error, "{label} task failed");
            Err(GatewayError::WatcherError(format!(
                "{label} task failed: {error}"
            )))
        }
    }
}

#[cfg(test)]
#[path = "server/tests.rs"]
mod tests;
