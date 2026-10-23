//! Bounded descriptor-relative import copies. No link to a source is retained.
use super::*;
use crate::ctx::EffectContext;
use crate::fs::{self, DirFd, FsError};
use sha2::{Digest as _, Sha256};
use std::io::{Read, Write};

const TREE_LIMIT: u64 = 4 << 30;
const DEPTH_LIMIT: usize = 32;

pub(super) fn digest(
    cx: &ObserveContext,
    dir: &DirFd,
    name: &FileName,
) -> Result<String, StoreError> {
    let (mut file, stat) = dir
        .open_file(name, libc::O_RDONLY, None, Trust::Private)?
        .ok_or_else(|| StoreError::Corrupt("migration source file disappeared".into()))?;
    if stat.st_size as u64 > crate::store::MAX_DB_BYTES {
        return Err(StoreError::TooLarge {
            limit: crate::store::MAX_DB_BYTES,
        });
    }
    let mut hash = Sha256::new();
    let mut total = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        cx.check("migration source fingerprint")
            .map_err(StoreError::Interrupted)?;
        let size = file
            .read(&mut buffer)
            .map_err(|error| FsError::io("migration read", dir.path(), &error))?;
        if size == 0 {
            break;
        }
        total += size as u64;
        if total > crate::store::MAX_DB_BYTES {
            return Err(StoreError::TooLarge {
                limit: crate::store::MAX_DB_BYTES,
            });
        }
        hash.update(&buffer[..size]);
    }
    if total != stat.st_size as u64 {
        return Err(StoreError::Corrupt(
            "migration source changed during fingerprint".into(),
        ));
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub(super) fn tree(
    cx: &EffectContext,
    source: &DirFd,
    destination: &DirFd,
) -> Result<(), StoreError> {
    let mut bytes_left = TREE_LIMIT;
    let mut entries_left = ENTRY_LIMIT;
    copy_tree(
        cx,
        source,
        destination,
        &mut bytes_left,
        &mut entries_left,
        DEPTH_LIMIT,
    )
}
fn copy_tree(
    cx: &EffectContext,
    source: &DirFd,
    destination: &DirFd,
    bytes_left: &mut u64,
    entries_left: &mut usize,
    depth: usize,
) -> Result<(), StoreError> {
    if depth == 0 {
        return Err(StoreError::Corrupt(
            "migration file tree depth exceeded".into(),
        ));
    }
    for entry in source.list(ENTRY_LIMIT)? {
        cx.operation()
            .check("migration file copy")
            .map_err(StoreError::Interrupted)?;
        *entries_left = entries_left.checked_sub(1).ok_or(StoreError::TooLarge {
            limit: ENTRY_LIMIT as u64,
        })?;
        let name = FileName::new(
            entry
                .to_str()
                .ok_or_else(|| StoreError::Corrupt("non-UTF8 migration file name".into()))?,
        )?;
        let stat = source
            .entry(&name)?
            .ok_or_else(|| StoreError::Corrupt("migration file vanished".into()))?;
        if stat.st_mode & libc::S_IFMT == libc::S_IFDIR {
            let source = source
                .child(name.as_str(), Trust::Private, false)?
                .ok_or_else(|| StoreError::Corrupt("migration directory vanished".into()))?;
            let target = destination
                .child(name.as_str(), Trust::Private, true)?
                .expect("created import directory");
            copy_tree(cx, &source, &target, bytes_left, entries_left, depth - 1)?;
            // Preserve private immutable object modes and executable staging
            // directories, after all children were written and synced.
            target.chmod(private_mode(stat.st_mode))?;
            target.sync()?;
        } else {
            let (mut input, stat) = source
                .open_file(&name, libc::O_RDONLY, None, Trust::Private)?
                .ok_or_else(|| StoreError::Corrupt("migration file vanished".into()))?;
            if stat.st_size as u64 > *bytes_left {
                return Err(StoreError::TooLarge { limit: TREE_LIMIT });
            }
            let (mut output, _) = destination
                .open_file(
                    &name,
                    libc::O_WRONLY | libc::O_EXCL,
                    Some(0o600),
                    Trust::Private,
                )?
                .expect("created import file");
            let mut copied = 0u64;
            let mut buffer = [0; 64 * 1024];
            loop {
                cx.operation()
                    .check("migration file bytes")
                    .map_err(StoreError::Interrupted)?;
                let size = input
                    .read(&mut buffer)
                    .map_err(|error| FsError::io("migration read", source.path(), &error))?;
                if size == 0 {
                    break;
                }
                copied += size as u64;
                if copied > stat.st_size as u64 {
                    return Err(StoreError::Corrupt(
                        "migration file grew while copied".into(),
                    ));
                }
                output
                    .write_all(&buffer[..size])
                    .map_err(|error| FsError::io("migration write", destination.path(), &error))?;
            }
            if copied != stat.st_size as u64 {
                return Err(StoreError::Corrupt(
                    "migration file changed while copied".into(),
                ));
            }
            *bytes_left -= copied;
            fs::fchmod(&output, private_mode(stat.st_mode))
                .map_err(|error| FsError::io("migration chmod", destination.path(), &error))?;
            fs::fsync(&output)
                .map_err(|error| FsError::io("migration fsync", destination.path(), &error))?;
        }
    }
    destination.sync()?;
    Ok(())
}

/// Darwin uses a narrower native mode_t than Linux. Preserve only the
/// source owner's permissions while removing group/other access in staging.
fn private_mode(mode: libc::mode_t) -> u32 {
    #[cfg(target_os = "macos")]
    let mode = u32::from(mode);
    mode & 0o700
}
