//! Service catalog and compose assembly for Effigy container environments.
//!
//! This crate provides:
//!
//! - Loading and parsing of service catalog fragments (bundled, user-global,
//!   project-local).
//! - Parameter schema validation against `service.toml` declarations.
//! - Template rendering via Jinja2-style syntax (minijinja).
//! - Compose assembly from rendered fragments into a complete
//!   `docker-compose.yml`.
//!
//! The crate is intentionally isolated — no dependency on other effigy domain
//! crates. Integration into the main runner happens separately.

pub mod assembly;
pub mod error;
pub mod fragment;
pub mod output;
pub mod schema;
pub mod template;

pub use assembly::ComposeAssembler;
pub use error::CatalogError;
pub use fragment::{CatalogFragment, CatalogResolver};
pub use output::ComposeOutput;
pub use schema::{ParamSchema, ParamType, ServiceSchema};
pub use template::TemplateRenderer;
