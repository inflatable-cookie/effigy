//! The append destination installed before a captured child is released.
mod migration;
pub use migration::{FileIdentity, PlainLog, PlainRead};
use std::fs::File;
#[cfg(test)]
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

/// A canonical host log. Child bytes can only enter through `ProcessCore`'s
/// sanitize/redact pipeline; this type deliberately exposes no raw write API.
/// A failed append poisons this destination, including after a partial write.
#[derive(Debug)]
pub struct MergedLog {
    file: File,
    failed: bool,
    committed: u64,
    tap: Option<super::styled_tap::Tap>,
}

impl MergedLog {
    /// Prepare the one-time legacy cutover under a newly admitted host owner.
    /// Run admission proves predecessor settlement and retains the owner gate.
    pub fn prepare(run: &mut crate::run::Run<'_>, path: &Path) -> Result<Self, super::SpawnError> {
        run.verify_custody()?;
        if run.fence().resource.kind() != crate::id::ResourceKind::HostProcess
            || run.phase()? != crate::run::Phase::Admitted
        {
            return Err(super::SpawnError::Refused("host log cutover needs a newly admitted host-process owner with all predecessors settled".into()));
        }
        let owner = crate::id::Digest::of(
            &serde_json::to_vec(&run.fence().resource).map_err(io::Error::other)?,
        )
        .0;
        let ticket = run.intend(crate::run::EffectSpec {
            target: run.fence().clone(),
            kind: "host-log-cutover".into(),
            payload: serde_json::json!({"version":1,"path_digest":crate::id::Digest::of(path.as_os_str().as_encoded_bytes())}),
            settlement: crate::run::Settlement::Retain,
            required: false,
        })?;
        let file = migration::prepare(path, owner, &run.id().to_string(), |_| Ok(()))?;
        let log = Self::from_file(file)?;
        let identity = Self::identity(path)?;
        run.applied(
            &ticket,
            serde_json::json!({"version":1,"canonical":identity.identity}),
        )?;
        let mut log = log;
        log.tap = Some(migration::tap(path, identity.identity)?);
        Ok(log)
    }

    /// Open only a previously published canonical identity, never a legacy file.
    #[cfg(test)]
    fn open(path: &Path) -> io::Result<Self> {
        Self::from_file(migration::open(path)?)
    }

    fn from_file(file: File) -> io::Result<Self> {
        let committed = file.metadata()?.len();
        Ok(Self {
            file,
            failed: false,
            committed,
            tap: None,
        })
    }

    #[cfg(test)]
    pub(super) fn prepare_test(path: &Path) -> io::Result<Self> {
        let mut log = Self::from_file(migration::prepare(path, [7; 32], "test", |_| Ok(()))?)?;
        log.tap = Some(migration::tap(path, Self::identity(path)?.identity)?);
        Ok(log)
    }

    /// Styles are a bounded optional projection; bytes must come from a
    /// verified canonical read before using this API.
    pub fn styles(path: &Path, read: &PlainRead) -> Vec<effigy_core::terminal_text::StyledSpan> {
        migration::styles(path, read)
    }

    /// Read-only canonical provenance and snapshot extent.
    pub fn identity(path: &Path) -> io::Result<PlainLog> {
        migration::identity(path)
    }

    /// Read an identity-addressed plain slice. Unknown/replaced identities reset
    /// the byte offset with a typed cutover event; legacy bytes are refused.
    pub fn read_plain(
        path: &Path,
        expected: Option<FileIdentity>,
        offset: u64,
        max: usize,
    ) -> io::Result<PlainRead> {
        migration::read(path, expected, offset, max)
    }

    /// Absolute plain-text offset after the last successful durable append.
    /// A failed write can leave an unacknowledged partial suffix on disk.
    pub fn committed_offset(&self) -> u64 {
        self.committed
    }

    pub(super) fn publish_styles(
        &mut self,
        chunk: effigy_core::terminal_text::StyledChunk,
    ) -> io::Result<()> {
        self.tap.as_mut().map_or(Ok(()), |tap| tap.append(chunk))
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
        for part in bytes.chunks(4096) {
            crate::fault_point!(CaptureWrite)?;
            self.file.write_all(part)?;
        }
        crate::fault_point!(CaptureSync)?;
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
        let mut log = MergedLog::prepare_test(&path).unwrap();
        assert_eq!(log.committed_offset(), 0);
        log.append(b"next\n").unwrap();
        assert_eq!(log.committed_offset(), 5);
        assert_eq!(std::fs::read(&path).unwrap(), b"next\n");
        drop(log);
        let mut restarted = MergedLog::open(&path).unwrap();
        restarted.append(b"restart\n").unwrap();
        assert_eq!(restarted.committed_offset(), 13);
        assert_eq!(std::fs::read(&path).unwrap(), b"next\nrestart\n");
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
            tap: None,
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
