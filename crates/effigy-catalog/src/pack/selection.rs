//! Catalog layer selection: project override, user override, active installed
//! pack, compiled baseline — in that order.
//!
//! This is the only place layer paths are computed. Every catalog-backed
//! command goes through [`resolve_catalog_layers`], so there is one selection
//! implementation and one place where an unhealthy active pack turns into a
//! visible baseline fallback.
//!
//! Nothing here touches the network. Selection reads local state only; the OCI
//! seam is reachable exclusively from an explicit `service pack install`.

use std::path::{Path, PathBuf};

use super::home::effigy_home_dir;
use super::store::{InstalledPackRecord, PackStore};
use super::verify::{verify_installed_pack, PackDefect};
use crate::fragment::{CatalogResolver, InstalledPackLayer};

/// Project-local override directory, relative to the repo root.
pub const PROJECT_LOCAL_CATALOG_DIR: &str = "infra/dev/catalog";

/// User-global override directory, relative to the Effigy user-state home.
pub const USER_GLOBAL_CATALOG_DIR: &str = "catalog";

/// Why the active catalog layer below the overrides was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackSelectionReason {
    /// No installed-pack store exists on this machine.
    NoStore,
    /// A store exists but the compiled baseline is selected.
    NoActivePack,
    /// A healthy installed pack is active.
    ActivePack,
    /// Store state exists but could not be read; baseline selected.
    FallbackStoreUnreadable,
    /// Store state names an install it has no record for; baseline selected.
    FallbackStateCorrupt,
    /// The active install's content is missing; baseline selected.
    FallbackMissingContent,
    /// The active install's manifest no longer parses; baseline selected.
    FallbackInvalidManifest,
    /// Stored content no longer matches its recorded identity; baseline
    /// selected.
    FallbackContentChanged,
    /// The stored manifest disagrees with the record describing it; baseline
    /// selected.
    FallbackRecordMismatch,
    /// The active install's fragments no longer validate; baseline selected.
    FallbackInvalidPack,
    /// The active install no longer accepts this Effigy; baseline selected.
    FallbackIncompatible,
}

impl PackSelectionReason {
    /// Stable machine-readable label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NoStore => "no-store",
            Self::NoActivePack => "no-active-pack",
            Self::ActivePack => "active-pack",
            Self::FallbackStoreUnreadable => "fallback-store-unreadable",
            Self::FallbackStateCorrupt => "fallback-state-corrupt",
            Self::FallbackMissingContent => "fallback-missing-content",
            Self::FallbackInvalidManifest => "fallback-invalid-manifest",
            Self::FallbackContentChanged => "fallback-content-changed",
            Self::FallbackRecordMismatch => "fallback-record-mismatch",
            Self::FallbackInvalidPack => "fallback-invalid-pack",
            Self::FallbackIncompatible => "fallback-incompatible",
        }
    }

    /// Whether this reason represents a fallback away from an active pack.
    pub fn is_fallback(&self) -> bool {
        matches!(
            self,
            Self::FallbackStoreUnreadable
                | Self::FallbackStateCorrupt
                | Self::FallbackMissingContent
                | Self::FallbackInvalidManifest
                | Self::FallbackContentChanged
                | Self::FallbackRecordMismatch
                | Self::FallbackInvalidPack
                | Self::FallbackIncompatible
        )
    }
}

/// The resolved catalog layer below the project and user overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSelection {
    /// Why this layer was chosen.
    pub reason: PackSelectionReason,
    /// The active install, when a healthy pack is selected.
    pub active: Option<InstalledPackRecord>,
    /// Structured detail for a fallback, suitable for text and JSON output.
    pub detail: Option<String>,
    /// Store root, when a store handle could be resolved at all.
    pub store_root: Option<PathBuf>,
}

impl PackSelection {
    /// Whether the compiled baseline is in use.
    pub fn uses_baseline(&self) -> bool {
        self.active.is_none()
    }

    /// Human-readable warning for a visible fallback.
    pub fn fallback_warning(&self) -> Option<String> {
        if !self.reason.is_fallback() {
            return None;
        }
        let detail = self.detail.clone().unwrap_or_else(|| "unknown".to_owned());
        Some(format!(
            "[warn] active catalog pack is unhealthy ({}); using the compiled baseline. {detail} \
             Repair with `effigy service pack rollback` or `effigy service pack reset`.",
            self.reason.as_str()
        ))
    }
}

/// A resolver plus the selection facts that produced it.
pub struct CatalogLayers {
    /// Resolver wired with every selected layer.
    pub resolver: CatalogResolver,
    /// Which layer sits below the overrides, and why.
    pub selection: PackSelection,
}

/// Resolve every catalog layer for `repo_root` against the running Effigy,
/// using the default override directories and the user pack store.
pub fn resolve_catalog_layers(repo_root: Option<&Path>, effigy_version: &str) -> CatalogLayers {
    layered_resolver(
        project_local_catalog_dir(repo_root),
        user_global_catalog_dir(),
        PackStore::user().as_ref(),
        effigy_version,
    )
}

