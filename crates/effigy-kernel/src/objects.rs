//! Immutable file closures (§7.1–7.4). Publication and retention share one
//! short store gate; the owning run journals staging before creating it.
use crate::ctx::{EffectClass, EffectContext, ObserveContext};
use crate::fs::{self, DirFd, FileName, FsError, Trust};
use crate::gate::{Gate, GateGuard, Wait};
use crate::id::ResourceId;
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::root::{Family, StateRoot};
use crate::store::{Store, StoreError};
use rusqlite::OptionalExtension;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

pub(crate) const KIND: &str = "kernel:object-staging";
const MAX_FILES: usize = 4096;
const MAX_DEPTH: usize = 16;
const MAX_BYTES: usize = 64 << 20;

/// A pure, canonical closure. Names are relative UTF-8 paths; publication
/// keeps the private modes (files 0600, directories 0700) and the digest alone
/// proves integrity. No host path or registry enters the digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closure {
    files: BTreeMap<String, Vec<u8>>,
    digest: String,
}
impl Closure {
    pub fn new(files: BTreeMap<String, Vec<u8>>) -> Result<Self, StoreError> {
        if files.is_empty() || files.len() > MAX_FILES {
            return Err(StoreError::TooLarge {
                limit: MAX_FILES as u64,
            });
        }
        let mut size = 0usize;
        let mut hash = Sha256::new();
        hash.update(b"effigy.object.closure\0\x01");
        for (path, bytes) in &files {
            let parts = components(path)?;
            if parts.len() > MAX_DEPTH {
                return Err(invalid("object path too deep"));
            }
            let mut prefix = String::new();
            for component in parts.iter().take(parts.len() - 1) {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(component);
                if files.contains_key(&prefix) {
                    return Err(invalid("file is also a directory"));
                }
            }
            size = size.checked_add(bytes.len()).ok_or(StoreError::TooLarge {
                limit: MAX_BYTES as u64,
            })?;
            if size > MAX_BYTES {
                return Err(StoreError::TooLarge {
                    limit: MAX_BYTES as u64,
                });
            }
            hash.update((path.len() as u64).to_be_bytes());
            hash.update(path.as_bytes());
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(bytes);
        }
        let digest = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self { files, digest })
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
    pub fn size(&self) -> usize {
        self.files.values().map(Vec::len).sum()
    }
}
#[derive(Debug, Clone, Copy)]
pub enum Role {
    Desired,
    LastApplied,
}
impl Role {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Desired => "desired",
            Self::LastApplied => "last_applied",
        }
    }
}
/// A verified read snapshot, including the location engines give their backend.
#[derive(Debug)]
pub struct Object {
    pub path: PathBuf,
    pub closure: Closure,
}
#[derive(Debug)]
pub struct Objects;
impl Objects {
    /// Resolve and read a complete referenced closure. Absence and corruption
    /// never create state, locks, staging or registry allocations (R2).
    pub fn resolve(
        cx: &ObserveContext,
        root: &StateRoot,
        resource: &ResourceId,
        role: Role,
    ) -> Observed<Object> {
        if cx.placement() == crate::ctx::Placement::Guest {
            return Observed::Unknown(UnknownReason::HostAuthorityRequired);
        }
        let reader = match Store::read(cx, root) {
            Observed::Verified(reader, _) => reader,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        match reader.resource(resource) {
            Observed::Verified(_, _) => {}
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        }
        let result = (|| -> Result<Option<Object>, StoreError> {
            cx.check("object read").map_err(StoreError::Interrupted)?;
            let digest: Option<String> = reader
                .conn()
                .query_row(
                    "SELECT digest FROM object_refs WHERE resource=?1 AND role=?2",
                    rusqlite::params![resource.name().to_string(), role.name()],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(digest) = digest else {
                return Ok(None);
            };
            let dir = root
                .read_family(Family::Objects)?
                .ok_or_else(|| invalid("object family missing"))?;
            let object = open(&dir, &digest, cx)?;
            Ok(Some(object))
        })();
        match result {
            Ok(Some(object)) => Observed::Verified(
                object,
                Evidence::new("kernel object closure".into(), None).expect("fixed source"),
            ),
            Ok(None) => Observed::Absent(AbsenceProof::NoRow {
                table: "object_refs".into(),
            }),
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        }
    }
}
/// An object store this Effigy wrote no longer matches its own invariants: a
/// broken record, never a corrupt `kernel.db`, so it never suggests the repair.
pub(crate) fn invalid(reason: &str) -> StoreError {
    StoreError::Broken(format!("the object store: {reason}"))
}
fn components(path: &str) -> Result<Vec<&str>, StoreError> {
    let parts: Vec<_> = path.split('/').collect();
    for part in &parts {
        FileName::new(part)?;
    }
    Ok(parts)
}
pub(crate) fn validate_digest(digest: &str) -> Result<FileName, StoreError> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(invalid("invalid object digest"));
    }
    Ok(FileName::new(digest)?)
}
pub(crate) fn lock(cx: &EffectContext, root: &StateRoot) -> Result<GateGuard, StoreError> {
    if !matches!(
        cx.operation().class(),
        EffectClass::Prepare | EffectClass::Mutate
    ) {
        return Err(StoreError::ClassRefused(cx.operation().class()));
    }
    let deadline = match cx.operation().deadline() {
        crate::ctx::Deadline::At(at) => at,
        _ => return Err(StoreError::Unbounded),
    };
    Ok(Gate::acquire(
        cx,
        root,
        &[root.store_gate(cx, "objects")?],
        Wait::Until(deadline),
    )?)
}
pub(crate) fn write(dir: &DirFd, closure: &Closure, cx: &EffectContext) -> Result<(), StoreError> {
    for (path, bytes) in &closure.files {
        cx.operation()
            .check("object staging")
            .map_err(StoreError::Interrupted)?;
        let parts = components(path)?;
        crate::fault_point!(ObjectWriteBefore).map_err(|e| FsError::io("write", dir.path(), &e))?;
        write_file(dir, &parts, bytes)?;
        crate::fault_point!(ObjectWriteAfter).map_err(|e| FsError::io("write", dir.path(), &e))?;
    }
    sync_tree(dir, MAX_DEPTH)?;
    Ok(())
}
fn write_file(dir: &DirFd, parts: &[&str], bytes: &[u8]) -> Result<(), StoreError> {
    let (part, rest) = parts.split_first().expect("validated path");
    if !rest.is_empty() {
        let child = dir
            .child(part, Trust::Private, true)?
            .expect("created directory");
        return write_file(&child, rest, bytes);
    }
    let (mut file, _) = dir
        .open_file(
            &FileName::new(part)?,
            libc::O_WRONLY | libc::O_EXCL,
            Some(0o600),
            Trust::Private,
        )?
        .expect("created file");
    file.write_all(bytes)
        .map_err(|e| FsError::io("write", dir.path(), &e))?;
    fs::fsync(&file).map_err(|e| FsError::io("fsync", dir.path(), &e))?;
    Ok(())
}
/// Fsyncs every staged directory so the rename publishes a durable tree.
fn sync_tree(dir: &DirFd, depth: usize) -> Result<(), StoreError> {
    if depth == 0 {
        return Err(invalid("object tree too deep"));
    }
    for entry in dir.list(MAX_FILES)? {
        let name = FileName::new(
            entry
                .to_str()
                .ok_or_else(|| invalid("non UTF-8 object name"))?,
        )?;
        let stat = dir
            .entry(&name)?
            .ok_or_else(|| invalid("object vanished"))?;
        if stat.st_mode & libc::S_IFMT == libc::S_IFDIR {
            let child = dir
                .child(name.as_str(), Trust::Private, false)?
                .ok_or_else(|| invalid("object vanished"))?;
            sync_tree(&child, depth - 1)?;
        }
    }
    dir.sync()?;
    Ok(())
}
pub(crate) fn open(
    objects: &DirFd,
    digest: &str,
    cx: &ObserveContext,
) -> Result<Object, StoreError> {
    let name = validate_digest(digest)?;
    let dir = objects
        .child(name.as_str(), Trust::Private, false)?
        .ok_or_else(|| invalid("referenced object missing"))?;
    let mut files = BTreeMap::new();
    let mut left = MAX_BYTES;
    let mut entries_left = MAX_FILES * MAX_DEPTH;
    read_tree(
        &dir,
        "",
        &mut files,
        &mut left,
        &mut entries_left,
        MAX_DEPTH,
        cx,
    )?;
    let closure = Closure::new(files)?;
    if closure.digest != digest {
        return Err(invalid("object digest mismatch"));
    }
    Ok(Object {
        path: dir.path().into(),
        closure,
    })
}
fn read_tree(
    dir: &DirFd,
    prefix: &str,
    files: &mut BTreeMap<String, Vec<u8>>,
    left: &mut usize,
    entries_left: &mut usize,
    depth: usize,
    cx: &ObserveContext,
) -> Result<(), StoreError> {
    if depth == 0 {
        return Err(invalid("object tree too deep"));
    }
    let mut listed = 0usize;
    for entry in dir.list(MAX_FILES)? {
        cx.check("object read").map_err(StoreError::Interrupted)?;
        listed += 1;
        *entries_left = entries_left
            .checked_sub(1)
            .ok_or_else(|| invalid("too many object entries"))?;
        let name = FileName::new(
            entry
                .to_str()
                .ok_or_else(|| invalid("non UTF-8 object name"))?,
        )?;
        let path = format!("{prefix}{}", name.as_str());
        let stat = dir
            .entry(&name)?
            .ok_or_else(|| invalid("object vanished"))?;
        if stat.st_mode & libc::S_IFMT == libc::S_IFDIR {
            let child = dir
                .child(name.as_str(), Trust::Private, false)?
                .ok_or_else(|| invalid("object vanished"))?;
            read_tree(
                &child,
                &format!("{path}/"),
                files,
                left,
                entries_left,
                depth - 1,
                cx,
            )?;
        } else {
            if files.len() == MAX_FILES {
                return Err(invalid("too many object files"));
            }
            let (file, _) = dir
                .open_file(&name, libc::O_RDONLY, None, Trust::Private)?
                .ok_or_else(|| invalid("object vanished"))?;
            let bytes = fs::read_bounded(&file, *left as u64)
                .map_err(|e| FsError::io("read", dir.path(), &e))?
                .ok_or(StoreError::TooLarge {
                    limit: MAX_BYTES as u64,
                })?;
            *left -= bytes.len();
            files.insert(path, bytes);
        }
    }
    if listed == 0 {
        // A closure is a set of files, so no directory it produces is empty and
        // the digest alone covers every entry. An entry the digest cannot name
        // (an extra empty directory) is refused rather than silently trusted.
        return Err(invalid("object directory is empty"));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;

mod preflight;
