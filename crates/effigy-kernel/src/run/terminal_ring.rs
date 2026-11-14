//! The strict-persistence terminal record: one fixed-size, preallocated ring
//! file (effigy#985, contract 049 §"Strict persistence").
//!
//! A SQLite terminal row has no small, source-derived physical maximum: the
//! honest ceiling for one row and its log frames is about 1 GiB (effigy#981),
//! which does not fit a consumer that has to account for every byte one
//! invocation writes. Strict persistence therefore stores the record in this
//! file instead: `effigy run prepare` creates and fully preallocates it outside
//! the route, and the route only overwrites one slot in place. Nothing grows,
//! nothing is created, and no SQLite connection is opened (`run::terminal`).
//!
//! The file is a fixed header followed by [`SLOTS`] fixed-size slots. A slot
//! holds a sequence number, the bounded fields
//! [`super::terminal::TerminalRecord`] already defines, its own SHA-256 and a
//! one-byte commit marker. The commit marker is the ordering: the body is
//! written and flushed first, then the marker is written and flushed, so a
//! crash before the marker leaves the slot free and never a half record. A
//! committed slot whose checksum does not match is corrupt evidence: it is
//! never read as a valid record, and the route refuses rather than write.
//!
//! The next non-strict store open ingests each committed slot into
//! `foreground_terminals` (idempotently, by `run_id`) and then clears its commit
//! marker, which frees the slot for reuse. Until then `run status` and `runs`
//! read the slot directly, so a strict run's outcome is visible immediately.

use super::terminal::TerminalRecord;
use crate::ctx::EffectContext;
use crate::fs::{try_flock, FileName, Trust, FILE_MODE};
use crate::id::RunId;
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::root::StateRoot;
use crate::store::{Store, StoreError};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;
use std::time::{Duration, Instant};

/// The ring file, under `<checkout>/.effigy/state/`.
pub const RING_FILE: &str = "foreground-terminals.ring";
/// The temporary name `prepare` publishes the fully preallocated file from, so
/// a crash mid-preparation never leaves a short authoritative file.
const RING_TEMP: &str = "foreground-terminals.ring.new";
/// The file's first eight bytes: a ring written by another tool is refused.
const HEADER_MAGIC: &[u8; 8] = b"EFRGTERM";
/// The format version. A different version is refused, never reinterpreted.
const FORMAT_VERSION: u32 = 1;
/// The fixed header size.
pub const HEADER_BYTES: usize = 64;
/// Fixed slots in the ring: the most strict runs whose records can wait for
/// ingestion. A full ring refuses before admission; `effigy run prepare`
/// ingests it and frees the slots.
pub const SLOTS: usize = 16;
/// One slot's fixed size, and therefore the exact bytes one strict route
/// rewrites.
pub const SLOT_BYTES: usize = 1024;
/// The whole file: header plus every slot. Created once by `prepare` and never
/// grown by a route.
pub const RING_BYTES: u64 = (HEADER_BYTES + SLOTS * SLOT_BYTES) as u64;
/// How long the ring's own writer lock is waited for. The critical section is
/// one slot write or one clear, so a longer wait means something is wrong.
const LOCK_WAIT: Duration = Duration::from_secs(30);
const LOCK_POLL: Duration = Duration::from_millis(2);

// Header layout: magic, format version, slot count, slot size, header size,
// then the header's own SHA-256.
const HEADER_VERSION_OFF: usize = 8;
const HEADER_SLOTS_OFF: usize = 12;
const HEADER_SLOT_BYTES_OFF: usize = 16;
const HEADER_LEN_OFF: usize = 20;
const HEADER_CHECKSUM_OFF: usize = 32;
const HEADER_CHECKSUM_BYTES: usize = 32;