/// Resolve every catalog layer from explicit override directories and store.
///
/// Callers that own their own `~/.effigy` resolution use this so layer order
/// still has exactly one implementation while home discovery stays theirs.
pub fn layered_resolver(
    project_local: Option<PathBuf>,
    user_global: Option<PathBuf>,
    store: Option<&PackStore>,
    effigy_version: &str,
) -> CatalogLayers {
    let selection = select_pack_in(store, effigy_version);
    let layer = selection.active.as_ref().map(|record| InstalledPackLayer {
        root: install_root(&selection, record),
        pack_id: record.pack_id.clone(),
        pack_version: record.pack_version.clone(),
    });
    let resolver = CatalogResolver::new(project_local, user_global).with_installed_pack(layer);
    // One boundary, one notice: every catalog-backed consumer passes through
    // here, so none of them can silently swap pack content for baseline
    // content without the operator being told.
    super::fallback::report_once(&selection);
    CatalogLayers {
        resolver,
        selection,
    }
}

/// Project-local override directory for `repo_root`, when present.
pub fn project_local_catalog_dir(repo_root: Option<&Path>) -> Option<PathBuf> {
    let path = repo_root?.join(PROJECT_LOCAL_CATALOG_DIR);
    path.is_dir().then_some(path)
}

/// User-global override directory, when present.
pub fn user_global_catalog_dir() -> Option<PathBuf> {
    let path = effigy_home_dir()?.join(USER_GLOBAL_CATALOG_DIR);
    path.is_dir().then_some(path)
}

/// Choose between the active installed pack and the compiled baseline.
///
/// Every failure mode degrades to the baseline with a recorded reason. A
/// broken pack must never make ordinary catalog use fail.
pub fn select_pack(effigy_version: &str) -> PackSelection {
    select_pack_in(PackStore::user().as_ref(), effigy_version)
}

/// Choose between an explicit store's active pack and the compiled baseline.
pub fn select_pack_in(store: Option<&PackStore>, effigy_version: &str) -> PackSelection {
    let Some(store) = store else {
        return baseline(PackSelectionReason::NoStore, None, None);
    };
    let store_root = Some(store.root().to_path_buf());
    if !store.exists() {
        return baseline(PackSelectionReason::NoStore, None, store_root);
    }

    let state = match store.load() {
        Ok(state) => state,
        Err(error) => {
            return baseline(
                PackSelectionReason::FallbackStoreUnreadable,
                Some(error.to_string()),
                store_root,
            )
        }
    };

    // A selection pointer with no record behind it is corruption. Treating it
    // as "nothing installed" would hide a damaged store behind a healthy-
    // looking baseline.
    let broken = state.broken_cross_references();
    if !broken.is_empty() {
        return baseline(
            PackSelectionReason::FallbackStateCorrupt,
            Some(format!(
                "store state names unknown install(s): {}",
                broken.join(", ")
            )),
            store_root,
        );
    }

    let Some(record) = state.active_record().cloned() else {
        return baseline(PackSelectionReason::NoActivePack, None, store_root);
    };

    let root = store.install_dir(&record.install_id);
    match verify_active_install(&root, &record, effigy_version) {
        Ok(()) => PackSelection {
            reason: PackSelectionReason::ActivePack,
            active: Some(record),
            detail: None,
            store_root,
        },
        Err((reason, detail)) => baseline(reason, Some(detail), store_root),
    }
}

/// Prove the stored bytes still are what the record says they are.
///
/// Reloading `pack.toml` and checking compatibility is not enough: a deleted
/// compose fragment, an edited config file, or a swapped manifest identity all
/// leave a parseable manifest behind. Selection therefore re-runs the same
/// validation an install candidate faces, cross-checks the manifest against the
/// record, and re-hashes the whole tree against the recorded content identity.
///
/// A pack is the same shape and order of size as the compiled baseline (~200 KB
/// across ~50 small files), so a full re-hash costs well under a millisecond —
/// cheap enough to pay on every resolver construction rather than introduce a
/// cache whose invalidation would itself be a correctness surface.
fn verify_active_install(
    root: &Path,
    record: &InstalledPackRecord,
    effigy_version: &str,
) -> Result<(), (PackSelectionReason, String)> {
    verify_installed_pack(root, record, effigy_version)
        .map_err(|failure| (reason_for(failure.defect), failure.detail))
}

/// Map a verification defect onto the selection reason operators see.
pub(super) fn reason_for(defect: PackDefect) -> PackSelectionReason {
    match defect {
        PackDefect::MissingContent => PackSelectionReason::FallbackMissingContent,
        PackDefect::InvalidManifest => PackSelectionReason::FallbackInvalidManifest,
        PackDefect::Incompatible => PackSelectionReason::FallbackIncompatible,
        PackDefect::InvalidPack => PackSelectionReason::FallbackInvalidPack,
        PackDefect::RecordMismatch => PackSelectionReason::FallbackRecordMismatch,
        PackDefect::ContentChanged => PackSelectionReason::FallbackContentChanged,
    }
}

fn install_root(selection: &PackSelection, record: &InstalledPackRecord) -> PathBuf {
    selection
        .store_root
        .as_ref()
        .map(|root| root.join("installs").join(&record.install_id))
        .unwrap_or_default()
}

fn baseline(
    reason: PackSelectionReason,
    detail: Option<String>,
    store_root: Option<PathBuf>,
) -> PackSelection {
    PackSelection {
        reason,
        active: None,
        detail,
        store_root,
    }
}
