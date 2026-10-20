//! Service catalog and compose assembly for Effigy container environments.
//!
//! This crate provides:
//!
//! - Loading and parsing of service catalog fragments from ordered catalog
//!   sources: project, bundle, declared sources, compiled baseline.
//! - Fragment requirements (`[requires]`) checked against the running Effigy.
//! - Parameter schema validation against `service.toml` declarations.
//! - Template rendering via Jinja2-style syntax (minijinja).
//! - Compose assembly from rendered fragments into a complete
//!   `docker-compose.yml`.
//!
//! The crate is intentionally isolated — no dependency on other effigy domain
//! crates. Callers locate source directories; this crate only reads them.

pub mod assembly;
pub mod atomic_write;
pub mod error;
pub mod fragment;
pub mod requirements;
pub mod schema;
pub mod starter;
pub mod template;
pub mod volumes;

pub use assembly::ComposeAssembler;
pub use error::CatalogError;
pub use fragment::{
    service_name, split_qualified, validate_source_handle, CatalogFragment, CatalogResolver,
    FragmentInfo, FragmentSource, RESERVED_SOURCE_HANDLES,
};
pub use requirements::{
    current_effigy_version, FragmentRequirements, SUPPORTED_FRAGMENT_CAPABILITIES,
    SUPPORTED_FRAGMENT_SCHEMA,
};
pub use schema::{ParamSchema, ParamType, ServiceSchema};
pub use starter::{Starter, StarterError, StarterFile, StarterInfo, StarterResolver};
pub use template::TemplateRenderer;
