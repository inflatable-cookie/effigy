//! Catalog-pack acquisition: manifest, store, install transaction, selection,
//! and the fixed official channel model.
//!
//! Effigy's compiled catalog is the permanent baseline. A pack is an
//! independently versioned set of the same fragment directories that can be
//! installed into user state and selected *below* project and user overrides:
//!
//! ```text
//! project override > user override > active installed pack > compiled baseline
//! ```
//!
//! Nothing here runs during ordinary catalog use beyond reading local state.
//! Acquisition is explicit, digest-addressed for OCI, and always validated
//! before activation.

#[path = "pack/channel.rs"]
pub mod channel;
#[path = "pack/content.rs"]
pub mod content;
#[path = "pack/error.rs"]
pub mod error;
#[path = "pack/fallback.rs"]
pub mod fallback;
#[path = "pack/home.rs"]
pub mod home;
#[path = "pack/install.rs"]
pub mod install;
#[path = "pack/manifest.rs"]
pub mod manifest;
#[path = "pack/selection.rs"]
pub mod selection;
#[path = "pack/store.rs"]
pub mod store;

pub use channel::{
    official_update_reference, plan_official_update, OfficialPackChannel, OfficialUpdatePlan,
    OFFICIAL_PACK_CHANNEL, OFFICIAL_PACK_REPOSITORY,
};
pub use error::PackError;
pub use fallback::{set_diagnostic_mode, DiagnosticMode, FALLBACK_NOTICE_SCHEMA};
pub use home::{effigy_home_dir, with_test_effigy_home};
pub use install::{
    install_pack, LocalPackAcquirer, PackAcquireRequest, PackAcquisition, PackCandidateAcquirer,
    PackCandidateSource, PackInstallReport, StoredContentOutcome,
};
pub use manifest::{PackManifest, PACK_MANIFEST_FILE, SUPPORTED_PACK_MANIFEST_SCHEMA};
pub use selection::{
    layered_resolver, project_local_catalog_dir, resolve_catalog_layers, select_pack,
    select_pack_in, user_global_catalog_dir, CatalogLayers, PackSelection, PackSelectionReason,
    PROJECT_LOCAL_CATALOG_DIR,
};
pub use store::{
    InstalledPackRecord, PackSourceRecord, PackStore, PackStoreLock, PackStoreState,
    PACK_STORE_STATE_SCHEMA,
};

#[cfg(test)]
#[path = "pack/tests.rs"]
mod tests;
