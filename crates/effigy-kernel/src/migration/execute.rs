//! Cross-filesystem publication has one commit record, followed by local seals.
use super::fence::LegacyFence;
use super::*;
use crate::ctx::EffectContext;
use crate::fs::{DirFd, DirectoryIdentity};
use crate::gate::{Gate, GateGuard, Wait};
use crate::id::{CheckoutId, Scope};
use crate::record::{atomic_replace, read_record, Record, ReplaceOpts};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};
use std::time::Instant;

const FORMAT: &str = "effigy.runtime-import.v1";
const PREPARED: &str = "migration-prepared.json";
const SEALED: &str = "migration-committed.json";
const MANIFEST: &str = "cutover.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedDestination {
    pub checkout: Option<CheckoutId>,
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
        Ok(receipt)
    }
}
#[derive(Debug, Serialize, Deserialize)]
struct Seal {
    format: String,
    id: String,
    manifest_digest: String,
    destination: ImportedDestination,
}
impl Record for Seal {
    const MAX_BYTES: u64 = 64 << 10;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let seal: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if seal.format != FORMAT {
            return Err("unsupported import seal".into());
        }
        Ok(seal)
    }
}

#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub receipt: ImportReceipt,
    pub elapsed_ms: u128,
    pub resumed: bool,
}

#[derive(Debug)]
struct Destination {
    root: StateRoot,
    gate: GateGuard,
    stage: StateRoot,
    partition: Partition,
    checkout_root: PathBuf,
}

/// Read-only checkout identity resolver. Hints are diagnostic; each call must
/// verify the token and physical checkout/Git-directory generation itself.
pub type CheckoutResolver<'a> =
    dyn FnMut(CheckoutId, Option<&Path>, Option<&Path>) -> Result<PathBuf, StoreError> + 'a;

