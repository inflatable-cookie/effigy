//! Container execution context routing and CWD mapping for Effigy.
//!
//! This crate provides the pure logic layer for transparent container
//! execution — deciding whether a command should run in a container or on
//! the host, mapping host paths to container paths, and resolving exec
//! aliases.
//!
//! It operates on simple input types (paths, config structs, command
//! strings) and has no dependency on other effigy crates. Integration
//! into the manifest schema and CLI happens separately.
//!
//! ## Core concepts
//!
//! - **Execution context**: A container marked as `context = "dev"` becomes
//!   the implicit execution environment for the project.
//! - **Routing decision**: Given a command and context, determine whether
//!   it should run on the host or in the container.
//! - **CWD mapping**: Translate a host working directory to the
//!   corresponding container path.
//! - **Exec aliases**: Named shortcuts for running specific commands in
//!   specific services (e.g., `mysql` → `docker compose exec db mysql`).

pub mod alias;
pub mod cwd;
pub mod detection;
pub mod error;
pub mod health;
pub mod routing;

pub use alias::{ExecAlias, ExecAliasTable};
pub use cwd::CwdMapper;
pub use detection::{CapabilityCache, ContainerCapabilities, ExecStrategy};
pub use error::ExecError;
pub use health::{HealthCheck, HealthCheckConfig, HealthPoller, HealthState};
pub use routing::{ExecContext, ExecTarget, RoutingDecision};
