use super::*;
use chrono::Utc;
use effigy_gateway::loopback::LoopbackRegistry;
use effigy_gateway::routes::{Route, RouteSource};

fn tls_summary() -> GatewayTlsSummary {
    GatewayTlsSummary {
        https_addr: Some("127.0.0.1:443".parse().expect("https")),
        route_count: 0,
        cert_ready_count: 0,
        missing_domains: Vec::new(),
        mkcert_available: true,
        ca_installed: true,
    }
}

#[test]
fn render_gateway_status_json_includes_routes_when_stopped() {
    let table = RouteTable {
        routes: [(
            "demo.test".to_owned(),
            Route {
                domain: "demo.test".to_owned(),
                target: Some("127.0.0.1:8080".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                source: RouteSource::Manual,
                project: "/tmp/demo".to_owned(),
                tls: false,
                registered: Utc::now(),
            },
        )]
        .into_iter()
        .collect(),
    };
    let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));
    let tls = gateway_tls_summary(&config, &table);

    let rendered = serde_json::to_string(&render_routes_json(&gateway_route_dashboard(
        &config, &table, &tls,
    )))
    .expect("json");
    assert!(rendered.contains("demo.test"));
    assert!(rendered.contains("\"project\":\"/tmp/demo\""));
    assert!(rendered.contains("\"cert_ready\":false"));
}

#[test]
fn render_gateway_up_text_mentions_state_dir() {
    let status = GatewayStatus {
        pid: 1234,
        dns_addr: "127.0.0.1:15353".parse().expect("dns"),
        proxy_addr: "127.0.0.1:80".parse().expect("proxy"),
        route_count: 0,
        routes: Vec::new(),
        binary_version: Some("v0.3.2+local.test".to_owned()),
    };
    let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));

    let rendered = render_gateway_up_result(
        &config,
        GatewayUpState::Started(status),
        &tls_summary(),
        &[],
        false,
    )
    .expect("render");
    assert!(rendered.contains("gateway started"));
    assert!(rendered.contains("/tmp/effigy/gateway"));
    assert!(rendered.contains("https: 127.0.0.1:443"));
    assert!(rendered.contains("binary_version: v0.3.2+local.test"));
}

#[test]
fn gateway_status_match_requires_current_binary_identity() {
    let current = effigy_core::build_info::active_version();
    let matching = GatewayStatus {
        pid: 1234,
        dns_addr: "127.0.0.1:15353".parse().expect("dns"),
        proxy_addr: "127.0.0.1:80".parse().expect("proxy"),
        route_count: 0,
        routes: Vec::new(),
        binary_version: Some(current),
    };
    let missing = GatewayStatus {
        pid: 1234,
        dns_addr: "127.0.0.1:15353".parse().expect("dns"),
        proxy_addr: "127.0.0.1:80".parse().expect("proxy"),
        route_count: 0,
        routes: Vec::new(),
        binary_version: None,
    };

    assert!(gateway_status_matches_current_binary(&matching));
    assert!(!gateway_status_matches_current_binary(&missing));
}

#[test]
fn normalize_gateway_daemon_output_drops_error_block_prefix() {
    let rendered = normalize_gateway_daemon_output(
            "[error] Task failed\n  HTTP proxy failed to bind on 127.0.0.1:80: Permission denied (os error 13)",
        );
    assert!(!rendered.contains("[error] Task failed"));
    assert!(rendered.contains("Permission denied"));
    assert!(rendered.contains("requires elevated privileges"));
}

#[test]
fn gateway_up_preflight_reports_privileged_bind_requirement() {
    let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));
    let error = ensure_gateway_up_privileges(&config).expect_err("should fail as non-root");
    assert!(error
        .to_string()
        .contains("requires elevated privileges on this machine"));
    assert!(error.to_string().contains("127.0.0.1:80"));
    assert!(error.to_string().contains("127.0.0.1:443"));
    assert!(!error.to_string().contains("/etc/resolver"));
}

#[test]
fn render_gateway_up_text_includes_warning_lines() {
    let status = GatewayStatus {
        pid: 1234,
        dns_addr: "127.0.0.1:15353".parse().expect("dns"),
        proxy_addr: "127.0.0.1:8080".parse().expect("proxy"),
        route_count: 0,
        routes: Vec::new(),
        binary_version: None,
    };
    let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));

    let rendered = render_gateway_up_result(
        &config,
        GatewayUpState::Started(status),
        &tls_summary(),
        &["resolver setup skipped".to_owned()],
        false,
    )
    .expect("render");
    assert!(rendered.contains("[warn] resolver setup skipped"));
}

#[test]
fn prepare_gateway_state_creates_loopback_registry_file() {
    let root = std::env::temp_dir().join(format!(
        "effigy-gateway-state-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create temp root");

    let config = GatewayConfig::standard(root.join("gateway"));
    prepare_gateway_state_for_elevated_run(&config).expect("prepare");

    let registry = LoopbackRegistry::load(&config.loopback_registry_path).expect("registry");
    assert!(registry.is_empty());
    assert!(config.loopback_registry_path.exists());

    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(target_os = "macos")]
#[test]
fn build_gateway_elevated_shell_command_includes_gateway_env_and_subcommand() {
    let shell_command = build_gateway_elevated_shell_command(GatewaySubcommand::SetupTls, true)
        .expect("shell command");

    assert!(shell_command.contains("EFFIGY_GATEWAY_ESCALATED='1'"));
    assert!(shell_command.contains("EFFIGY_INTERNAL_SUPPRESS_HEADER='1'"));
    assert!(shell_command.contains("HOME="));
    assert!(!shell_command.contains("PATH="));
    assert!(shell_command.contains("gateway setup-tls --json"));
}
