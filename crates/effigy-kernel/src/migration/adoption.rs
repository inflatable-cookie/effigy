//! Local import publication receipts. Checking them never imports or repairs.
use super::*;
use crate::fs::DirectoryIdentity;
use crate::id::CheckoutId;
use crate::record::{read_record, Record};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(super) const FORMAT: &str = "effigy.runtime-import.v1";
pub(super) const PREPARED: &str = "migration-prepared.json";
pub(super) const SEALED: &str = "migration-committed.json";
pub(super) const MANIFEST: &str = "cutover.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedDestination {
    pub checkout: Option<CheckoutId>,
    #[serde(default)]
    pub checkout_aliases: Vec<CheckoutId>,
    #[serde(default)]
    pub git_directory: Option<PathBuf>,
    #[serde(default)]
    pub git_directory_identity: Option<DirectoryIdentity>,
    pub checkout_root: PathBuf,
    pub directory_identity: DirectoryIdentity,
    pub database_digest: String,
    pub rows: BTreeMap<String, u64>,
    pub unresolved_runs: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReceipt {
    pub format: String,
    pub id: String,
    pub source_digest: String,
    pub source_schema: i64,
    pub source_build: Option<String>,
    pub writer_build: String,
    pub destinations: Vec<ImportedDestination>,
}
impl Record for ImportReceipt {
    const MAX_BYTES: u64 = 16 << 20;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let receipt: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if receipt.format != FORMAT || receipt.id != receipt.source_digest {
            return Err("unsupported import receipt".into());
        }
        crate::objects::validate_digest(&receipt.id).map_err(|error| error.to_string())?;
        if receipt.destinations.len() > 1000 {
            return Err("too many import destinations".into());
        }
        for destination in &receipt.destinations {
            validate_destination(destination)?;
        }
        Ok(receipt)
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Seal {
    pub(super) format: String,
    pub(super) id: String,
    pub(super) manifest_digest: String,
    pub(super) destination: ImportedDestination,
}
impl Record for Seal {
    const MAX_BYTES: u64 = 64 << 10;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let seal: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if seal.format != FORMAT {
            return Err("unsupported import seal".into());
        }
        validate_destination(&seal.destination)?;
        Ok(seal)
    }
}

/// File-level publication evidence, not a substitute for a resource observation.
#[derive(Debug, Clone, Serialize)]
pub enum Authority {
    Fresh,
    Prepared {
        id: String,
    },
    Committed {
        id: String,
        checkout: Option<CheckoutId>,
        checkout_aliases: Vec<CheckoutId>,
    },
    LegacyEvidence,
}

#[derive(Debug)]
pub(crate) enum Access {
    Unchecked,
    Fresh,
    Committed {
        id: String,
        checkout: Option<CheckoutId>,
        checkout_aliases: Vec<CheckoutId>,
    },
}

pub fn authority(cx: &ObserveContext, root: &StateRoot) -> Observed<Authority> {
    let result = inspect_authority(cx, root);
    match result {
        Ok(state) => Observed::Verified(
            state,
            crate::observe::Evidence::new("local runtime import publication".into(), None)
                .expect("fixed evidence source"),
        ),
        Err(StoreError::Untrusted { reason, .. }) => {
            Observed::Unknown(UnknownReason::Untrusted(reason))
        }
        Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
    }
}

fn inspect_authority(cx: &ObserveContext, root: &StateRoot) -> Result<Authority, StoreError> {
    root.verify_binding()?;
    if !root.requires_adoption() {
        return Ok(Authority::LegacyEvidence);
    }
    let prepared: Option<ImportReceipt> = optional(cx, root, PREPARED)?;
    if let Some(seal) = optional::<Seal>(cx, root, SEALED)? {
        verify_seal(root, &seal)?;
        if prepared.is_some_and(|prepared| prepared.id != seal.id) {
            return Err(StoreError::Corrupt(
                "prepared import and local seal disagree".into(),
            ));
        }
        Ok(Authority::Committed {
            id: seal.id,
            checkout: seal.destination.checkout,
            checkout_aliases: seal.destination.checkout_aliases,
        })
    } else if let Some(prepared) = prepared {
        Ok(Authority::Prepared { id: prepared.id })
    } else {
        Ok(Authority::Fresh)
    }
}

pub(crate) fn access(cx: &ObserveContext, root: &StateRoot) -> Result<Access, StoreError> {
    match inspect_authority(cx, root)? {
        Authority::Fresh => Ok(Access::Fresh),
        Authority::LegacyEvidence => Ok(Access::Unchecked),
        Authority::Prepared { id } => Err(StoreError::MigrationPending(id)),
        Authority::Committed {
            id,
            checkout,
            checkout_aliases,
        } => {
            if root
                .dir()
                .entry(&FileName::new(root.database_name())?)?
                .is_none()
            {
                return Err(StoreError::Corrupt(
                    "sealed runtime database is missing; legacy state is not a fallback".into(),
                ));
            }
            Ok(Access::Committed {
                id,
                checkout,
                checkout_aliases,
            })
        }
    }
}

