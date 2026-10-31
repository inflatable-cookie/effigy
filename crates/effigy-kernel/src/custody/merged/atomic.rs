//! Descriptor-relative exclusive moves and swaps for custody-owned records.
//! Unsupported filesystem operations fail closed; there is no ordinary rename
//! fallback that could destroy a raced operator file.
use crate::fs::{self, DirFd, FileName, Trust};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU64, Ordering};
static STAGING: AtomicU64 = AtomicU64::new(0);
fn rejected(reason: &str) -> io::Error {
    io::Error::other(reason)
}
use std::ffi::CString;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u64,
}
impl FileIdentity {
    // libc device/inode widths differ across supported Unix targets.
    #[allow(clippy::unnecessary_cast)]
    pub(crate) fn of(stat: &libc::stat) -> Self {
        Self {
            device: stat.st_dev as u64,
            inode: stat.st_ino as u64,
        }
    }
}

pub(crate) fn publish(
    dir: &DirFd,
    name: &FileName,
    value: &impl Serialize,
    nonce: &str,
    expected: Option<FileIdentity>,
) -> io::Result<FileIdentity> {
    // A killed stage is private and ignored; it is retained for operator
    // handling rather than removing a file whose creation was not observed.
    let stage = name
        .with_suffix(&format!(
            ".{nonce}.{}.{}",
            std::process::id(),
            STAGING.fetch_add(1, Ordering::Relaxed)
        ))
        .map_err(io::Error::other)?;
    let (mut file, _) = dir
        .open_file(
            &stage,
            libc::O_WRONLY | libc::O_EXCL,
            Some(0o600),
            Trust::Private,
        )
        .map_err(io::Error::other)?
        .ok_or_else(|| rejected("stage creation missing"))?;
    fs::fchmod(&file, 0o600)?;
    file.write_all(&serde_json::to_vec(value).map_err(io::Error::other)?)?;
    file.sync_all()?;
    let identity = FileIdentity::of(&fs::fstat(file.as_raw_fd())?);
    rename(
        dir,
        &stage,
        dir,
        name,
        if expected.is_some() {
            Rename::Exchange
        } else {
            Rename::Exclusive
        },
    )?;
    if let Some(expected) = expected {
        if dir
            .entry(&stage)
            .map_err(io::Error::other)?
            .map(|stat| FileIdentity::of(&stat))
            != Some(expected)
        {
            let private = dir
                .child(".custody-record-conflicts", Trust::Private, true)
                .map_err(io::Error::other)?
                .ok_or_else(|| rejected("private record quarantine missing"))?;
            private.sync().map_err(io::Error::other)?;
            dir.sync().map_err(io::Error::other)?;
            rename(dir, &stage, &private, &stage, Rename::Exclusive)?;
            private.sync().map_err(io::Error::other)?;
            dir.sync().map_err(io::Error::other)?;
            return Err(rejected(
                "record changed during exchange; displaced operator data retained privately",
            ));
        }
        dir.remove(&stage, false).map_err(io::Error::other)?;
    }
    if dir
        .entry(name)
        .map_err(io::Error::other)?
        .map(|stat| FileIdentity::of(&stat))
        != Some(identity)
    {
        return Err(rejected("published record changed identity"));
    }
    dir.sync().map_err(io::Error::other)?;
    Ok(identity)
}

