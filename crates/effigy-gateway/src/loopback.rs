//! Loopback alias addresses for gateway-served TCP service aliases.
//!
//! The route table is the only record of loopback ownership: an address is in
//! use exactly when a route references it, as its DNS answer or as a target.
//! A checkout's routes share one address, and `down` frees it by removing
//! them, so there is no separate assignment to record, reclaim or release.

use std::collections::BTreeSet;
use std::net::{Ipv4Addr, SocketAddr};

use crate::error::GatewayError;
use crate::routes::RouteTable;

/// First assignable loopback IP in the bounded pool.
pub const DEFAULT_LOOPBACK_START: Ipv4Addr = Ipv4Addr::new(127, 1, 0, 1);

/// Last assignable loopback IP in the bounded pool.
pub const DEFAULT_LOOPBACK_END: Ipv4Addr = Ipv4Addr::new(127, 1, 0, 50);

/// The bounded pool, lowest address first.
pub fn pool() -> impl Iterator<Item = Ipv4Addr> {
    let [a, b, c, _] = DEFAULT_LOOPBACK_START.octets();
    (DEFAULT_LOOPBACK_START.octets()[3]..=DEFAULT_LOOPBACK_END.octets()[3])
        .map(move |octet| Ipv4Addr::new(a, b, c, octet))
}

/// Every address some route already references, as a DNS answer or a target.
pub fn addresses_in_use(table: &RouteTable) -> BTreeSet<Ipv4Addr> {
    let mut in_use = BTreeSet::new();
    for route in table.all_routes() {
        if let Some(ip) = route.dns_ip {
            in_use.insert(ip);
        }
        for target in [route.target.as_deref(), route.tcp_target.as_deref()]
            .into_iter()
            .flatten()
        {
            if let Ok(SocketAddr::V4(address)) = target.parse::<SocketAddr>() {
                in_use.insert(*address.ip());
            }
        }
    }
    in_use
}

/// The pool address this checkout's routes already answer with, if any.
pub fn checkout_address(table: &RouteTable, checkout: &str) -> Option<Ipv4Addr> {
    table
        .all_routes()
        .into_iter()
        .filter(|route| route.project == checkout)
        .filter_map(|route| route.dns_ip)
        .filter(|ip| pool().any(|candidate| candidate == *ip))
        .min()
}

/// The address this checkout's routes use, or the lowest pool address no
/// route references.
pub fn claim_address(table: &RouteTable, checkout: &str) -> Result<Ipv4Addr, GatewayError> {
    if let Some(address) = checkout_address(table, checkout) {
        return Ok(address);
    }
    let in_use = addresses_in_use(table);
    pool().find(|candidate| !in_use.contains(candidate)).ok_or(
        GatewayError::LoopbackPoolExhausted {
            range_start: DEFAULT_LOOPBACK_START,
            range_end: DEFAULT_LOOPBACK_END,
        },
    )
}

#[cfg(test)]
#[path = "loopback/tests.rs"]
mod tests;