// Slot layout. Every text field is NUL-padded to its fixed width, so a slot's
// size never depends on its record. The field widths are the bounds
// `run::terminal` already refuses or cuts to.
const SLOT_MAGIC_OFF: usize = 0;
const SLOT_VERSION_OFF: usize = 4;
const SLOT_SEQ_OFF: usize = 8;
const SLOT_CLOSED_OFF: usize = 16;
const SLOT_GROUPS_OFF: usize = 24;
const SLOT_EXIT_FLAG_OFF: usize = 32;
const SLOT_EXIT_OFF: usize = 33;
const SLOT_SIGNAL_FLAG_OFF: usize = 37;
const SLOT_SIGNAL_OFF: usize = 38;
const SLOT_RUN_ID_OFF: usize = 46;
const SLOT_CHECKOUT_OFF: usize = SLOT_RUN_ID_OFF + super::terminal::RUN_ID_MAX;
const SLOT_BOOT_OFF: usize = SLOT_CHECKOUT_OFF + super::terminal::CHECKOUT_MAX;
const SLOT_TASK_OFF: usize = SLOT_BOOT_OFF + super::terminal::BOOT_MAX;
const SLOT_OUTCOME_OFF: usize = SLOT_TASK_OFF + super::terminal::TASK_MAX;
const SLOT_CLEANUP_OFF: usize = SLOT_OUTCOME_OFF + super::terminal::WORD_MAX;
const SLOT_CHECKSUM_BYTES: usize = 32;
const SLOT_CHECKSUM_OFF: usize = SLOT_BYTES - 1 - SLOT_CHECKSUM_BYTES;
const SLOT_COMMIT_OFF: usize = SLOT_BYTES - 1;
const SLOT_MAGIC: &[u8; 4] = b"EFTS";
const SLOT_VERSION: u16 = 1;
/// The commit marker. Every other byte value means the slot holds no record:
/// a body written before a crash is discarded, never read as a record.
pub(crate) const SLOT_COMMIT: u8 = 0xA5;

const _: () = assert!(SLOT_CLEANUP_OFF + super::terminal::WORD_MAX <= SLOT_CHECKSUM_OFF);
const _: () = assert!(SLOT_CHECKSUM_OFF + SLOT_CHECKSUM_BYTES == SLOT_COMMIT_OFF);
const _: () = assert!(SLOT_COMMIT_OFF + 1 == SLOT_BYTES);
const _: () = assert!(HEADER_CHECKSUM_OFF + HEADER_CHECKSUM_BYTES == HEADER_BYTES);

/// What the ring currently holds, for `effigy run prepare` and its `--check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RingStatus {
    /// Fixed slots in the file.
    pub slots: usize,
    /// Slots holding a committed record that no store open has ingested yet.
    pub committed: usize,
    /// Slots a strict route can write: never committed, or already ingested.
    pub free: usize,
    /// The whole file's fixed size.
    pub bytes: u64,
    /// The exact bytes one strict route rewrites.
    pub per_write_bytes: u64,
}

impl RingStatus {
    /// Whether a strict route could record right now. A full ring has to be
    /// ingested first, which `effigy run prepare` does outside the route.
    pub fn ready(&self) -> bool {
        self.free > 0
    }
}

/// One committed, not-yet-ingested slot, exactly as it was read. Ingestion
/// clears a slot only while it still holds these bytes, so a slot a concurrent
/// route reused is left alone.
#[derive(Debug)]
pub(crate) struct PendingSlot {
    index: usize,
    seq: u64,
    checksum: [u8; SLOT_CHECKSUM_BYTES],
    pub(crate) record: TerminalRecord,
}

fn name() -> FileName {
    FileName::new(RING_FILE).expect("fixed ring file name")
}

fn name_temp() -> FileName {
    FileName::new(RING_TEMP).expect("fixed temporary ring file name")
}

fn path(root: &StateRoot) -> std::path::PathBuf {
    root.path().join(RING_FILE)
}

fn refused(root: &StateRoot, reason: impl Into<String>) -> StoreError {
    StoreError::TerminalRing {
        path: path(root),
        reason: reason.into(),
    }
}

/// Prepares the ring outside any strict route: creates and fully preallocates a
/// missing file, and verifies an existing one. It never repairs or reuses a
/// file it cannot verify — that would discard records an ingested store has not
/// seen yet — so a mismatched file is refused with the step that clears it.
pub fn prepare(root: &StateRoot) -> Result<RingStatus, StoreError> {
    let name = name();
    if root.dir().entry(&name)?.is_some() {
        return status(root);
    }
    // Write the whole file through a temporary name and publish it by rename:
    // a crash during preallocation leaves a short temporary file, never a short
    // authoritative one.
    let temp = name_temp();
    let (file, _) = root
        .dir()
        .open_file(
            &temp,
            libc::O_RDWR | libc::O_TRUNC,
            Some(FILE_MODE),
            Trust::Private,
        )?
        .ok_or_else(|| refused(root, "the temporary ring file could not be created"))?;
    let mut bytes = vec![0_u8; RING_BYTES as usize];
    write_header(&mut bytes);
    // One write of every byte, so the filesystem allocates the whole file here
    // rather than on the route's first slot write.
    file.write_all_at(&bytes, 0)
        .map_err(|error| ring_io(root, "write", &error))?;
    file.set_len(RING_BYTES)
        .map_err(|error| ring_io(root, "truncate", &error))?;
    file.sync_all()
        .map_err(|error| ring_io(root, "flush", &error))?;
    drop(file);
    root.dir().rename(&temp, &name)?;
    root.dir().sync()?;
    status(root)
}

