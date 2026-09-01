//! Versioned user-state store for installed catalog packs.
//!
//! Layout under the Effigy user-state home:
//!
//! ```text
//! ~/.effigy/catalog-packs/v1/
//!   state.json          # active/previous selection plus install lineage
//!   installs/<id>/      # immutable installed pack content
//!   staging/<id>/       # transient candidate work area
//! ```
//!
//! `state.json` is the only activation authority. It is rewritten by writing a
//! sibling temp file and renaming over the target, so a reader either sees the
//! whole previous selection or the whole new one — never a partial flip.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::PackError;
use super::home::effigy_home_dir;

/// Store directory under the Effigy user-state home.
const STORE_DIR: &str = "catalog-packs";

/// On-disk store layout version. A future layout lands beside this one rather
/// than rewriting it in place.
const STORE_LAYOUT_VERSION: &str = "v1";

/// Schema identifier of the persisted state document.
pub const PACK_STORE_STATE_SCHEMA: &str = "effigy.catalog-pack.store.v1";

/// Installs retained beyond the active selection. Keeps one deterministic
/// rollback target plus one spare so a rollback followed by a reinstall still
/// has recoverable lineage.
const MAX_RETAINED_INSTALLS: usize = 3;

/// Where an installed pack came from, with its immutable content identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PackSourceRecord {
    /// Digest-addressed OCI artifact.
    Oci {
        /// Redacted `oci://` reference as supplied by the operator.
        reference: String,
        /// Resolved immutable registry digest.
        digest: String,
    },
    /// Explicitly operator-selected local directory.
    Local {
        /// Absolute path the candidate was read from at install time.
        path: PathBuf,
    },
}

impl PackSourceRecord {
    /// Short kind label used in text and JSON output.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Oci { .. } => "oci",
            Self::Local { .. } => "local",
        }
    }

    /// Operator-facing description of the source.
    pub fn display(&self) -> String {
        match self {
            Self::Oci { reference, .. } => reference.clone(),
            Self::Local { path } => path.display().to_string(),
        }
    }

    /// Resolved registry digest, when the source was an OCI artifact.
    pub fn digest(&self) -> Option<&str> {
        match self {
            Self::Oci { digest, .. } => Some(digest),
            Self::Local { .. } => None,
        }
    }
}

/// One installed pack recorded in the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPackRecord {
    /// Store-unique, content-addressed install identifier.
    pub install_id: String,
    /// Pack identity from the validated manifest.
    pub pack_id: String,
    /// Pack version from the validated manifest.
    pub pack_version: String,
    /// Manifest schema version the pack declared.
    pub manifest_schema_version: u32,
    /// Effigy compatibility requirement the pack declared.
    pub requires_effigy: String,
    /// Acquisition source plus immutable source identity.
    pub source: PackSourceRecord,
    /// Deterministic digest over the installed content.
    pub content_id: String,
    /// RFC 3339-ish install timestamp, seconds since the Unix epoch.
    pub installed_at_unix: u64,
}

/// Persisted store document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackStoreState {
    /// Schema identifier, checked on read.
    pub schema: String,
    /// Schema version, checked on read.
    pub schema_version: u32,
    /// Currently active install, or `None` for the compiled baseline.
    pub active: Option<String>,
    /// Rollback target: the selection replaced by the current one.
    pub previous: Option<String>,
    /// Install lineage, newest first.
    pub installs: Vec<InstalledPackRecord>,
}

impl Default for PackStoreState {
    fn default() -> Self {
        Self {
            schema: PACK_STORE_STATE_SCHEMA.to_owned(),
            schema_version: 1,
            active: None,
            previous: None,
            installs: Vec::new(),
        }
    }
}

impl PackStoreState {
    /// Look up an install by identifier.
    pub fn record(&self, install_id: &str) -> Option<&InstalledPackRecord> {
        self.installs
            .iter()
            .find(|record| record.install_id == install_id)
    }

    /// The active install record, when one is selected.
    pub fn active_record(&self) -> Option<&InstalledPackRecord> {
        self.active.as_deref().and_then(|id| self.record(id))
    }

    /// The rollback target record, when one is recoverable.
    pub fn previous_record(&self) -> Option<&InstalledPackRecord> {
        self.previous.as_deref().and_then(|id| self.record(id))
    }
}

/// Handle to the versioned installed-pack store.
#[derive(Debug, Clone)]
pub struct PackStore {
    root: PathBuf,
}

impl PackStore {
    /// Build a store handle rooted at an explicit directory.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Build a store handle under an explicit Effigy user-state home.
    ///
    /// Callers that already own a `~/.effigy` resolution (including a test
    /// override) thread it through here so the pack store and the user-global
    /// override directory never disagree about which home they mean.
    pub fn under_home(home: &Path) -> Self {
        Self::at(home.join(STORE_DIR).join(STORE_LAYOUT_VERSION))
    }