pub(crate) fn validate_database(conn: &Connection, access: &Access) -> Result<(), StoreError> {
    if matches!(access, Access::Unchecked) {
        return Ok(());
    }
    let has_meta: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='meta')",
        [],
        |row| row.get(0),
    )?;
    if !has_meta {
        return if matches!(access, Access::Fresh) {
            Ok(())
        } else {
            Err(StoreError::Corrupt(
                "sealed runtime database has no import metadata".into(),
            ))
        };
    }
    let id: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key='migration_id'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    match (access, id) {
        (Access::Fresh, Some(id)) => Err(StoreError::MigrationPending(id)),
        (Access::Fresh, None) => Ok(()),
        (Access::Committed { id: expected, .. }, Some(id)) if expected == &id => {
            let format: Option<String> = conn
                .query_row(
                    "SELECT value FROM meta WHERE key='migration_format'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            let source: Option<String> = conn
                .query_row(
                    "SELECT value FROM meta WHERE key='migration_source_digest'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            if format.as_deref() != Some("q010-v1") || source.as_deref() != Some(expected) {
                return Err(StoreError::Corrupt(
                    "sealed runtime database import metadata mismatch".into(),
                ));
            }
            Ok(())
        }
        _ => Err(StoreError::Corrupt(
            "runtime database and local import seal disagree".into(),
        )),
    }
}

pub(crate) fn check_invocation(
    cx: &ObserveContext,
    root: &StateRoot,
    invocation: &crate::id::ResolvedInvocation,
) -> Result<(), StoreError> {
    if let Access::Committed {
        checkout,
        checkout_aliases,
        ..
    } = access(cx, root)?
    {
        let actual = invocation.spec().task.checkout();
        if checkout.map_or(
            actual.kind() != crate::id::CheckoutKind::Machine,
            |expected| actual != expected && !checkout_aliases.contains(&actual),
        ) {
            return Err(StoreError::Corrupt(
                "run checkout identity disagrees with its imported journal".into(),
            ));
        }
    }
    Ok(())
}
fn verify_seal(root: &StateRoot, seal: &Seal) -> Result<(), StoreError> {
    if root.owning_directory().identity()? != seal.destination.directory_identity {
        return Err(StoreError::Corrupt(
            "runtime checkout directory was replaced after import".into(),
        ));
    }
    match (
        &seal.destination.git_directory,
        seal.destination.git_directory_identity,
    ) {
        (Some(path), Some(expected)) if crate::fs::DirFd::open(path)?.identity()? == expected => {}
        (None, None) => {}
        _ => {
            return Err(StoreError::Corrupt(
                "runtime Git directory generation changed after import".into(),
            ))
        }
    }
    if (root.database_name() == "machine.db") != seal.destination.checkout.is_none() {
        return Err(StoreError::Corrupt(
            "import seal belongs to a different runtime database".into(),
        ));
    }
    Ok(())
}
fn optional<T: Record>(
    cx: &ObserveContext,
    root: &StateRoot,
    name: &str,
) -> Result<Option<T>, StoreError> {
    match read_record(cx, root.dir(), &FileName::new(name)?) {
        Observed::Verified(value, _) => Ok(Some(value)),
        Observed::Absent(_) => Ok(None),
        Observed::Unknown(UnknownReason::Untrusted(reason)) => Err(StoreError::Untrusted {
            path: root.path().into(),
            reason,
        }),
        Observed::Unknown(reason) => Err(inconsistent(reason)),
    }
}

fn validate_destination(destination: &ImportedDestination) -> Result<(), String> {
    if destination.checkout_aliases.len() > 32 {
        return Err("too many checkout aliases".into());
    }
    if destination.git_directory.is_some() != destination.git_directory_identity.is_some()
        || destination
            .git_directory
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
    {
        return Err("incomplete Git directory generation".into());
    }
    match destination.checkout {
        None if !destination.checkout_aliases.is_empty() || destination.git_directory.is_some() => {
            return Err("machine destination has checkout aliases or Git state".into())
        }
        Some(primary) => {
            if primary.kind() == crate::id::CheckoutKind::Machine
                || destination
                    .checkout_aliases
                    .iter()
                    .any(|alias| alias.kind() != primary.kind())
            {
                return Err("checkout alias kind mismatch".into());
            }
            let mut ids = std::collections::BTreeSet::new();
            for id in std::iter::once(primary).chain(destination.checkout_aliases.iter().copied()) {
                if !ids.insert(id.resource().canonical()) {
                    return Err("duplicate checkout alias".into());
                }
            }
        }
        _ => {}
    }
    Ok(())
}
