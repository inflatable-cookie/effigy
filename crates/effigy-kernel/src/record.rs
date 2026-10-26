//! File records outside `kernel.db` (I-S6): `atomic_replace`, bounded typed
//! reads, and the one-pointer commit for multi-file artefacts (§5.5).
use crate::ctx::{EffectContext, ObserveContext};
use crate::fs::{self, DirFd, FileName, FsError, IoError, Trust};
use crate::gate::GateGuard;
use crate::id::{Generation, ResourceKind};
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::store::StoreError;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);
const TEMP_MARK: &str = ".effigy-tmp-";
const POINTER_MAX: u64 = 64 * 1024;
const LIST_MAX: usize = 4096;

/// A typed file record. Decoding failure is `Unknown`, never a default.
pub trait Record: Sized {
    const MAX_BYTES: u64;
    fn decode(bytes: &[u8]) -> Result<Self, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplaceOpts {
    /// Final mode; group or other write bits are refused.
    pub mode: u32,
    /// Hard-link the current file to `<name>.prev` before the rename.
    pub keep_previous: bool,
}
impl Default for ReplaceOpts {
    fn default() -> Self {
        Self {
            mode: fs::FILE_MODE,
            keep_previous: false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Replaced {
    pub previous_kept: bool,
}

/// Temp, write, fsync, optional `.prev` link, `renameat`, directory fsync.
/// The target is always the old or the new file, never torn (I-S4). The
/// caller holds the target's store gate; leftover temporaries from a killed
/// writer are removed here, under that gate.
pub fn atomic_replace(
    cx: &EffectContext,
    guard: &GateGuard,
    dir: &DirFd,
    name: &FileName,
    bytes: &[u8],
    opts: ReplaceOpts,
) -> Result<Replaced, StoreError> {
    if !guard.ids().any(|id| id.kind() == ResourceKind::Store) {
        return Err(StoreError::Refused(
            "this record change needs the write guard for its resource".into(),
        ));
    }
    if opts.mode & !0o777 != 0 || opts.mode & 0o022 != 0 {
        return Err(StoreError::Untrusted {
            path: dir.join(name.as_str()),
            reason: "group- or other-writable mode".into(),
        });
    }
    cx.operation()
        .check("atomic replace")
        .map_err(StoreError::Interrupted)?;
    let target = dir.join(name.as_str());
    if let Some(stat) = dir.entry(name)? {
        fs::check(&stat, Trust::Operator, fs::Kind::File, &target)?;
    }
    remove_temporaries(dir, name)?;
    let temp = name.with_suffix(&format!(
        "{TEMP_MARK}{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))?;
    let temp = FileName::new(&format!(".{temp}"))?;
    let io = |op, error: std::io::Error| StoreError::Io(IoError::new(op, &target, &error));

    crate::fault_point!(ReplaceOpen).map_err(|e| io("open", e))?;
    let (mut file, _) = dir
        .open_file(
            &temp,
            libc::O_WRONLY | libc::O_EXCL,
            Some(opts.mode),
            Trust::Operator,
        )?
        .expect("created temporary");
    let written = (|| {
        fs::fchmod(&file, opts.mode).map_err(|e| io("chmod", e))?;
        crate::fault_point!(ReplaceWrite).map_err(|e| io("write", e))?;
        file.write_all(bytes).map_err(|e| io("write", e))?;
        crate::fault_point!(ReplaceFsync).map_err(|e| io("fsync", e))?;
        fs::fsync(&file).map_err(|e| io("fsync", e))?;
        Ok::<_, StoreError>(())
    })();
    drop(file);
    let mut previous_kept = false;
    let renamed = written.and_then(|()| {
        if opts.keep_previous && dir.entry(name)?.is_some() {
            crate::fault_point!(ReplaceLinkPrev).map_err(|e| io("link", e))?;
            let staged = FileName::new(&format!(".{}", temp.as_str().trim_start_matches('.')))?
                .with_suffix(".prev")?;
            dir.link(name, &staged)?;
            dir.rename(&staged, &name.with_suffix(".prev")?)?;
            // rename(2) is a no-op when both names already link one inode
            // (a retry after a failed replace), which would leave `staged`.
            dir.remove(&staged, false)?;
            previous_kept = true;
        }
        crate::fault_point!(ReplaceRename).map_err(|e| io("rename", e))?;
        dir.rename(&temp, name)?;
        Ok(())
    });
    if let Err(error) = renamed {
        let _ = dir.remove(&temp, false);
        return Err(error);
    }
    crate::fault_point!(ReplaceDirFsync).map_err(|e| io("fsync", e))?;
    dir.sync()?;
    Ok(Replaced { previous_kept })
}

/// Open a regular output record for a child, under its store gate. Check the
/// inode before truncation; stdout/stderr may share clones of this descriptor.
/// The caller keeps the guard until the child closes its descriptors.
pub fn open_output(
    cx: &EffectContext,
    guard: &GateGuard,
    dir: &DirFd,
    name: &FileName,
) -> Result<std::fs::File, StoreError> {
    record_effect(cx, guard)?;
    let (file, stat) = dir
        .open_file(name, libc::O_WRONLY, Some(fs::FILE_MODE), Trust::Operator)?
        .ok_or_else(|| {
            StoreError::Broken("an output record Effigy just created is missing".into())
        })?;
    if stat.st_nlink != 1 {
        return Err(StoreError::Untrusted {
            path: dir.path().join(name.as_str()),
            reason: "hard-linked output record".into(),
        });
    }
    fs::fchmod(&file, fs::FILE_MODE).map_err(|error| {
        StoreError::Io(IoError::new(
            "protect output",
            &dir.path().join(name.as_str()),
            &error,
        ))
    })?;
    file.set_len(0).map_err(|error| {
        StoreError::Io(IoError::new(
            "truncate output",
            &dir.path().join(name.as_str()),
            &error,
        ))
    })?;
    Ok(file)
}

/// Remove an owner-checked regular record without following a symlink.
/// Cache retirement must supply the same store gate used by publication.
pub fn remove_record(
    cx: &EffectContext,
    guard: &GateGuard,
    dir: &DirFd,
    name: &FileName,
) -> Result<(), StoreError> {
    record_effect(cx, guard)?;
    if let Some(stat) = dir.entry(name)? {
        fs::check(
            &stat,
            Trust::Operator,
            fs::Kind::File,
            &dir.path().join(name.as_str()),
        )?;
        if stat.st_nlink != 1 {
            return Err(StoreError::Untrusted {
                path: dir.path().join(name.as_str()),
                reason: "hard-linked cache record".into(),
            });
        }
        dir.remove(name, false)?;
        dir.sync()?;
    }
    Ok(())
}

fn record_effect(cx: &EffectContext, guard: &GateGuard) -> Result<(), StoreError> {
    cx.require(crate::ctx::Capability::HostAuthority)
        .map_err(|_| StoreError::HostAuthorityRequired)?;
    cx.operation()
        .check("record effect")
        .map_err(StoreError::Interrupted)?;
    if !guard.ids().any(|id| id.kind() == ResourceKind::Store) {
        return Err(StoreError::Refused(
            "this record effect needs the write guard for its resource".into(),
        ));
    }
    Ok(())
}

/// Bounded, no-follow, owner-checked read (I-S3).
pub fn read_record<T: Record>(cx: &ObserveContext, dir: &DirFd, name: &FileName) -> Observed<T> {
    #[cfg(all(test, feature = "fault-injection"))]
    if let Some(injected) = crate::fault::injected() {
        return injected;
    }
    match read_bytes(cx, dir, name, T::MAX_BYTES) {
        Observed::Verified(bytes, evidence) => match T::decode(&bytes) {
            Ok(value) => Observed::Verified(value, evidence),
            Err(reason) => Observed::Unknown(UnknownReason::Unparsable(reason)),
        },
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}
/// A derived cache read: anything but `Verified` is a miss (I-S7).
pub fn read_cache<T: Record>(cx: &ObserveContext, dir: &DirFd, name: &FileName) -> Option<T> {
    match read_record(cx, dir, name) {
        Observed::Verified(value, _) => Some(value),
        _ => None,
    }
}
fn read_bytes(cx: &ObserveContext, dir: &DirFd, name: &FileName, max: u64) -> Observed<Vec<u8>> {
    if cx.check("record read").is_err() {
        return Observed::Unknown(UnknownReason::TimedOut {
            phase: "record read".into(),
        });
    }
    let (file, stat) = match dir.open_file(name, libc::O_RDONLY, None, Trust::Operator) {
        Ok(Some(opened)) => opened,
        Ok(None) => {
            return Observed::Absent(AbsenceProof::NoFile {
                name: name.as_str().into(),
            })
        }
        Err(error) => return Observed::Unknown(crate::root::unknown(&error)),
    };
    if stat.st_size as u64 > max {
        return Observed::Unknown(UnknownReason::Unparsable("oversized record".into()));
    }
    match fs::read_bounded(&file, max) {
        Ok(Some(bytes)) => Observed::Verified(
            bytes,
            Evidence::new("kernel file record".into(), None).expect("fixed source"),
        ),
        Ok(None) => Observed::Unknown(UnknownReason::Unparsable("oversized record".into())),
        Err(error) => Observed::Unknown(crate::root::unknown(&FsError::io(
            "read",
            &dir.join(name.as_str()),
            &error,
        ))),
    }
}
fn remove_temporaries(dir: &DirFd, name: &FileName) -> Result<(), StoreError> {
    let prefix = format!(".{}{TEMP_MARK}", name.as_str());
    for entry in dir.list(LIST_MAX)? {
        if let Some(entry) = entry.to_str().filter(|entry| entry.starts_with(&prefix)) {
            dir.remove(&FileName::new(entry)?, false)?;
        }
    }
    Ok(())
}

/// One committed generation of a multi-file artefact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationSet {
    pub generation: Generation,
    pub parts: Vec<(String, Vec<u8>)>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pointer {
    v: u8,
    generation: u64,
    parts: Vec<PointerPart>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PointerPart {
    stem: String,
    file: String,
    sha256: String,
}
impl Record for Pointer {
    const MAX_BYTES: u64 = POINTER_MAX;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let pointer: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if pointer.v != 1 {
            return Err("unknown pointer version".into());
        }
        Ok(pointer)
    }
}

/// Writes every part under a generation-suffixed name, then selects them
/// with one `atomic_replace` of the pointer. Two renames are not a
/// transaction; one pointer is. Generations must strictly increase.
pub fn commit_generation(
    cx: &EffectContext,
    guard: &GateGuard,
    dir: &DirFd,
    pointer: &FileName,
    generation: Generation,
    parts: &[(&FileName, &[u8])],
    part_max: u64,
) -> Result<(), StoreError> {
    let left = cx.operation().remaining().ok_or(StoreError::Unbounded)?;
    let observe = cx
        .operation()
        .child(left)
        .observe()
        .map_err(|_| StoreError::Unbounded)?;
    let previous = match read_record::<Pointer>(&observe, dir, pointer) {
        Observed::Verified(current, _) => Some(current.generation),
        Observed::Absent(_) => None,
        // Unreadable current pointer: still committable, since the new
        // generation is complete on its own once selected.
        Observed::Unknown(_) => None,
    };
    if let Some(current) = previous.filter(|previous| *previous >= generation.get()) {
        return Err(StoreError::StaleGeneration { current });
    }
    let mut entries = Vec::new();
    for (stem, bytes) in parts {
        if bytes.len() as u64 > part_max {
            return Err(StoreError::TooLarge { limit: part_max });
        }
        let file = stem.with_suffix(&format!(".g{}", generation.get()))?;
        atomic_replace(cx, guard, dir, &file, bytes, ReplaceOpts::default())?;
        entries.push(PointerPart {
            stem: stem.as_str().into(),
            file: file.as_str().into(),
            sha256: hex(&Sha256::digest(bytes)),
        });
    }
    let body = serde_json::to_vec(&Pointer {
        v: 1,
        generation: generation.get(),
        parts: entries,
    })
    .expect("serializable pointer");
    atomic_replace(
        cx,
        guard,
        dir,
        pointer,
        &body,
        ReplaceOpts {
            keep_previous: true,
            ..ReplaceOpts::default()
        },
    )?;
    // Keep the selected and previous generations; older parts are garbage.
    if let Some(previous) = previous {
        for (stem, _) in parts {
            let prefix = format!("{}.g", stem.as_str());
            for entry in dir.list(LIST_MAX)? {
                let Some(entry) = entry.to_str() else {
                    continue;
                };
                let older = entry
                    .strip_prefix(&prefix)
                    .and_then(|number| number.parse::<u64>().ok())
                    .is_some_and(|number| number < previous);
                if older {
                    dir.remove(&FileName::new(entry)?, false)?;
                }
            }
        }
    }
    Ok(())
}
/// Resolves exactly one complete generation, or `Unknown`; never a mix.
pub fn read_generation(
    cx: &ObserveContext,
    dir: &DirFd,
    pointer: &FileName,
    part_max: u64,
) -> Observed<GenerationSet> {
    let current = match read_record::<Pointer>(cx, dir, pointer) {
        Observed::Verified(current, _) => current,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    let mut parts = Vec::new();
    for part in &current.parts {
        let Ok(name) = FileName::new(&part.file) else {
            return Observed::Unknown(UnknownReason::Unparsable("pointer part name".into()));
        };
        match read_bytes(cx, dir, &name, part_max) {
            Observed::Verified(bytes, _) if hex(&Sha256::digest(&bytes)) == part.sha256 => {
                parts.push((part.stem.clone(), bytes));
            }
            Observed::Verified(..) | Observed::Absent(_) => {
                return Observed::Unknown(UnknownReason::Inconsistent(
                    "pointer names an incomplete generation".into(),
                ))
            }
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        }
    }
    Observed::Verified(
        GenerationSet {
            generation: Generation::new(current.generation),
            parts,
        },
        Evidence::new("kernel generation pointer".into(), None).expect("fixed source"),
    )
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
