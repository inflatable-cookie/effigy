use super::*;
use crate::dns::DnsCache;
use crate::routes::{Route, RouteSource, RouteTable};
use std::sync::{Arc, RwLock};
use tokio::sync::watch;

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
    assert_eq!(
        config.proxy.tls_bind_addr,
        Some("127.0.0.1:443".parse().unwrap())
    );
    assert_eq!(
        config.tls.as_ref().unwrap().certs_dir,
        PathBuf::from("/tmp/effigy/gateway/certs")
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

#[tokio::test]
async fn run_gateway_propagates_proxy_bind_failure() {
    let dir = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let occupied_port = listener.local_addr().unwrap().port();
    let dns_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dns_port = dns_socket.local_addr().unwrap().port();
    drop(dns_socket);

    let config = GatewayConfig::standard(dir.path().to_path_buf()).with_addrs(
        format!("127.0.0.1:{dns_port}").parse().unwrap(),
        format!("127.0.0.1:{occupied_port}").parse().unwrap(),
    );

    let error = run_gateway(config)
        .await
        .expect_err("proxy bind should fail");
    assert!(matches!(error, GatewayError::ProxyBindError { .. }));
}

fn demo_route_table() -> RouteTable {
    let mut table = RouteTable::new();
    table.upsert(Route {
        domain: "demo.test".to_owned(),
        target: Some("127.0.0.1:41003".to_owned()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        tls: false,
        source: RouteSource::Container,
        project: "/tmp/demo".to_owned(),
        registered: chrono::Utc::now(),
    });
    table
}

#[test]
fn apply_reloaded_route_table_noops_when_already_empty() {
    let table = Arc::new(RwLock::new(RouteTable::new()));
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));

    let action = apply_reloaded_route_table(&table, RouteTable::new(), &dns_cache);

    assert_eq!(action, IdleShutdownAction::None);
    assert!(table.read().unwrap().is_empty());
}

#[test]
fn apply_reloaded_route_table_arms_idle_shutdown_when_last_route_removed() {
    let table = Arc::new(RwLock::new(demo_route_table()));
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));

    let action = apply_reloaded_route_table(&table, RouteTable::new(), &dns_cache);

    assert_eq!(action, IdleShutdownAction::Arm);
    assert!(table.read().unwrap().is_empty());
}

#[test]
fn apply_reloaded_route_table_cancels_idle_shutdown_when_route_returns() {
    let table = Arc::new(RwLock::new(RouteTable::new()));
    let dns_cache = Arc::new(DnsCache::new(std::time::Duration::from_secs(2)));

    let action = apply_reloaded_route_table(&table, demo_route_table(), &dns_cache);

    assert_eq!(action, IdleShutdownAction::Cancel);
    assert_eq!(table.read().unwrap().len(), 1);
}

#[test]
fn scheduled_idle_shutdown_stops_gateway_when_table_stays_empty() {
    let table = Arc::new(RwLock::new(RouteTable::new()));
    let generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    schedule_idle_shutdown(
        Arc::clone(&table),
        generation,
        shutdown_tx,
        1,
        std::time::Duration::from_millis(10),
    );
    std::thread::sleep(std::time::Duration::from_millis(30));

    assert!(*shutdown_rx.borrow());
}

#[test]
fn scheduled_idle_shutdown_is_cancelled_when_generation_changes() {
    let table = Arc::new(RwLock::new(RouteTable::new()));
    let generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    schedule_idle_shutdown(
        Arc::clone(&table),
        Arc::clone(&generation),
        shutdown_tx,
        1,
        std::time::Duration::from_millis(20),
    );
    generation.store(2, std::sync::atomic::Ordering::SeqCst);
    std::thread::sleep(std::time::Duration::from_millis(40));

    assert!(!*shutdown_rx.borrow());
}