/// Verifies an existing ring without writing anything, and reports what it
/// holds. Used before admission and by `effigy run prepare --check`.
pub fn status(root: &StateRoot) -> Result<RingStatus, StoreError> {
    let image = read_image(root)?.ok_or_else(|| {
        refused(
            root,
            "does not exist; `effigy run prepare` creates and preallocates it outside a strict run",
        )
    })?;
    let mut committed = 0;
    for slot in image.slots() {
        if let Decoded::Committed(_) = decode(root, slot)? {
            committed += 1;
        }
    }
    Ok(RingStatus {
        slots: SLOTS,
        committed,
        free: SLOTS - committed,
        bytes: RING_BYTES,
        per_write_bytes: SLOT_BYTES as u64,
    })
}

/// The ring's committed records, oldest sequence first, for a reader that has
/// no store open to ingest them.
pub fn records(root: &StateRoot) -> Observed<Vec<TerminalRecord>> {
    let image = match read_image(root) {
        Ok(Some(image)) => image,
        Ok(None) => {
            return Observed::Absent(AbsenceProof::NoFile {
                name: RING_FILE.into(),
            })
        }
        Err(error) => return Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
    };
    let mut found = Vec::new();
    for slot in image.slots() {
        match decode(root, slot) {
            Ok(Decoded::Committed(committed)) => found.push((committed.seq, committed.record)),
            Ok(Decoded::Free) => {}
            Err(error) => return Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        }
    }
    found.sort_by_key(|(seq, _)| *seq);
    Observed::Verified(
        found.into_iter().map(|(_, record)| record).collect(),
        Evidence::new("strict terminal ring".into(), None).expect("fixed evidence source"),
    )
}

