//! Runtime publication evidence and unreleased-main refusal.
//!
//! v0.15 ships one migration path: the v0.14.1 importers, which read legacy
//! files on first use. The importer for the current-main central
//! `~/.effigy/state/kernel.db` is retired (contract 052 §11.1). A machine which
//! still holds that store has to be migrated with Effigy main 7b82d26 before it
//! upgrades; this module only recognises the store and refuses once.
pub(crate) mod adoption;
pub use adoption::{authority, committed, Authority, ImportReceipt};

use crate::ctx::ObserveContext;
use crate::fs::FileName;
use crate::observe::Observed;
use crate::root::StateRoot;
use crate::store::StoreError;
use std::path::{Path, PathBuf};

/// The caller resolves Git metadata; the kernel holds both directory
/// generations through publication and persists their facts in the local seal.
#[derive(Debug, Clone)]
pub struct CheckoutLocation {
    pub root: PathBuf,
    pub git_directory: Option<PathBuf>,
}

/// Refuse once when this machine still holds the retired central store. A
/// committed migration manifest means its owner already cut over, so the file
/// is ignored entirely (M3: the retired source is never deleted here). With no
/// manifest there is no v0.15 import path, and guessing is never permission.
pub fn refuse_unreleased_main(cx: &ObserveContext, machine_home: &Path) -> Result<(), StoreError> {
    if committed(cx, machine_home)?.is_some() {
        return Ok(());
    }
    let legacy = match StateRoot::legacy_observe(cx, machine_home) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Ok(()),
        Observed::Unknown(reason) => {
            return Err(StoreError::Corrupt(format!(
                "unreleased-main runtime state is unknown: {reason:?}"
            )))
        }
    };
    let database = legacy
        .dir()
        .entry(&FileName::new(crate::store::DB_NAME)?)?
        .is_some();
    if database {
        return Err(StoreError::UnreleasedMainRuntime);
    }
    Ok(())
}