/// `resolve` must read and verify the existing checkout token, never mint one.
/// Its optional path is an untrusted source hint, not placement authority. It
/// may locate that identity through bounded discovery; proofs map it to an
/// independent private checkout. It is called again immediately before commit.
pub fn import(
    cx: &EffectContext,
    source: &StateRoot,
    machine_home: &Path,
    quiescence: &LegacyQuiescence,
    resolve: &mut CheckoutResolver<'_>,
) -> Result<ImportReport, MigrationError> {
    cx.require(crate::ctx::Capability::HostAuthority)
        .map_err(|_| StoreError::HostAuthorityRequired)?;
    if cx.operation().class() != crate::ctx::EffectClass::Mutate {
        return Err(StoreError::ClassRefused(cx.operation().class()).into());
    }
    cx.operation()
        .check("legacy migration")
        .map_err(StoreError::Interrupted)?;
    let started = Instant::now();
    let observe = cx
        .operation()
        .observe()
        .map_err(|_| StoreError::Unbounded)?;
    if let Some(receipt) = committed(&observe, machine_home)? {
        finish_seals(cx, machine_home, &receipt, resolve)?;
        return Ok(ImportReport {
            receipt,
            elapsed_ms: started.elapsed().as_millis(),
            resumed: true,
        });
    }
    source.dir().require_local().map_err(StoreError::from)?;
    let preflight = inspect(&observe, source)?;
    if !preflight.ready() {
        return Err(MigrationError::Blocked(preflight));
    }
    let _source_fence = LegacyFence::acquire(&observe, source, quiescence)?;

    let source_digest = files::digest(
        &observe,
        source.dir(),
        &FileName::new(source.database_name()).map_err(StoreError::from)?,
    )?;
    let reader = match Store::read_sealed(&observe, source) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => {
            return Err(
                StoreError::Corrupt("legacy source disappeared before import".into()).into(),
            )
        }
        Observed::Unknown(reason) => return Err(inconsistent(reason).into()),
    };
    reader
        .conn()
        .execute_batch("BEGIN")
        .map_err(StoreError::from)?;
    let mut plan = ImportPlan {
        preflight,
        partitions: super::plan::partitions(&observe, &reader)?,
    };
    if plan
        .partitions
        .iter()
        .any(|part| part.checkout.is_none() && !part.objects.is_empty())
    {
        return Err(StoreError::Corrupt(
            "legacy machine objects have no permitted shared-fact destination".into(),
        )
        .into());
    }
    if plan.partitions.len() > 1000 {
        return Err(StoreError::TooLarge { limit: 1000 }.into());
    }
    if plan
        .preflight
        .writer_build
        .as_ref()
        .is_some_and(|build| build.len() > 512)
    {
        return Err(StoreError::Corrupt("legacy writer build exceeds its bound".into()).into());
    }
    // Resolve and trust-check every destination before creating any state.
    let mut locations = Vec::new();
    for part in &plan.partitions {
        if let Some(checkout) = part.checkout {
            let path = resolve(
                checkout,
                part.path_hint.as_deref().map(Path::new),
                part.git_dir_hint.as_deref().map(Path::new),
            )?;
            let dir = DirFd::open(&path).map_err(StoreError::from)?;
            locations.push((checkout, dir));
        }
    }
    let machine_dir = DirFd::open(machine_home).map_err(StoreError::from)?;
    let mut identities = std::collections::BTreeSet::new();
    for dir in locations
        .iter()
        .map(|(_, dir)| dir)
        .chain(std::iter::once(&machine_dir))
    {
        let identity = dir.identity().map_err(StoreError::from)?;
        if !identities.insert((identity.device, identity.inode)) {
            return Err(StoreError::Corrupt(
                "multiple import authorities resolve to one directory".into(),
            )
            .into());
        }
    }
    if plan.partitions.iter().all(|part| part.checkout.is_some()) {
        plan.partitions.push(Partition {
            checkout: None,
            path_hint: None,
            git_dir_hint: None,
            resources: Default::default(),
            runs: Default::default(),
            objects: Default::default(),
            unresolved_runs: 0,
        });
    }
    // Deterministic destination order; no gate is acquired under a transaction.
    plan.partitions
        .sort_by_key(|part| part.checkout.map(|id| id.resource().canonical()));
    let machine = StateRoot::machine_effects(cx, machine_dir.path()).map_err(StoreError::from)?;
    let migration_id = ResourceId::store(Scope::Machine, "migrate:runtime")?;
    let machine_gate =
        Gate::acquire(cx, &machine, &[migration_id], Wait::FailFast).map_err(StoreError::from)?;
    // A racing importer may have committed while this one was resolving roots.
    if let Some(receipt) = committed(&observe, machine_home)? {
        finish_seals(cx, machine_home, &receipt, resolve)?;
        return Ok(ImportReport {
            receipt,
            elapsed_ms: started.elapsed().as_millis(),
            resumed: true,
        });
    }
    let mut destinations = Vec::new();
    for part in plan.partitions {
        let (root, checkout_root) = match part.checkout {
            Some(checkout) => {
                let dir = &locations
                    .iter()
                    .find(|(id, _)| *id == checkout)
                    .expect("all checkout locations resolved")
                    .1;
                (
                    StateRoot::effects(cx, dir.path()).map_err(StoreError::from)?,
                    dir.path().to_path_buf(),
                )
            }
            None => (machine.clone(), machine_dir.path().to_path_buf()),
        };
        let owner = ResourceId::store(
            part.checkout.map_or(Scope::Machine, Scope::Checkout),
            "migrate:checkout",
        )?;
        let gate = Gate::acquire(cx, &root, &[owner], Wait::FailFast).map_err(StoreError::from)?;
        crate::fault_point!(MigrationStageBefore).map_err(StoreError::from)?;
        let stage_name = format!(".import-{source_digest}");
        let previous: Option<ImportReceipt> = optional_record(&observe, root.dir(), PREPARED)?;
        if let Some(previous) = previous {
            // Only roots tagged by our own prepared receipt can be discarded.
            // No committed manifest exists, so they never acquired authority.
            discard_prepared(&observe, &root, &previous)?;
        } else {
            let mut names = vec![root.database_name()];
            if part.checkout.is_some() {
                names.extend(["objects", "runs", "staging"]);
            }
            for name in names {
                if root
                    .dir()
                    .entry(&FileName::new(name).map_err(StoreError::from)?)
                    .map_err(StoreError::from)?
                    .is_some()
                {
                    return Err(StoreError::Corrupt(format!(
                        "refusing to overwrite existing destination {}",
                        root.path().join(name).display()
                    ))
                    .into());
                }
            }
        }
        let receipt = ImportReceipt {
            format: FORMAT.into(),
            id: source_digest.clone(),
            source_digest: source_digest.clone(),
            source_schema: reader.schema_version(),
            source_build: plan.preflight.writer_build.clone(),
            writer_build: concat!("effigy-kernel ", env!("CARGO_PKG_VERSION")).into(),
            destinations: Vec::new(),
        };
        replace(cx, &gate, root.dir(), PREPARED, &receipt)?;
        let stage = root
            .migration_stage(&stage_name)
            .map_err(StoreError::from)?;
        crate::fault_point!(MigrationStageAfter).map_err(StoreError::from)?;
        destinations.push(Destination {
            root,
            gate,
            stage,
            partition: part,
            checkout_root,
        });
    }
    let mut receipt = ImportReceipt {
        format: FORMAT.into(),
        id: source_digest.clone(),
        source_digest: source_digest.clone(),
        source_schema: reader.schema_version(),
        source_build: plan.preflight.writer_build.clone(),
        writer_build: concat!("effigy-kernel ", env!("CARGO_PKG_VERSION")).into(),
        destinations: Vec::new(),
    };
    for destination in &destinations {
        crate::fault_point!(MigrationCopyBefore).map_err(StoreError::from)?;
        let mut store = Store::open(cx, &destination.stage)?;
        let copied = copy::partition(
            cx,
            &reader,
            &mut store,
            &destination.partition,
            destination.root.path(),
            &source_digest,
            &source_digest,
        )?;
        store.check_integrity()?;
        drop(store); // last connection checkpoints before fingerprint/publication
        copy_files(cx, &observe, source, destination)?;
        let database_digest = files::digest(
            &observe,
            destination.stage.dir(),
            &FileName::new(destination.stage.database_name()).map_err(StoreError::from)?,
        )?;
        crate::fault_point!(MigrationCopyAfter).map_err(StoreError::from)?;
        receipt.destinations.push(ImportedDestination {
            checkout: destination.partition.checkout,
            checkout_root: destination.checkout_root.clone(),
            directory_identity: DirFd::open(&destination.checkout_root)
                .map_err(StoreError::from)?
                .identity()
                .map_err(StoreError::from)?,
            database_digest,
            rows: copied.rows,
            unresolved_runs: destination.partition.unresolved_runs,
        });
    }
    // Source bytes and every checkout binding must still match at publication.
    if files::digest(
        &observe,
        source.dir(),
        &FileName::new(source.database_name()).map_err(StoreError::from)?,
    )? != source_digest
    {
        return Err(StoreError::Corrupt("legacy source changed during migration".into()).into());
    }
    source.verify_binding().map_err(StoreError::from)?;
    if DirFd::open(machine_dir.path())
        .map_err(StoreError::from)?
        .identity()
        .map_err(StoreError::from)?
        != machine_dir.identity().map_err(StoreError::from)?
    {
        return Err(
            StoreError::Corrupt("machine home identity changed before publication".into()).into(),
        );
    }
    for destination in &destinations {
        if let Some(checkout) = destination.partition.checkout {
            let current = resolve(
                checkout,
                Some(&destination.checkout_root),
                destination.partition.git_dir_hint.as_deref().map(Path::new),
            )?;
            let current_dir = DirFd::open(&current).map_err(StoreError::from)?;
            let expected = &locations
                .iter()
                .find(|(id, _)| *id == checkout)
                .expect("resolved checkout binding")
                .1;
            if current_dir.path() != destination.checkout_root
                || current_dir.identity().map_err(StoreError::from)?
                    != expected.identity().map_err(StoreError::from)?
            {
                return Err(StoreError::Corrupt(
                    "checkout resolution changed before migration publication".into(),
                )
                .into());
            }
        }
        destination
            .root
            .verify_binding()
            .map_err(StoreError::from)?;
        crate::fault_point!(MigrationPreparedBefore).map_err(StoreError::from)?;
        let local = ImportReceipt {
            destinations: receipt
                .destinations
                .iter()
                .filter(|entry| entry.checkout_root == destination.checkout_root)
                .cloned()
                .collect(),
            ..receipt.clone()
        };
        replace(
            cx,
            &destination.gate,
            destination.root.dir(),
            PREPARED,
            &local,
        )?;
        publish_prepared(destination)?;
        crate::fault_point!(MigrationPreparedAfter).map_err(StoreError::from)?;
    }
    if files::digest(
        &observe,
        source.dir(),
        &FileName::new(source.database_name()).map_err(StoreError::from)?,
    )? != source_digest
    {
        return Err(
            StoreError::Corrupt("legacy source changed before cutover commit".into()).into(),
        );
    }
    // The only authority switch. Prepared databases alone are always inert.
    let migration = machine
        .dir()
        .child("migration", Trust::Private, true)
        .map_err(StoreError::from)?
        .expect("created migration directory");
    crate::fault_point!(MigrationCommitBefore).map_err(StoreError::from)?;
    replace(cx, &machine_gate, &migration, MANIFEST, &receipt)?;
    crate::fault_point!(MigrationCommitAfter).map_err(StoreError::from)?;
    finish_seals(cx, machine_home, &receipt, resolve)?;
    Ok(ImportReport {
        receipt,
        elapsed_ms: started.elapsed().as_millis(),
        resumed: false,
    })
}