/// One run's record while it is still in the ring: the slot is read after
/// ingestion is gone, so `run status` sees a strict run's outcome immediately.
pub fn read(root: &StateRoot, run: RunId) -> Observed<Option<TerminalRecord>> {
    let wanted = run.to_string();
    match records(root) {
        Observed::Verified(records, evidence) => {
            match records.into_iter().find(|record| record.run_id == wanted) {
                Some(record) => Observed::Verified(Some(record), evidence),
                None => Observed::Absent(AbsenceProof::NoFile {
                    name: RING_FILE.into(),
                }),
            }
        }
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}

/// Writes one run's record into a free slot, in place.
///
/// Idempotent by `run_id`: a repeated settle of the same run finds its
/// committed slot and returns without writing, so a second settle neither
/// changes nor duplicates the slot. Two distinct runs take distinct slots and
/// distinct sequence numbers, so they never coalesce.
///
/// The write ordering is the crash contract: body (through the checksum) and
/// flush, then the commit marker and flush. A crash before the marker leaves
/// the slot free; a committed slot with a mismatched checksum is refused, never
/// read as a record.
pub(crate) fn write(root: &StateRoot, record: &TerminalRecord) -> Result<(), StoreError> {
    let record = record.checked()?;
    let (file, stat) = root
        .dir()
        .open_file(&name(), libc::O_RDWR, None, Trust::Private)?
        .ok_or_else(|| {
            refused(
                root,
                "does not exist; `effigy run prepare` creates and preallocates it outside a strict \
                 run",
            )
        })?;
    verify_size(root, stat.st_size)?;
    lock_exclusive(root, &file)?;
    let image = read_file(root, &file)?;
    let mut highest = 0_u64;
    let mut free = None;
    for (index, slot) in image.slots().enumerate() {
        match decode(root, slot)? {
            Decoded::Free => {
                free.get_or_insert(index);
            }
            Decoded::Committed(committed) => {
                if committed.record.run_id == record.run_id {
                    return Ok(());
                }
                highest = highest.max(committed.seq);
            }
        }
    }
    let Some(index) = free else {
        return Err(refused(
            root,
            format!(
                "has no free slot: all {SLOTS} slots hold records waiting for ingestion; \
                 `effigy run prepare` ingests them into the journal outside a strict run and frees \
                 them"
            ),
        ));
    };
    let seq = highest.checked_add(1).ok_or_else(|| {
        refused(
            root,
            "has reached its last sequence number; move it aside outside a strict run and then \
             `effigy run prepare` recreates it",
        )
    })?;
    let slot = encode(&record, seq);
    let offset = (HEADER_BYTES + index * SLOT_BYTES) as u64;
    file.write_all_at(&slot[..SLOT_COMMIT_OFF], offset)
        .map_err(|error| ring_io(root, "write", &error))?;
    // The body must be durable before the marker names it: a marker that
    // outlived a torn body would make a half record look committed.
    file.sync_all()
        .map_err(|error| ring_io(root, "flush", &error))?;
    file.write_all_at(&slot[SLOT_COMMIT_OFF..], offset + SLOT_COMMIT_OFF as u64)
        .map_err(|error| ring_io(root, "write", &error))?;
    file.sync_all()
        .map_err(|error| ring_io(root, "flush", &error))?;
    Ok(())
}

/// The committed slots a non-strict store open has not ingested yet, oldest
/// sequence first. Absence is an empty list: a checkout that never ran strict
/// has no ring and nothing to ingest.
pub(crate) fn pending(root: &StateRoot) -> Result<Vec<PendingSlot>, StoreError> {
    let Some(image) = read_image(root)? else {
        return Ok(Vec::new());
    };
    let mut pending = Vec::new();
    for (index, slot) in image.slots().enumerate() {
        match decode(root, slot)? {
            Decoded::Free => {}
            Decoded::Committed(committed) => pending.push(PendingSlot {
                index,
                seq: committed.seq,
                checksum: committed.checksum,
                record: committed.record,
            }),
        }
    }
    pending.sort_by_key(|slot| slot.seq);
    Ok(pending)
}

/// Frees the slots ingestion copied into `foreground_terminals`, in one flush.
/// A slot is cleared only while it still holds the bytes that were ingested, so
/// a slot a route reused in the meantime keeps its new record.
pub(crate) fn clear_ingested(
    root: &StateRoot,
    ingested: &[PendingSlot],
) -> Result<usize, StoreError> {
    if ingested.is_empty() {
        return Ok(0);
    }
    let Some((file, stat)) = root
        .dir()
        .open_file(&name(), libc::O_RDWR, None, Trust::Private)?
    else {
        return Ok(0);
    };
    verify_size(root, stat.st_size)?;
    lock_exclusive(root, &file)?;
    let image = read_file(root, &file)?;
    let mut cleared = 0;
    for slot in ingested {
        if let Ok(Decoded::Committed(committed)) = decode(root, image.slot(slot.index)) {
            if committed.seq == slot.seq && committed.checksum == slot.checksum {
                file.write_all_at(
                    &[0_u8],
                    (HEADER_BYTES + slot.index * SLOT_BYTES + SLOT_COMMIT_OFF) as u64,
                )
                .map_err(|error| ring_io(root, "write", &error))?;
                cleared += 1;
            }
        }
    }
    if cleared > 0 {
        file.sync_all()
            .map_err(|error| ring_io(root, "flush", &error))?;
    }
    Ok(cleared)
}

/// Ingests every committed slot into the checkout journal, then frees the ones
/// that were copied. This is the one place strict records enter SQLite, and it
/// runs only on a non-strict store open, outside any strict route.
pub(crate) fn ingest(
    cx: &EffectContext,
    store: &mut Store,
    root: &StateRoot,
) -> Result<usize, StoreError> {
    let pending = pending(root)?;
    if pending.is_empty() {
        return Ok(0);
    }
    store.txn(cx, |txn| {
        for slot in &pending {
            super::terminal::insert_row(txn, &slot.record)?;
        }
        Ok(())
    })?;
    clear_ingested(root, &pending)
}

/// The parsed file: the header has been verified and both the size and the
/// header checksum already hold.
struct Image {
    bytes: Vec<u8>,
}

impl Image {
    fn slot(&self, index: usize) -> &[u8] {
        let start = HEADER_BYTES + index * SLOT_BYTES;
        &self.bytes[start..start + SLOT_BYTES]
    }

    fn slots(&self) -> std::slice::ChunksExact<'_, u8> {
        self.bytes[HEADER_BYTES..].chunks_exact(SLOT_BYTES)
    }
}

