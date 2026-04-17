use super::*;
use hickory_proto::op::Message;
use hickory_proto::rr::RecordType;
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use std::str::FromStr;

use hickory_proto::rr::Name;

fn test_config() -> DnsConfig {
    DnsConfig::default()
}

fn build_query(domain: &str, record_type: RecordType) -> Vec<u8> {
    let mut msg = Message::new();
    msg.set_id(1234);
    msg.set_message_type(MessageType::Query);
    msg.set_op_code(OpCode::Query);
    msg.set_recursion_desired(true);

    let name = Name::from_str(domain).unwrap();
    msg.add_query(hickory_proto::op::Query::query(name, record_type));

    msg.to_bytes().unwrap()
}

fn route_table_with(domain: &str) -> Arc<RwLock<RouteTable>> {
    let mut table = RouteTable::new();
    table.upsert(crate::routes::Route {
        domain: domain.to_string(),
        target: "127.0.0.1:8080".to_string(),
        source: crate::routes::RouteSource::Container,
        project: "/tmp/test".to_string(),
        tls: false,
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

    assert_eq!(response.response_code(), ResponseCode::NoError);
    assert_eq!(response.answers().len(), 1);

    let answer = &response.answers()[0];
    assert_eq!(answer.record_type(), RecordType::A);
    match answer.data() {
        RData::A(a) => assert_eq!(a.0, Ipv4Addr::LOCALHOST),
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
    assert_eq!(response.response_code(), ResponseCode::NoError);
    assert_eq!(response.answers().len(), 1);
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

    assert_eq!(response.response_code(), ResponseCode::Refused);
    assert_eq!(response.answers().len(), 0);
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
    assert_eq!(response.response_code(), ResponseCode::NoError);
    assert_eq!(response.answers().len(), 0);
}

#[test]
fn response_preserves_query_id() {
    let config = test_config();
    let table = route_table_with("myapp.test");
    let query = build_query("myapp.test.", RecordType::A);

    let cache = DnsCache::new(Duration::from_secs(2));
    let (response_bytes, _) = handle_dns_query(&query, &config, &table, &cache);
    let response = Message::from_bytes(&response_bytes.unwrap()).unwrap();

    assert_eq!(response.id(), 1234);
}

// ── Cache tests ──────────────────────────────────────────────────

#[test]
fn cache_stores_and_retrieves() {
    let cache = DnsCache::new(Duration::from_secs(60));
    assert!(cache.get("myapp.test").is_none());

    cache.put("myapp.test".to_string(), true);
    assert_eq!(cache.get("myapp.test"), Some(true));

    cache.put("other.test".to_string(), false);
    assert_eq!(cache.get("other.test"), Some(false));
}

#[test]
fn cache_expires() {
    let cache = DnsCache::new(Duration::from_millis(1));
    cache.put("myapp.test".to_string(), true);

    std::thread::sleep(Duration::from_millis(10));
    assert!(cache.get("myapp.test").is_none());
}

#[test]
fn cache_clear_invalidates_all() {
    let cache = DnsCache::new(Duration::from_secs(60));
    cache.put("a.test".to_string(), true);
    cache.put("b.test".to_string(), false);

    cache.clear();
    assert!(cache.get("a.test").is_none());
    assert!(cache.get("b.test").is_none());
}

#[test]
fn cached_lookup_avoids_route_table() {
    let config = test_config();
    let table = route_table_with("myapp.test");
    let query = build_query("myapp.test.", RecordType::A);

    let cache = DnsCache::new(Duration::from_secs(60));

    // First query populates the cache.
    let (resp1, resolved1) = handle_dns_query(&query, &config, &table, &cache);
    assert!(resolved1);
    assert!(resp1.is_some());

    // Verify the cache was populated.
    assert_eq!(cache.get("myapp.test"), Some(true));

    // Second query should use the cache (even if we clear the
    // route table, the cached result is still valid).
    let empty_table = Arc::new(RwLock::new(RouteTable::new()));
    let (resp2, resolved2) = handle_dns_query(&query, &config, &empty_table, &cache);
    assert!(resolved2, "should resolve from cache");
    assert!(resp2.is_some());
}
