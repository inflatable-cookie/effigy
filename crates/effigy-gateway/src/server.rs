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
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::watch;
use tracing::{debug, error, info};

use crate::dns::{run_dns_server, DnsCache, DnsConfig};
use crate::error::GatewayError;
use crate::proxy::{run_proxy_server, run_tls_proxy_server, ProxyConfig};
use crate::routes::{LiveRouteTable, RouteTable};
use crate::stats::GatewayStats;
use crate::tls::TlsConfig;

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
            proxy: ProxyConfig::default(),
            tls: None,
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
pub fn write_pid_file(path: &PathBuf) -> Result<(), GatewayError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, std::process::id().to_string())?;
    Ok(())
}

/// Read the PID from a PID file.
pub fn read_pid_file(path: &PathBuf) -> Result<u32, GatewayError> {
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
pub fn remove_pid_file(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
}

/// Check whether a process with the given PID is running.
#[cfg(unix)]
pub fn process_is_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
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
    let _watcher = setup_file_watcher(&watcher_path, watcher_table, watcher_cache)?;

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
    let tls_handle =
        if let (Some(tls_addr), Some(tls_config)) = (config.proxy.tls_bind_addr, &config.tls) {
            // Build an SNI resolver that serves the right cert per domain.
            match crate::tls::build_sni_resolver_from_dir(&tls_config.certs_dir) {
                Ok(resolver) => {
                    if resolver.cert_count() == 0 {
                        info!(
                            "HTTPS enabled but no certificates found in {} — \
                         run `effigy gateway setup-tls` first",
                            tls_config.certs_dir.display()
                        );
                        None
                    } else {
                        info!(
                            certs = resolver.cert_count(),
                            "loaded TLS certificates for HTTPS"
                        );
                        let server_config = resolver.into_server_config();
                        let arc_config = std::sync::Arc::new(server_config);
                        Some(tokio::spawn(run_tls_proxy_server(
                            tls_addr,
                            arc_config,
                            Arc::clone(&shared_table),
                            Arc::clone(&stats),
                            config.proxy.clone(),
                            shutdown_rx,
                        )))
                    }
                }
                Err(e) => {
                    error!(error = %e, "failed to load TLS certs — HTTPS disabled");
                    None
                }
            }
        } else {
            None
        };

    // Wait for any server task to finish (typically all stop on shutdown).
    tokio::select! {
        result = dns_handle => {
            if let Err(e) = result {
                error!(error = %e, "DNS server task failed");
            }
        }
        result = proxy_handle => {
            if let Err(e) = result {
                error!(error = %e, "proxy server task failed");
            }
        }
        result = async {
            if let Some(handle) = tls_handle {
                handle.await
            } else {
                // No TLS handle — never resolves, so the other branches win.
                std::future::pending().await
            }
        } => {
            if let Err(e) = result {
                error!(error = %e, "HTTPS server task failed");
            }
        }
    }

    // Clean up PID file.
    remove_pid_file(&config.pid_file_path);

    info!("gateway stopped");
    Ok(())
}

/// Set up a filesystem watcher that reloads the route table when it changes.
fn setup_file_watcher(
    path: &PathBuf,
    table: Arc<RwLock<RouteTable>>,
    dns_cache: Arc<DnsCache>,
) -> Result<RecommendedWatcher, GatewayError> {
    let watched_path = path.clone();

    let mut watcher =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(ev) => {
                use notify::EventKind;
                match ev.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        debug!(path = %watched_path.display(), "route table changed, reloading");
                        match RouteTable::load(&watched_path) {
                            Ok(new_table) => {
                                let mut guard = table.write().expect("route table lock poisoned");
                                *guard = new_table;
                                // Invalidate DNS cache so new routes are
                                // resolved immediately.
                                dns_cache.clear();
                                debug!("route table reloaded, DNS cache cleared");
                            }
                            Err(e) => {
                                error!(error = %e, "failed to reload route table");
                            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_config_paths() {
        let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));
        assert_eq!(
            config.route_table_path,
            PathBuf::from("/tmp/effigy/gateway/routes.json")
        );
        assert_eq!(
            config.pid_file_path,
            PathBuf::from("/tmp/effigy/gateway/gateway.pid")
        );
    }

    #[test]
    fn config_with_custom_addrs() {
        let config = GatewayConfig::standard(PathBuf::from("/tmp"))
            .with_addrs(
                "127.0.0.1:5353".parse().unwrap(),
                "127.0.0.1:8080".parse().unwrap(),
            )
            .with_tld("dev".to_string());

        assert_eq!(config.dns.bind_addr.port(), 5353);
        assert_eq!(config.proxy.bind_addr.port(), 8080);
        assert_eq!(config.dns.tld, "dev");
    }

    #[test]
    fn pid_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");

        write_pid_file(&pid_path).unwrap();
        let pid = read_pid_file(&pid_path).unwrap();
        assert_eq!(pid, std::process::id());

        remove_pid_file(&pid_path);
        assert!(!pid_path.exists());
    }

    #[test]
    fn read_missing_pid_file_returns_not_running() {
        let result = read_pid_file(&PathBuf::from("/nonexistent/test.pid"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GatewayError::NotRunning));
    }

    #[test]
    fn current_process_is_running() {
        assert!(process_is_running(std::process::id()));
    }

    #[test]
    fn nonexistent_process_is_not_running() {
        // PID 99999999 almost certainly doesn't exist.
        assert!(!process_is_running(99_999_999));
    }
}