enum Decoded {
    Free,
    Committed(Committed),
}

struct Committed {
    seq: u64,
    checksum: [u8; SLOT_CHECKSUM_BYTES],
    record: TerminalRecord,
}

fn ring_io(root: &StateRoot, op: &'static str, error: &io::Error) -> StoreError {
    StoreError::Io(crate::fs::IoError::new(op, &path(root), error))
}

fn verify_size(root: &StateRoot, size: i64) -> Result<(), StoreError> {
    let size = size.max(0) as u64;
    if size == RING_BYTES {
        return Ok(());
    }
    Err(refused(
        root,
        format!(
            "is {size} bytes instead of the fixed {RING_BYTES}; nothing was written, so move it \
             aside outside a strict run and then `effigy run prepare` recreates it"
        ),
    ))
}

fn verify_header(root: &StateRoot, bytes: &[u8]) -> Result<(), StoreError> {
    if &bytes[..8] != HEADER_MAGIC {
        return Err(refused(
            root,
            "does not hold this build's ring header; nothing was written, so move it aside outside \
             a strict run and then `effigy run prepare` recreates it",
        ));
    }
    let version = u32::from_le_bytes(
        bytes[HEADER_VERSION_OFF..HEADER_VERSION_OFF + 4]
            .try_into()
            .expect("fixed slice"),
    );
    let slots = u32::from_le_bytes(
        bytes[HEADER_SLOTS_OFF..HEADER_SLOTS_OFF + 4]
            .try_into()
            .expect("fixed slice"),
    );
    let slot_bytes = u32::from_le_bytes(
        bytes[HEADER_SLOT_BYTES_OFF..HEADER_SLOT_BYTES_OFF + 4]
            .try_into()
            .expect("fixed slice"),
    );
    let header_bytes = u32::from_le_bytes(
        bytes[HEADER_LEN_OFF..HEADER_LEN_OFF + 4]
            .try_into()
            .expect("fixed slice"),
    );
    if version != FORMAT_VERSION
        || slots as usize != SLOTS
        || slot_bytes as usize != SLOT_BYTES
        || header_bytes as usize != HEADER_BYTES
    {
        return Err(refused(
            root,
            format!(
                "records format {version}, {slots} slots of {slot_bytes} bytes and a \
                 {header_bytes}-byte header, and this build writes format {FORMAT_VERSION}, {SLOTS} \
                 slots of {SLOT_BYTES} bytes and a {HEADER_BYTES}-byte header; move it aside outside \
                 a strict run and then `effigy run prepare` recreates it"
            ),
        ));
    }
    let checksum = Sha256::digest(&bytes[..HEADER_CHECKSUM_OFF]);
    if checksum.as_slice() != &bytes[HEADER_CHECKSUM_OFF..HEADER_BYTES] {
        return Err(refused(
            root,
            "has a header checksum that does not match; nothing was written, so move it aside \
             outside a strict run and then `effigy run prepare` recreates it",
        ));
    }
    Ok(())
}

fn write_header(bytes: &mut [u8]) {
    bytes[..8].copy_from_slice(HEADER_MAGIC);
    bytes[HEADER_VERSION_OFF..HEADER_VERSION_OFF + 4]
        .copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes[HEADER_SLOTS_OFF..HEADER_SLOTS_OFF + 4].copy_from_slice(&(SLOTS as u32).to_le_bytes());
    bytes[HEADER_SLOT_BYTES_OFF..HEADER_SLOT_BYTES_OFF + 4]
        .copy_from_slice(&(SLOT_BYTES as u32).to_le_bytes());
    bytes[HEADER_LEN_OFF..HEADER_LEN_OFF + 4].copy_from_slice(&(HEADER_BYTES as u32).to_le_bytes());
    let checksum = Sha256::digest(&bytes[..HEADER_CHECKSUM_OFF]);
    bytes[HEADER_CHECKSUM_OFF..HEADER_BYTES].copy_from_slice(&checksum);
}

