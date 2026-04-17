use super::*;

#[test]
fn new_stats_are_zero() {
    let stats = GatewayStats::new();
    assert_eq!(stats.http_requests.load(Ordering::Relaxed), 0);
    assert_eq!(stats.dns_queries.load(Ordering::Relaxed), 0);
    assert_eq!(stats.proxied_requests.load(Ordering::Relaxed), 0);
}

#[test]
fn increment_counter() {
    let stats = GatewayStats::new();
    GatewayStats::inc(&stats.http_requests);
    GatewayStats::inc(&stats.http_requests);
    GatewayStats::inc(&stats.http_requests);
    assert_eq!(stats.http_requests.load(Ordering::Relaxed), 3);
}

#[test]
fn uptime_increases() {
    let stats = GatewayStats::new();
    std::thread::sleep(std::time::Duration::from_millis(10));
    assert!(stats.uptime_secs() > 0.0);
}

#[test]
fn json_has_expected_fields() {
    let stats = GatewayStats::new();
    GatewayStats::inc(&stats.dns_queries);
    GatewayStats::inc(&stats.dns_queries);
    GatewayStats::inc(&stats.proxied_requests);

    let json = stats.to_json();
    assert!(json["uptime_secs"].as_f64().unwrap() >= 0.0);
    assert_eq!(json["dns_queries"], 2);
    assert_eq!(json["proxied_requests"], 1);
    assert_eq!(json["http_requests"], 0);
}
