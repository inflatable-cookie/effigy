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
