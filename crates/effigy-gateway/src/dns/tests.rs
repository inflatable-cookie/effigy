use super::*;
use hickory_proto::op::{Message, MessageType};
use hickory_proto::rr::RecordType;
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use std::str::FromStr;

use hickory_proto::rr::Name;

fn test_config() -> DnsConfig {
    DnsConfig::default()
}

fn build_query(domain: &str, record_type: RecordType) -> Vec<u8> {
    let mut msg = Message::new(1234, MessageType::Query, OpCode::Query);
    msg.metadata.recursion_desired = true;

    let name = Name::from_str(domain).unwrap();
    msg.add_query(hickory_proto::op::Query::query(name, record_type));

    msg.to_bytes().unwrap()
}

fn route_table_with(domain: &str) -> Arc<RwLock<RouteTable>> {
    let mut table = RouteTable::new();
    table.upsert(crate::routes::Route {
        domain: domain.to_string(),
        target: Some("127.0.0.1:8080".to_string()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        source: crate::routes::RouteSource::Container,
        project: "/tmp/test".to_string(),
        tls: false,
        managed_listener: None,
        registered: chrono::Utc::now(),
    });
    Arc::new(RwLock::new(table))
}

fn route_table_with_dns_ip(domain: &str, dns_ip: Ipv4Addr) -> Arc<RwLock<RouteTable>> {
    let mut table = RouteTable::new();
    table.upsert(crate::routes::Route {
        domain: domain.to_string(),
        target: Some("127.0.0.1:8080".to_string()),
        dns_ip: Some(dns_ip),
        tcp_port: None,
        tcp_target: None,
        source: crate::routes::RouteSource::Container,
        project: "/tmp/test".to_string(),
        tls: false,
        managed_listener: None,
        registered: chrono::Utc::now(),
    });
    Arc::new(RwLock::new(table))
}

#[test]
fn resolves_registered_domain() {
    let config = test_config();
    let table = route_table_with("myapp.test");
    let query = build_query("myapp.test.", RecordType::A);

    let cache = DnsCache::new(Duration::from_secs(2));
    let (response_bytes, resolved) = handle_dns_query(&query, &config, &table, &cache);
    assert!(resolved, "should report as resolved route");
    let response = Message::from_bytes(&response_bytes.unwrap()).unwrap();

    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert_eq!(response.answers.len(), 1);

    let answer = &response.answers[0];
    assert_eq!(answer.record_type(), RecordType::A);
    match &answer.data {
        RData::A(a) => assert_eq!(a.0, Ipv4Addr::LOCALHOST),
        other => panic!("expected A record, got {other:?}"),
    }
}

#[test]
fn resolves_registered_domain_to_route_specific_dns_ip() {
    let config = test_config();
    let table = route_table_with_dns_ip("db.myapp.test", Ipv4Addr::new(127, 1, 0, 7));
    let query = build_query("db.myapp.test.", RecordType::A);

    let cache = DnsCache::new(Duration::from_secs(2));
    let (response_bytes, resolved) = handle_dns_query(&query, &config, &table, &cache);
    assert!(resolved, "should report as resolved route");
    let response = Message::from_bytes(&response_bytes.unwrap()).unwrap();

    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert_eq!(response.answers.len(), 1);

    let answer = &response.answers[0];
    match &answer.data {
        RData::A(a) => assert_eq!(a.0, Ipv4Addr::new(127, 1, 0, 7)),
        other => panic!("expected A record, got {other:?}"),
    }
}

#[test]
fn resolves_unregistered_tld_domain() {
    let config = test_config();
    let table = Arc::new(RwLock::new(RouteTable::new()));
    let query = build_query("unknown.test.", RecordType::A);

    let cache = DnsCache::new(Duration::from_secs(2));
    let (response_bytes, resolved) = handle_dns_query(&query, &config, &table, &cache);
    assert!(
        !resolved,
        "unregistered domain should not count as resolved"
    );
    let response = Message::from_bytes(&response_bytes.unwrap()).unwrap();

    // Should still resolve (proxy will show error page).
    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert_eq!(response.answers.len(), 1);
}

#[test]
fn refuses_non_tld_domain() {
    let config = test_config();
    let table = route_table_with("myapp.test");
    let query = build_query("google.com.", RecordType::A);

    let cache = DnsCache::new(Duration::from_secs(2));
    let (response_bytes, resolved) = handle_dns_query(&query, &config, &table, &cache);
    assert!(!resolved);
    let response = Message::from_bytes(&response_bytes.unwrap()).unwrap();

    assert_eq!(response.metadata.response_code, ResponseCode::Refused);
    assert_eq!(response.answers.len(), 0);
}

#[test]
fn resolves_registered_non_tld_domain() {
    // Public-domain route declared by a manifest (e.g. via `target_host`
    // in cumberland) should be answered locally, not refused. Without
    // this, browsers fall back to upstream DNS and bypass the gateway.
    let config = test_config();
    let table = route_table_with("dev.cumberland.co.uk");
    let query = build_query("dev.cumberland.co.uk.", RecordType::A);

    let cache = DnsCache::new(Duration::from_secs(2));
    let (response_bytes, resolved) = handle_dns_query(&query, &config, &table, &cache);
    assert!(resolved, "registered non-tld domain should resolve");
    let response = Message::from_bytes(&response_bytes.unwrap()).unwrap();

    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert_eq!(response.answers.len(), 1);
    match &response.answers[0].data {
        RData::A(a) => assert_eq!(a.0, Ipv4Addr::LOCALHOST),
        other => panic!("expected A record, got {other:?}"),
    }
}

#[test]
fn registered_non_tld_aaaa_returns_noerror_not_refused() {
    // Browsers run AAAA in parallel with A. If we Refuse the AAAA
    // query, they may retry through upstream DNS and reach the public
    // record, bypassing the local gateway. Returning NoError-with-no-
    // answers keeps the lookup pinned to us.
    let config = test_config();
    let table = route_table_with("dev.cumberland.co.uk");
    let query = build_query("dev.cumberland.co.uk.", RecordType::AAAA);

    let cache = DnsCache::new(Duration::from_secs(2));
    let (response_bytes, _) = handle_dns_query(&query, &config, &table, &cache);
    let response = Message::from_bytes(&response_bytes.unwrap()).unwrap();

    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert_eq!(response.answers.len(), 0);
}

#[test]
fn ignores_non_a_queries() {
    let config = test_config();
    let table = route_table_with("myapp.test");
    let query = build_query("myapp.test.", RecordType::AAAA);

    let cache = DnsCache::new(Duration::from_secs(2));
    let (response_bytes, resolved) = handle_dns_query(&query, &config, &table, &cache);
    assert!(!resolved, "AAAA query should not count as resolved");
    let response = Message::from_bytes(&response_bytes.unwrap()).unwrap();

    // AAAA query for .test domain — no A answers, but NoError
    // (not Refused) to prevent slow dual-stack browser lookups.
    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert_eq!(response.answers.len(), 0);
}

#[test]
fn response_preserves_query_id() {
    let config = test_config();
    let table = route_table_with("myapp.test");
    let query = build_query("myapp.test.", RecordType::A);

    let cache = DnsCache::new(Duration::from_secs(2));
    let (response_bytes, _) = handle_dns_query(&query, &config, &table, &cache);
    let response = Message::from_bytes(&response_bytes.unwrap()).unwrap();

    assert_eq!(response.metadata.id, 1234);
}

// ── Cache tests ──────────────────────────────────────────────────

/// A cached answer keeps both halves of what the lookup decided: whether the
/// name resolved, and the route's own DNS address when it declares one.
#[test]
fn cache_stores_and_retrieves_the_answer_and_its_route_address() {
    let cache = DnsCache::new(Duration::from_secs(60));
    assert!(cache.get("myapp.test").is_none());

    let cases = [
        ("myapp.test", true, None),
        ("other.test", false, None),
        ("db.myapp.test", true, Some(Ipv4Addr::new(127, 1, 0, 7))),
    ];
    for (domain, resolved, dns_ip) in cases {
        cache.put(domain.to_string(), resolved, dns_ip);
        assert_eq!(
            cache.get(domain),
            Some((resolved, dns_ip)),
            "domain {domain}"
        );
    }
}

#[test]
fn cache_expires() {
    let cache = DnsCache::new(Duration::from_millis(1));
    cache.put("myapp.test".to_string(), true, None);

    std::thread::sleep(Duration::from_millis(10));
    assert!(cache.get("myapp.test").is_none());
}

#[test]
fn cache_clear_invalidates_all() {
    let cache = DnsCache::new(Duration::from_secs(60));
    cache.put("a.test".to_string(), true, None);
    cache.put("b.test".to_string(), false, None);

    cache.clear();
    assert!(cache.get("a.test").is_none());
    assert!(cache.get("b.test").is_none());
}

/// A route's own DNS address never outlives its route: a reload clears the
/// cache, so a withdrawn domain falls back to the loopback answer instead of
/// the address the closed route named.
#[test]
fn a_withdrawn_route_stops_resolving_after_the_reload_clears_the_cache() {
    let config = test_config();
    let dns_ip = Ipv4Addr::new(127, 1, 0, 7);
    let table = route_table_with_dns_ip("db.myapp.test", dns_ip);
    let query = build_query("db.myapp.test.", RecordType::A);
    let cache = DnsCache::new(Duration::from_secs(60));

    let (response_bytes, resolved) = handle_dns_query(&query, &config, &table, &cache);
    assert!(resolved);
    assert_eq!(cache.get("db.myapp.test"), Some((true, Some(dns_ip))));
    let response = Message::from_bytes(&response_bytes.unwrap()).unwrap();
    match &response.answers[0].data {
        RData::A(a) => assert_eq!(a.0, dns_ip),
        other => panic!("expected A record, got {other:?}"),
    }

    // The watcher installs a table that no longer names the domain and clears
    // the cache in the same step.
    let withdrawn = Arc::new(RwLock::new(RouteTable::new()));
    cache.clear();
    assert!(cache.get("db.myapp.test").is_none());

    let (response_bytes, resolved) = handle_dns_query(&query, &config, &withdrawn, &cache);
    assert!(!resolved, "a withdrawn domain must not count as resolved");
    let response = Message::from_bytes(&response_bytes.unwrap()).unwrap();
    match &response.answers[0].data {
        RData::A(a) => assert_eq!(
            a.0,
            Ipv4Addr::LOCALHOST,
            "falls back to loopback, not the withdrawn route's address"
        ),
        other => panic!("expected A record, got {other:?}"),
    }
}
