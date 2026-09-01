//! Service catalog and compose assembly for Effigy container environments.
//!
//! This crate provides:
//!
//! - Loading and parsing of service catalog fragments (bundled, installed
//!   pack, user-global, project-local).
//! - Parameter schema validation against `service.toml` declarations.
//! - Template rendering via Jinja2-style syntax (minijinja).
//! - Compose assembly from rendered fragments into a complete
//!   `docker-compose.yml`.
//! - Local validation of Effigy's catalog-pack update support floor. That
//!   parser is not part of pack selection, acquisition, or activation.
//!
//! The crate is intentionally isolated — no dependency on other effigy domain
//! crates. Integration into the main runner happens separately.

pub mod assembly;
pub mod error;
pub mod fragment;
pub mod output;
pub mod pack;
pub mod schema;
pub mod starter;
pub mod support_policy;
pub mod template;
pub mod volumes;

pub use assembly::ComposeAssembler;
pub use error::CatalogError;
pub use fragment::{CatalogFragment, CatalogResolver, FragmentSource, InstalledPackLayer};
pub use output::ComposeOutput;
pub use pack::{
    install_pack, resolve_catalog_layers, select_pack, CatalogLayers, InstalledPackRecord,
    OfficialPackChannel, PackCandidateAcquirer, PackCandidateSource, PackError, PackManifest,
    PackSelection, PackSelectionReason, PackSourceRecord, PackStore, PackStoreState,
};
pub use schema::{ParamSchema, ParamType, ServiceSchema};
pub use starter::{Starter, StarterError, StarterFile, StarterInfo, StarterResolver};
pub use support_policy::{
    current_effigy_release, CatalogPackUpdatePolicy, PackUpdateCapability, SupportPolicyError,
    CATALOG_PACK_UPDATE_POLICY_FILE, SUPPORTED_CATALOG_PACK_UPDATE_SCHEMA,
};
pub use template::TemplateRenderer;