    /// Build a store handle under the Effigy user-state home, when one exists.
    pub fn user() -> Option<Self> {
        effigy_home_dir().map(|home| Self::under_home(&home))
    }

    /// Store root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path of the persisted state document.
    pub fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }

    /// Content directory of one install.
    pub fn install_dir(&self, install_id: &str) -> PathBuf {
        self.root.join("installs").join(install_id)
    }

    /// Transient staging directory used by the install transaction.
    pub fn staging_root(&self) -> PathBuf {
        self.root.join("staging")
    }

    /// Whether any store state has been written yet.
    pub fn exists(&self) -> bool {
        self.state_path().is_file()
    }

    /// Read persisted state, returning the empty default when absent.
    ///
    /// A present-but-unreadable document is an error rather than a silent
    /// reset: losing lineage quietly would make rollback undiagnosable.
    pub fn load(&self) -> Result<PackStoreState, PackError> {
        let path = self.state_path();
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PackStoreState::default())
            }
            Err(error) => {
                return Err(PackError::StoreStateUnreadable {
                    path,
                    reason: error.to_string(),
                })
            }
        };
        let state: PackStoreState =
            serde_json::from_str(&contents).map_err(|error| PackError::StoreStateUnreadable {
                path: path.clone(),
                reason: error.to_string(),
            })?;
        if state.schema != PACK_STORE_STATE_SCHEMA || state.schema_version != 1 {
            return Err(PackError::StoreStateUnreadable {
                path,
                reason: format!(
                    "unsupported store schema `{}` v{}",
                    state.schema, state.schema_version
                ),
            });
        }
        Ok(state)
    }

    /// Persist state atomically: write a sibling temp file, then rename.
    pub fn commit(&self, state: &PackStoreState) -> Result<(), PackError> {
        std::fs::create_dir_all(&self.root).map_err(|error| PackError::io(&self.root, &error))?;
        let encoded = serde_json::to_string_pretty(state).map_err(|error| {
            PackError::StoreStateUnreadable {
                path: self.state_path(),
                reason: error.to_string(),
            }
        })?;
        let target = self.state_path();
        let temp = self
            .root
            .join(format!("state.json.tmp-{}", unique_suffix()));
        std::fs::write(&temp, format!("{encoded}\n"))
            .map_err(|error| PackError::io(&temp, &error))?;
        std::fs::rename(&temp, &target).map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            PackError::io(&target, &error)
        })
    }

    /// Select the previous validated install.
    ///
    /// Rollback is a swap, not a pop: the selection it replaces becomes the
    /// next rollback target, so `rollback` after `rollback` returns.
    pub fn rollback(&self) -> Result<PackStoreState, PackError> {
        let mut state = self.load()?;
        let Some(target) = state.previous.clone() else {
            return Err(PackError::NoRollbackTarget);
        };
        if state.record(&target).is_none() || !self.install_dir(&target).is_dir() {
            return Err(PackError::RollbackTargetMissing { install_id: target });
        }
        state.previous = state.active.take();
        state.active = Some(target);
        self.commit(&state)?;
        Ok(state)
    }

    /// Select the compiled baseline.
    ///
    /// Installed content is retained and the displaced selection becomes the
    /// rollback target, so reset is recoverable rather than destructive.
    pub fn reset(&self) -> Result<PackStoreState, PackError> {
        let mut state = self.load()?;
        if let Some(active) = state.active.take() {
            state.previous = Some(active);
        }
        self.commit(&state)?;
        Ok(state)
    }

    /// Record a validated install and activate it atomically.
    pub(super) fn activate(
        &self,
        record: InstalledPackRecord,
    ) -> Result<PackStoreState, PackError> {
        let mut state = self.load()?;
        if state.active.as_deref() != Some(record.install_id.as_str()) {
            state.previous = state.active.take();
        }
        state.active = Some(record.install_id.clone());
        state
            .installs
            .retain(|existing| existing.install_id != record.install_id);
        state.installs.insert(0, record);
        self.prune(&mut state);
        self.commit(&state)?;
        Ok(state)
    }

    /// Drop lineage beyond the retention limit, deleting its content.
    fn prune(&self, state: &mut PackStoreState) {
        let mut kept = Vec::new();
        let mut dropped = Vec::new();
        for record in std::mem::take(&mut state.installs) {
            let pinned = state.active.as_deref() == Some(record.install_id.as_str())
                || state.previous.as_deref() == Some(record.install_id.as_str());
            if pinned || kept.len() < MAX_RETAINED_INSTALLS {
                kept.push(record);
            } else {
                dropped.push(record);
            }
        }
        for record in dropped {
            let _ = std::fs::remove_dir_all(self.install_dir(&record.install_id));
        }
        state.installs = kept;
    }
}

/// Seconds since the Unix epoch, saturating at zero on a skewed clock.
pub(super) fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

fn unique_suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    )
}