fn copy_files(
    cx: &EffectContext,
    observe: &ObserveContext,
    source: &StateRoot,
    destination: &Destination,
) -> Result<(), StoreError> {
    if !destination.partition.objects.is_empty() {
        let objects = source
            .read_family(Family::Objects)?
            .ok_or_else(|| StoreError::Corrupt("legacy object family is missing".into()))?;
        let target = destination
            .stage
            .family(Family::Objects)?
            .expect("created objects family");
        for digest in &destination.partition.objects {
            crate::objects::open(&objects, digest, observe)?;
            let input = objects
                .child(digest, Trust::Private, false)?
                .ok_or_else(|| StoreError::Corrupt("legacy object vanished".into()))?;
            let output = target
                .child(digest, Trust::Private, true)?
                .expect("created object import");
            files::tree(cx, &input, &output)?;
            output.chmod(0o500)?;
            crate::objects::open(&target, digest, observe)?;
        }
        target.sync()?;
    }
    for family in [Family::Runs, Family::Staging]
        .into_iter()
        .filter(|_| destination.partition.checkout.is_some())
    {
        let Some(input) = source.read_family(family)? else {
            continue;
        };
        for run in &destination.partition.runs {
            let Some(input) = input.child(run, Trust::Private, false)? else {
                continue;
            };
            let output = destination
                .stage
                .family(family)?
                .expect("created import family")
                .child(run, Trust::Private, true)?
                .expect("created run import");
            files::tree(cx, &input, &output)?;
        }
    }
    destination.stage.dir().sync()?;
    Ok(())
}
fn publish_prepared(destination: &Destination) -> Result<(), StoreError> {
    let mut names = vec![destination.root.database_name()];
    if destination.partition.checkout.is_some() {
        names.extend(["objects", "runs", "staging"]);
    }
    for name in names {
        let name = FileName::new(name)?;
        if destination.stage.dir().entry(&name)?.is_none() {
            continue;
        }
        if destination.root.dir().entry(&name)?.is_some() {
            return Err(StoreError::Corrupt(
                "prepared destination family already exists".into(),
            ));
        }
        destination
            .stage
            .dir()
            .move_to(&name, destination.root.dir(), &name)?;
    }
    destination.stage.dir().sync()?;
    destination.root.dir().sync()?;
    Ok(())
}
fn discard_prepared(
    observe: &ObserveContext,
    root: &StateRoot,
    previous: &ImportReceipt,
) -> Result<(), StoreError> {
    root.verify_binding()?;
    if root.dir().entry(&FileName::new(SEALED)?)?.is_some() {
        return Err(StoreError::Corrupt(
            "sealed destination cannot be discarded".into(),
        ));
    }
    if root
        .dir()
        .entry(&FileName::new(root.database_name())?)?
        .is_some()
    {
        let reader = match Store::read_sealed(observe, root) {
            Observed::Verified(reader, _) => reader,
            other => {
                return Err(StoreError::Corrupt(format!(
                    "cannot verify prepared database before discard: {other:?}"
                )))
            }
        };
        let id: Option<String> = reader
            .conn()
            .query_row(
                "SELECT value FROM meta WHERE key='migration_id'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if reader.schema_version() != crate::store::SCHEMA_VERSION
            || id.as_deref() != Some(&previous.id)
        {
            return Err(StoreError::Corrupt(
                "refusing to discard a database not tagged by this prepared import".into(),
            ));
        }
    }
    let mut names = vec![root.database_name()];
    if root.database_name() == crate::store::DB_NAME {
        names.extend(["objects", "runs", "staging"]);
    }
    for name in names {
        root.dir().remove_tree(&FileName::new(name)?, 40)?;
    }
    root.dir()
        .remove_tree(&FileName::new(&format!(".import-{}", previous.id))?, 40)?;
    root.dir().sync()?;
    Ok(())
}
fn finish_seals(
    cx: &EffectContext,
    machine_home: &Path,
    receipt: &ImportReceipt,
    resolve: &mut CheckoutResolver<'_>,
) -> Result<(), StoreError> {
    let bytes =
        serde_json::to_vec(receipt).map_err(|error| StoreError::Corrupt(error.to_string()))?;
    let manifest_digest = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let observe = cx
        .operation()
        .observe()
        .map_err(|_| StoreError::Unbounded)?;
    for destination in &receipt.destinations {
        let root = match destination.checkout {
            Some(checkout) => {
                let path = resolve(checkout, Some(&destination.checkout_root), None)?;
                let dir = DirFd::open(&path)?;
                if dir.path() != destination.checkout_root
                    || dir.identity()? != destination.directory_identity
                {
                    return Err(StoreError::Corrupt(
                        "committed checkout binding changed".into(),
                    ));
                }
                StateRoot::effects(cx, dir.path())?
            }
            None => {
                let dir = DirFd::open(machine_home)?;
                if dir.path() != destination.checkout_root
                    || dir.identity()? != destination.directory_identity
                {
                    return Err(StoreError::Corrupt(
                        "committed machine binding changed".into(),
                    ));
                }
                StateRoot::machine_effects(cx, machine_home)?
            }
        };
        if let Some(seal) = optional_record::<Seal>(&observe, root.dir(), SEALED)? {
            if seal.id != receipt.id
                || seal.manifest_digest != manifest_digest
                || seal.destination != *destination
            {
                return Err(StoreError::Corrupt(
                    "migration seal disagrees with commit manifest".into(),
                ));
            }
            continue;
        }
        if files::digest(&observe, root.dir(), &FileName::new(root.database_name())?)?
            != destination.database_digest
        {
            return Err(StoreError::Corrupt(
                "committed destination database digest mismatch".into(),
            ));
        }
        let owner = ResourceId::store(
            destination.checkout.map_or(Scope::Machine, Scope::Checkout),
            "migrate:seal",
        )?;
        let gate = Gate::acquire(cx, &root, &[owner], Wait::FailFast)?;
        let seal = Seal {
            format: FORMAT.into(),
            id: receipt.id.clone(),
            manifest_digest: manifest_digest.clone(),
            destination: destination.clone(),
        };
        crate::fault_point!(MigrationSealBefore).map_err(StoreError::from)?;
        replace(cx, &gate, root.dir(), SEALED, &seal)?;
        crate::fault_point!(MigrationSealAfter).map_err(StoreError::from)?;
    }
    Ok(())
}
pub fn committed(
    cx: &ObserveContext,
    machine_home: &Path,
) -> Result<Option<ImportReceipt>, StoreError> {
    let root = match StateRoot::machine_observe(cx, machine_home) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Ok(None),
        Observed::Unknown(reason) => return Err(inconsistent(reason)),
    };
    let Some(dir) = root.dir().child("migration", Trust::Private, false)? else {
        return Ok(None);
    };
    optional_record(cx, &dir, MANIFEST)
}
fn optional_record<T: Record>(
    cx: &ObserveContext,
    dir: &DirFd,
    name: &str,
) -> Result<Option<T>, StoreError> {
    match read_record(cx, dir, &FileName::new(name)?) {
        Observed::Verified(record, _) => Ok(Some(record)),
        Observed::Absent(_) => Ok(None),
        Observed::Unknown(reason) => Err(inconsistent(reason)),
    }
}
fn replace(
    cx: &EffectContext,
    gate: &GateGuard,
    dir: &DirFd,
    name: &str,
    record: &impl Serialize,
) -> Result<(), StoreError> {
    let bytes =
        serde_json::to_vec(record).map_err(|error| StoreError::Corrupt(error.to_string()))?;
    if bytes.len() > 16 << 20 {
        return Err(StoreError::TooLarge { limit: 16 << 20 });
    }
    atomic_replace(
        cx,
        gate,
        dir,
        &FileName::new(name)?,
        &bytes,
        ReplaceOpts::default(),
    )?;
    Ok(())
}