/// Reads and verifies an existing ring. `None` is absence: a checkout that
/// never prepared a ring has nothing to read.
fn read_image(root: &StateRoot) -> Result<Option<Image>, StoreError> {
    let Some((file, stat)) = root
        .dir()
        .open_file(&name(), libc::O_RDONLY, None, Trust::Private)?
    else {
        return Ok(None);
    };
    verify_size(root, stat.st_size)?;
    Ok(Some(read_file(root, &file)?))
}

fn read_file(root: &StateRoot, file: &File) -> Result<Image, StoreError> {
    let bytes = crate::fs::read_bounded(file, RING_BYTES)
        .map_err(|error| ring_io(root, "read", &error))?
        .ok_or_else(|| {
            refused(
                root,
                format!("is longer than the fixed {RING_BYTES} bytes; nothing was written"),
            )
        })?;
    if bytes.len() as u64 != RING_BYTES {
        return Err(refused(
            root,
            format!(
                "is {} bytes instead of the fixed {RING_BYTES}; nothing was written, so move it \
                 aside outside a strict run and then `effigy run prepare` recreates it",
                bytes.len()
            ),
        ));
    }
    verify_header(root, &bytes)?;
    Ok(Image { bytes })
}

/// Takes the ring's own writer lock, so two strict routes settling together
/// take distinct slots. The lock is on the ring file itself: it is not another
/// file, and it writes no byte.
fn lock_exclusive(root: &StateRoot, file: &File) -> Result<(), StoreError> {
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        match try_flock(file, libc::LOCK_EX) {
            Ok(true) => return Ok(()),
            Ok(false) if Instant::now() < deadline => std::thread::sleep(LOCK_POLL),
            Ok(false) => {
                return Err(StoreError::Busy {
                    waited: LOCK_WAIT,
                    holder: Box::new(Observed::Unknown(UnknownReason::Inconsistent(
                        "the strict terminal ring's writer lock stayed held".into(),
                    ))),
                })
            }
            Err(error) => return Err(ring_io(root, "lock", &error)),
        }
    }
}

fn decode(root: &StateRoot, slot: &[u8]) -> Result<Decoded, StoreError> {
    debug_assert_eq!(slot.len(), SLOT_BYTES);
    if slot[SLOT_COMMIT_OFF] != SLOT_COMMIT {
        return Ok(Decoded::Free);
    }
    let checksum: [u8; SLOT_CHECKSUM_BYTES] = slot[SLOT_CHECKSUM_OFF..SLOT_COMMIT_OFF]
        .try_into()
        .expect("fixed checksum slice");
    let computed = Sha256::digest(&slot[..SLOT_CHECKSUM_OFF]);
    if &checksum[..] != computed.as_slice()
        || &slot[SLOT_MAGIC_OFF..SLOT_MAGIC_OFF + 4] != SLOT_MAGIC
    {
        return Err(refused(
            root,
            "holds a committed slot whose bytes do not match its checksum; nothing was written, \
             so move the file aside outside a strict run (this loses the records it still holds) \
             and then `effigy run prepare` recreates it",
        ));
    }
    let version = u16::from_le_bytes(
        slot[SLOT_VERSION_OFF..SLOT_VERSION_OFF + 2]
            .try_into()
            .expect("fixed slice"),
    );
    if version != SLOT_VERSION {
        return Err(refused(
            root,
            format!(
                "holds a committed slot at format {version}, and this build writes {SLOT_VERSION}; \
                 move it aside outside a strict run and then `effigy run prepare` recreates it"
            ),
        ));
    }
    let seq = u64::from_le_bytes(
        slot[SLOT_SEQ_OFF..SLOT_SEQ_OFF + 8]
            .try_into()
            .expect("fixed slice"),
    );
    let closed_at_ms = i64::from_le_bytes(
        slot[SLOT_CLOSED_OFF..SLOT_CLOSED_OFF + 8]
            .try_into()
            .expect("fixed slice"),
    );
    let workload_groups = u64::from_le_bytes(
        slot[SLOT_GROUPS_OFF..SLOT_GROUPS_OFF + 8]
            .try_into()
            .expect("fixed slice"),
    );
    let exit_code = optional_i32(slot, SLOT_EXIT_FLAG_OFF, SLOT_EXIT_OFF);
    let signal = optional_i32(slot, SLOT_SIGNAL_FLAG_OFF, SLOT_SIGNAL_OFF);
    let record = TerminalRecord {
        run_id: text(root, slot, SLOT_RUN_ID_OFF, super::terminal::RUN_ID_MAX)?,
        checkout: text(root, slot, SLOT_CHECKOUT_OFF, super::terminal::CHECKOUT_MAX)?,
        boot: text(root, slot, SLOT_BOOT_OFF, super::terminal::BOOT_MAX)?,
        task: text(root, slot, SLOT_TASK_OFF, super::terminal::TASK_MAX)?,
        outcome: text(root, slot, SLOT_OUTCOME_OFF, super::terminal::WORD_MAX)?,
        cleanup: text(root, slot, SLOT_CLEANUP_OFF, super::terminal::WORD_MAX)?,
        exit_code,
        signal,
        workload_groups,
        closed_at_ms,
    };
    Ok(Decoded::Committed(Committed {
        seq,
        checksum,
        record,
    }))
}