/// Writer/reader serialization uses the existing nonblocking kernel flock
/// wrapper. Writers have a fixed admission bound; presentation readers retry.
#[derive(Debug)]
pub(crate) struct FileLock(File);
impl FileLock {
    pub(crate) fn shared(file: &File) -> io::Result<Self> {
        let file = file.try_clone()?;
        if fs::try_flock(&file, libc::LOCK_SH)? {
            Ok(Self(file))
        } else {
            Err(io::Error::from(io::ErrorKind::WouldBlock))
        }
    }
    pub(crate) fn exclusive(file: File) -> io::Result<Self> {
        // Own an independent open description, not a dup of the long-lived
        // writer. Even an unlock error releases this lock when its fd closes.
        let until = std::time::Instant::now() + std::time::Duration::from_millis(500);
        loop {
            if fs::try_flock(&file, libc::LOCK_EX)? {
                return Ok(Self(file));
            }
            if std::time::Instant::now() >= until {
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}
impl Drop for FileLock {
    fn drop(&mut self) {
        fs::unlock(&self.0);
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    version: u8,
    identity: FileIdentity,
    committed: u64,
}
fn pending_name(record: &FileName) -> io::Result<FileName> {
    record.with_suffix(".pending").map_err(io::Error::other)
}
pub(crate) fn begin_append(
    dir: &DirFd,
    record: &FileName,
    identity: FileIdentity,
    committed: u64,
) -> io::Result<()> {
    publish(
        dir,
        &pending_name(record)?,
        &Pending {
            version: 1,
            identity,
            committed,
        },
        "pending",
        None,
    )
    .map(|_| ())
}
pub(crate) fn pending_extent(
    dir: &DirFd,
    record: &FileName,
    identity: FileIdentity,
) -> io::Result<Option<u64>> {
    let Some((file, stat)) = dir
        .open_file(&pending_name(record)?, libc::O_RDONLY, None, Trust::Private)
        .map_err(io::Error::other)?
    else {
        return Ok(None);
    };
    if stat.st_size < 0 || stat.st_size > 256 {
        return Err(rejected("invalid pending append record"));
    }
    let pending: Pending = serde_json::from_reader(file.take(257)).map_err(io::Error::other)?;
    if pending.version != 1 || pending.identity != identity {
        return Err(rejected("pending append identity changed"));
    }
    Ok(Some(pending.committed))
}
pub(crate) fn finish_append(dir: &DirFd, record: &FileName) -> io::Result<bool> {
    crate::fault_point!(CapturePendingRemoval)?;
    dir.remove(&pending_name(record)?, false)
        .map_err(io::Error::other)?;
    // The data and extent are already durable. A failure syncing removal may
    // resurrect only this conservative recovery barrier; the committed bytes
    // are acknowledged, capture stops, and restart may require operator repair.
    Ok(crate::fault_point!(CapturePendingSync)
        .and_then(|()| dir.sync().map_err(io::Error::other))
        .is_ok())
}

pub(crate) enum Rename {
    Exclusive,
    Exchange,
}

pub(crate) fn rename(
    source: &DirFd,
    from: &FileName,
    target: &DirFd,
    to: &FileName,
    operation: Rename,
) -> io::Result<()> {
    let from = CString::new(from.as_str()).map_err(io::Error::other)?;
    let to = CString::new(to.as_str()).map_err(io::Error::other)?;
    #[cfg(target_os = "macos")]
    let flags = match operation {
        Rename::Exclusive => libc::RENAME_EXCL,
        Rename::Exchange => libc::RENAME_SWAP,
    };
    #[cfg(target_os = "linux")]
    let flags = match operation {
        Rename::Exclusive => libc::RENAME_NOREPLACE,
        Rename::Exchange => libc::RENAME_EXCHANGE,
    };
    // SAFETY: both DirFd references retain live directory descriptors for the
    // call. Each CString owns one initialized, NUL-terminated allocation with
    // no interior NUL, made from a validated relative single FileName. Its
    // pointers stay readable and unmodified through the synchronous syscall.
    // Only the selected documented, mutually exclusive flag is passed. No Rust
    // reference aliases memory the syscall mutates; it changes directory entries.
    // Contracts: Apple xnu-11417.140.69 bsd/man/man2/rename.2; Linux man-pages
    // rename(2), RENAME_NOREPLACE/RENAME_EXCHANGE. Neither follows final symlinks.
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            source.raw(),
            from.as_ptr(),
            target.raw(),
            to.as_ptr(),
            flags,
        )
    };
    #[cfg(target_os = "linux")]
    // SAFETY: source/target retain live directory fds, both CStrings own their
    // initialized NUL-terminated, unmodified allocations through this call,
    // and flags is exactly one supported, mutually exclusive renameat2 flag.
    // Names are relative components; no Rust memory is mutated or aliased.
    let result = unsafe {
        libc::renameat2(
            source.raw(),
            from.as_ptr(),
            target.raw(),
            to.as_ptr(),
            flags,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exclusive_move_never_overwrites_a_raced_destination() {
        let home = effigy_test_support::short_tempdir("ar-").unwrap();
        let dir = DirFd::open(home.path()).unwrap();
        let from = FileName::new("source").unwrap();
        let to = FileName::new("operator").unwrap();
        std::fs::write(home.path().join("source"), b"source").unwrap();
        std::fs::write(home.path().join("operator"), b"operator data").unwrap();
        assert_eq!(
            rename(&dir, &from, &dir, &to, Rename::Exclusive)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            std::fs::read(home.path().join("operator")).unwrap(),
            b"operator data"
        );
        assert_eq!(
            std::fs::read(home.path().join("source")).unwrap(),
            b"source"
        );
    }
    #[test]
    fn exchanged_foreign_record_is_retained_privately_without_clobbering() {
        let home = effigy_test_support::short_tempdir("ax-").unwrap();
        let dir = DirFd::open(home.path()).unwrap();
        let name = FileName::new("record").unwrap();
        let expected = publish(&dir, &name, &serde_json::json!({"ours":1}), "first", None).unwrap();
        std::fs::rename(
            home.path().join("record"),
            home.path().join("held-original"),
        )
        .unwrap();
        std::fs::write(home.path().join("record"), b"foreign operator data").unwrap();
        assert!(publish(
            &dir,
            &name,
            &serde_json::json!({"ours":2}),
            "raced",
            Some(expected)
        )
        .is_err());
        let private = home.path().join(".custody-record-conflicts");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&private).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let saved = std::fs::read_dir(private).unwrap().next().unwrap().unwrap();
        assert_eq!(
            std::fs::read(saved.path()).unwrap(),
            b"foreign operator data"
        );
        assert!(std::fs::read(home.path().join("held-original"))
            .unwrap()
            .starts_with(b"{\"ours\":1}"));
    }
}
