//! The append destination installed before a captured child is released.
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// A canonical host log. Child bytes can only enter through `ProcessCore`'s
/// sanitize/redact pipeline; this type deliberately exposes no raw write API.
/// A failed append poisons this destination, including after a partial write.
#[derive(Debug)]
pub struct MergedLog {
    file: File,
    failed: bool,
    committed: u64,
}

impl MergedLog {
    /// Open an existing follower path without replacing or truncating it.
    /// The caller owns the private parent directory and serializes writers.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::other("merged capture requires a regular file"));
        }
        Ok(Self {
            file,
            failed: false,
            committed: metadata.len(),
        })
    }

    /// Absolute plain-text offset after the last successful durable append.
    /// A failed write can leave an unacknowledged partial suffix on disk.
    pub fn committed_offset(&self) -> u64 {
        self.committed
    }

    pub(super) fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("merged capture destination failed"));
        }
        if bytes.is_empty() {
            return Ok(());
        }
        // Poison first: neither partial writes nor sync failures may be retried
        // as though they had written nothing, duplicating a follower's bytes.
        self.failed = true;
        crate::fault_point!(CaptureWrite)?;
        self.file.write_all(bytes)?;
        self.file.sync_data()?;
        self.committed = self.file.metadata()?.len();
        self.failed = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_preserves_existing_follower_offsets() {
        let home = effigy_test_support::short_tempdir("ml-").unwrap();
        let path = home.path().join("host.log");
        std::fs::write(&path, b"old\n").unwrap();
        let mut log = MergedLog::open(&path).unwrap();
        assert_eq!(log.committed_offset(), 4);
        log.append(b"next\n").unwrap();
        assert_eq!(log.committed_offset(), 9);
        assert_eq!(std::fs::read(&path).unwrap(), b"old\nnext\n");
        drop(log);
        let mut restarted = MergedLog::open(&path).unwrap();
        restarted.append(b"restart\n").unwrap();
        assert_eq!(restarted.committed_offset(), 17);
        assert_eq!(std::fs::read(&path).unwrap(), b"old\nnext\nrestart\n");
    }

    #[test]
    fn failed_append_does_not_acknowledge_and_poison_is_sticky() {
        let home = effigy_test_support::short_tempdir("ml-").unwrap();
        let path = home.path().join("host.log");
        std::fs::write(&path, b"old\n").unwrap();
        let mut log = MergedLog {
            file: File::open(&path).unwrap(), // a deterministic EBADF writer
            failed: false,
            committed: 4,
        };
        assert!(log.append(b"unwritten\n").is_err());
        assert_eq!(log.committed_offset(), 4);
        log.file = OpenOptions::new().append(true).open(&path).unwrap();
        assert!(log.append(b"must not retry\n").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"old\n");
    }

    #[test]
    fn rejects_symlinks_and_non_regular_destinations() {
        let home = effigy_test_support::short_tempdir("ml-").unwrap();
        let path = home.path().join("host.log");
        let link = home.path().join("link");
        std::fs::write(&path, b"old\n").unwrap();
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(MergedLog::open(&link).is_err());
        assert!(MergedLog::open(home.path()).is_err());
    }
}
