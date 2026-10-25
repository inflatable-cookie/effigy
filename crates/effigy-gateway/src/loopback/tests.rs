use std::net::Ipv4Addr;

use super::*;
use crate::routes::{Route, RouteSource};

fn route(domain: &str, checkout: &str, dns_ip: Option<Ipv4Addr>, target: Option<&str>) -> Route {
    Route {
        domain: domain.to_owned(),
        target: target.map(str::to_owned),
        dns_ip,
        tcp_port: dns_ip.map(|_| 5432),
        tcp_target: target.map(str::to_owned),
        source: RouteSource::Container,
        project: checkout.to_owned(),
        scope: None,
        managed_listener: None,
        tls: false,
        registered: chrono::Utc::now(),
    }
}

fn table(routes: Vec<Route>) -> RouteTable {
    let mut table = RouteTable::new();
    for route in routes {
        table.upsert(route);
    }
    table
}

#[test]
fn the_first_checkout_takes_the_pool_start() {
    assert_eq!(
        claim_address(&RouteTable::new(), "/checkouts/one").expect("claim"),
        DEFAULT_LOOPBACK_START
    );
}

#[test]
fn another_checkouts_routes_reserve_their_addresses() {
    let table = table(vec![
        route(
            "a.test",
            "/checkouts/one",
            Some(Ipv4Addr::new(127, 1, 0, 1)),
            None,
        ),
        route(
            "b.test",
            "/checkouts/one",
            Some(Ipv4Addr::new(127, 1, 0, 1)),
            None,
        ),
        route(
            "c.test",
            "/checkouts/two",
            Some(Ipv4Addr::new(127, 1, 0, 2)),
            None,
        ),
    ]);
    assert_eq!(
        claim_address(&table, "/checkouts/three").expect("claim"),
        Ipv4Addr::new(127, 1, 0, 3)
    );
}

#[test]
fn a_route_target_reserves_its_address_too() {
    let table = table(vec![route(
        "a.test",
        "/checkouts/one",
        None,
        Some("127.1.0.1:5432"),
    )]);
    assert_eq!(
        claim_address(&table, "/checkouts/two").expect("claim"),
        Ipv4Addr::new(127, 1, 0, 2)
    );
}

#[test]
fn a_checkout_keeps_the_address_its_routes_answer_with() {
    let table = table(vec![route(
        "a.test",
        "/checkouts/one",
        Some(Ipv4Addr::new(127, 1, 0, 5)),
        None,
    )]);
    assert_eq!(
        checkout_address(&table, "/checkouts/one"),
        Some(Ipv4Addr::new(127, 1, 0, 5))
    );
    assert_eq!(
        claim_address(&table, "/checkouts/one").expect("claim"),
        Ipv4Addr::new(127, 1, 0, 5)
    );
    assert_eq!(checkout_address(&table, "/checkouts/two"), None);
}

#[test]
fn an_unreferenced_address_becomes_free_at_once() {
    let table = table(vec![
        route(
            "a.test",
            "/checkouts/one",
            Some(Ipv4Addr::new(127, 1, 0, 1)),
            None,
        ),
        route(
            "b.test",
            "/checkouts/two",
            Some(Ipv4Addr::new(127, 1, 0, 2)),
            None,
        ),
    ]);
    let mut table = table;
    let _ = table.deregister("a.test");
    assert_eq!(
        claim_address(&table, "/checkouts/three").expect("claim"),
        Ipv4Addr::new(127, 1, 0, 1)
    );
}

#[test]
fn an_exhausted_pool_names_the_range() {
    let routes = pool()
        .map(|ip| {
            route(
                &format!("{ip}.test"),
                &format!("/checkouts/{ip}"),
                Some(ip),
                None,
            )
        })
        .collect();
    let error = claim_address(&table(routes), "/checkouts/one").expect_err("exhausted pool");
    assert!(error
        .to_string()
        .contains(&format!("{DEFAULT_LOOPBACK_START}–{DEFAULT_LOOPBACK_END}")));
}
