//! DNS resolver and reverse proxy gateway for Effigy local development.
//!
//! The gateway provides two services:
//!
//! - **DNS resolver**: Responds to `*.test` (or configured TLD) queries with
//!   `127.0.0.1`, enabling local project domains like `myproject.test`.
//!
//! - **Reverse proxy**: Routes HTTP requests by `Host` header to the correct
//!   project port, so `http://myproject.test` reaches `localhost:8080`.
//!
//! The gateway runs as a host-native background process, not a container.
//! Container and managed-listener registration publish transactional endpoint
//! claims in the kernel Store. The gateway reloads committed snapshots.
//!
//! ## Architecture
//!
//! ```text
//! Browser -> DNS query for myproject.test
//!         -> gateway DNS (port 15353) -> 127.0.0.1
//!
//! Browser -> HTTP request Host: myproject.test
//!         -> gateway proxy (port 80) -> localhost:8080
//! ```

mod atomic_write;
mod locks;
#[cfg(target_os = "macos")]
mod macos_socket;

#[cfg(any(target_os = "macos", test))]
pub mod bind_broker;
pub mod dns;
pub mod endpoint_store;
pub mod error;
pub mod identity;
pub mod linux_operator;
pub mod listener_handoff;
pub mod loopback;
pub mod operator;
pub mod ports;
pub mod private_state;
pub mod proxy;
pub mod registration;
pub mod resolver_setup;
pub mod routes;
pub mod server;
pub mod setup;
pub mod socket_ownership;
pub mod stats;
pub mod tcp_alias;
pub mod tls;
pub mod transition_lock;

pub use error::GatewayError;
pub use routes::{Route, RouteTable};
pub use server::{GatewayConfig, GatewayMode, PrivateGatewayPolicy};

#[cfg(test)]
#[path = "../../../tests/support/scoped_env.rs"]
mod env_test_support;

#[cfg(test)]
#[path = "../../../tests/support/route_table_fixture.rs"]
mod route_table_fixture;

#[cfg(test)]
extern crate self as effigy_gateway;