fn optional_i32(slot: &[u8], flag: usize, offset: usize) -> Option<i32> {
    (slot[flag] != 0)
        .then(|| i32::from_le_bytes(slot[offset..offset + 4].try_into().expect("fixed slice")))
}

fn text(root: &StateRoot, slot: &[u8], offset: usize, len: usize) -> Result<String, StoreError> {
    let raw = &slot[offset..offset + len];
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(len);
    if raw[end..].iter().any(|byte| *byte != 0) {
        return Err(refused(
            root,
            "holds a committed slot whose text is not NUL-padded; nothing was written",
        ));
    }
    String::from_utf8(raw[..end].to_vec()).map_err(|_| {
        refused(
            root,
            "holds a committed slot whose text is not valid UTF-8; nothing was written",
        )
    })
}

fn encode(record: &TerminalRecord, seq: u64) -> [u8; SLOT_BYTES] {
    let mut slot = [0_u8; SLOT_BYTES];
    slot[SLOT_MAGIC_OFF..SLOT_MAGIC_OFF + 4].copy_from_slice(SLOT_MAGIC);
    slot[SLOT_VERSION_OFF..SLOT_VERSION_OFF + 2].copy_from_slice(&SLOT_VERSION.to_le_bytes());
    slot[SLOT_SEQ_OFF..SLOT_SEQ_OFF + 8].copy_from_slice(&seq.to_le_bytes());
    slot[SLOT_CLOSED_OFF..SLOT_CLOSED_OFF + 8].copy_from_slice(&record.closed_at_ms.to_le_bytes());
    slot[SLOT_GROUPS_OFF..SLOT_GROUPS_OFF + 8]
        .copy_from_slice(&record.workload_groups.to_le_bytes());
    if let Some(code) = record.exit_code {
        slot[SLOT_EXIT_FLAG_OFF] = 1;
        slot[SLOT_EXIT_OFF..SLOT_EXIT_OFF + 4].copy_from_slice(&code.to_le_bytes());
    }
    if let Some(signal) = record.signal {
        slot[SLOT_SIGNAL_FLAG_OFF] = 1;
        slot[SLOT_SIGNAL_OFF..SLOT_SIGNAL_OFF + 4].copy_from_slice(&signal.to_le_bytes());
    }
    write_text(&mut slot, SLOT_RUN_ID_OFF, &record.run_id);
    write_text(&mut slot, SLOT_CHECKOUT_OFF, &record.checkout);
    write_text(&mut slot, SLOT_BOOT_OFF, &record.boot);
    write_text(&mut slot, SLOT_TASK_OFF, &record.task);
    write_text(&mut slot, SLOT_OUTCOME_OFF, &record.outcome);
    write_text(&mut slot, SLOT_CLEANUP_OFF, &record.cleanup);
    let checksum = Sha256::digest(&slot[..SLOT_CHECKSUM_OFF]);
    slot[SLOT_CHECKSUM_OFF..SLOT_COMMIT_OFF].copy_from_slice(&checksum);
    slot[SLOT_COMMIT_OFF] = SLOT_COMMIT;
    slot
}

fn write_text(slot: &mut [u8], offset: usize, value: &str) {
    debug_assert!(value.len() <= slot.len() - offset);
    slot[offset..offset + value.len()].copy_from_slice(value.as_bytes());
}
