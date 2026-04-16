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
//! Route registration is file-based: container lifecycle events update a
//! JSON route table at `~/.effigy/gateway/routes.json`, and the gateway
//! watches for changes.
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

pub mod dns;
pub mod error;
pub mod ports;
pub mod proxy;
pub mod registration;
pub mod resolver_setup;
pub mod routes;
pub mod server;
pub mod tls;

pub use error::GatewayError;
pub use routes::{Route, RouteTable};
pub use server::GatewayConfig;
