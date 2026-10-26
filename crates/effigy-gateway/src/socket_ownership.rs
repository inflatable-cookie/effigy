//! Process-owned listening sockets, as inspected for a handed-over descriptor.
//!
//! Only the listener handoff reads these: `macos_socket` reports one
//! descriptor's own endpoint, and no caller enumerates every listener a
//! process owns. A managed route no longer scans other processes for socket
//! ownership (Q-011: no second record of runtime state).

use std::net::SocketAddr;

/// Transport for one gateway role endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GatewayTransport {
    /// UDP listener (DNS).
    Udp,
    /// TCP listener (HTTP/HTTPS).
    Tcp,
}

/// One process-owned listening endpoint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GatewayEndpoint {
    /// UDP or TCP.
    pub transport: GatewayTransport,
    /// Local bind address.
    pub addr: SocketAddr,
}
