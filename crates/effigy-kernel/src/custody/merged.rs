//! The append destination installed before a captured child is released.
pub(crate) mod atomic;
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
    commit: migration::Commit,
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
        let log = Self::from_file(path, file)?;
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
        Self::from_file(path, migration::open(path)?)
    }

    fn from_file(path: &Path, file: File) -> io::Result<Self> {
        let commit = migration::Commit::open(path, &file)?;
        let committed = commit.extent();
        Ok(Self {
            file,
            failed: false,
            committed,
            commit,
            tap: None,
        })
    }

    #[cfg(test)]
    pub(super) fn prepare_test(path: &Path) -> io::Result<Self> {
        let mut log =
            Self::from_file(path, migration::prepare(path, [7; 32], "test", |_| Ok(()))?)?;
        log.tap = Some(migration::tap(path, Self::identity(path)?.identity)?);
        Ok(log)
    }

    /// Styles are a bounded optional projection; bytes must come from a
    /// verified canonical read before using this API.
    pub fn styles(path: &Path, read: &PlainRead) -> Vec<effigy_core::terminal_text::StyledSpan> {
        migration::styles(path, read)
    }

    /// Read-only canonical provenance and snapshot extent.
    /// Returns `WouldBlock` during writer commit; polling fronts retry rather
    /// than blocking their UI or bypassing the durable boundary.
    pub fn identity(path: &Path) -> io::Result<PlainLog> {
        migration::identity(path)
    }

    /// Read an identity-addressed plain slice. Unknown/replaced identities reset
    /// the byte offset with a typed cutover event; legacy bytes are refused.
    /// Returns `WouldBlock` during writer commit. One-shot callers may retry
    /// within their own deadline, without reading an unacknowledged suffix.
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
        let _lock = self.commit.lock()?;
        if self.file.metadata()?.len() != self.committed {
            return Err(io::Error::other(
                "canonical length changed outside its committed writer",
            ));
        }
        self.commit.begin()?;
        for part in bytes.chunks(4096) {
            crate::fault_point!(CaptureWrite)?;
            self.file.write_all(part)?;
        }
        crate::fault_point!(CaptureSync)?;
        self.file.sync_data()?;
        let next = self
            .committed
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| io::Error::other("canonical extent overflow"))?;
        if self.file.metadata()?.len() != next {
            return Err(io::Error::other("canonical length changed during append"));
        }
        let cleanup_durable = self.commit.publish(next)?;
        self.committed = next;
        self.failed = !cleanup_durable;
        Ok(())
    }
    pub(super) fn incomplete(&self) -> bool {
        self.failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_preserves_existing_follower_offsets() {
        let home = effigy_test_support::short_tempdir("ml-").unwrap();
        let path = home.path().join("host.log");
        effigy_test_support::write_private(&path, b"old\n").unwrap();
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
        effigy_test_support::write_private(&path, b"old\n").unwrap();
        let mut log = MergedLog::prepare_test(&path).unwrap();
        log.append(b"old\n").unwrap();
        log.file = File::open(&path).unwrap(); // a deterministic EBADF writer
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
        effigy_test_support::write_private(&path, b"old\n").unwrap();
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(MergedLog::open(&link).is_err());
        assert!(MergedLog::open(home.path()).is_err());
    }
    #[cfg(feature = "fault-injection")]
    #[test]
    fn readers_and_restarts_exclude_valid_and_torn_unacknowledged_suffixes() {
        use crate::fault::{self, Fault, Point};
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                fault::hook(None);
                fault::arm(None);
            }
        }
        for point in [
            Point::CaptureSync,
            Point::CaptureCommitAfterPublish,
            Point::CapturePendingRemoval,
        ] {
            let home = effigy_test_support::short_tempdir("mc-").unwrap();
            let path = home.path().join("app.log");
            let mut log = MergedLog::prepare_test(&path).unwrap();
            log.append(b"before\n").unwrap();
            let probe = path.clone();
            let seen = std::rc::Rc::new(std::cell::Cell::new(false));
            let observed = seen.clone();
            let _reset = Reset;
            fault::hook(Some(Box::new(move |at| {
                if at == Point::CaptureSync {
                    observed.set(true);
                    // This read is deliberately between write_all and failed
                    // sync. A concurrent writer may refuse the read; it may never
                    // expose the unacknowledged suffix.
                    if let Ok(read) = MergedLog::read_plain(&probe, None, 0, 4096) {
                        assert_eq!(read.bytes, b"before\n");
                    }
                }
            })));
            fault::arm_point(point, Fault::Enospc);
            assert!(log.append(b"valid but unacknowledged\n").is_err());
            assert!(seen.get() && fault::fired());
            fault::hook(None);
            fault::arm(None);
            assert_eq!(log.committed_offset(), 7);
            assert_eq!(
                MergedLog::read_plain(&path, None, 0, 4096).unwrap().bytes,
                b"before\n"
            );
            let retained = std::fs::read(&path).unwrap();
            drop(log);
            assert!(MergedLog::prepare_test(&path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), retained);
        }
        let home = effigy_test_support::short_tempdir("mt-").unwrap();
        let path = home.path().join("app.log");
        let mut log = MergedLog::prepare_test(&path).unwrap();
        let mut bytes = vec![b'x'; 4095];
        bytes.extend_from_slice("é tail".as_bytes());
        let _reset = Reset;
        fault::arm_nth(Point::CaptureWrite, 1, Fault::Enospc);
        assert!(log.append(&bytes).is_err());
        fault::arm(None);
        let torn = std::fs::read(&path).unwrap();
        assert_eq!(torn.len(), 4096);
        assert!(std::str::from_utf8(&torn).is_err());
        assert!(MergedLog::read_plain(&path, None, 0, 4096)
            .unwrap()
            .bytes
            .is_empty());
        drop(log);
        assert!(MergedLog::prepare_test(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), torn);
    }
    #[cfg(feature = "fault-injection")]
    #[test]
    fn durable_bytes_survive_uncertain_pending_cleanup_with_explicit_failure_state() {
        use crate::fault::{self, Fault, Point};
        let home = effigy_test_support::short_tempdir("mu-").unwrap();
        let path = home.path().join("app.log");
        let mut log = MergedLog::prepare_test(&path).unwrap();
        fault::arm_point(Point::CapturePendingSync, Fault::Enospc);
        let result = log.append(b"durable\n");
        let fired = fault::fired();
        fault::arm(None);
        assert!(result.is_ok() && fired && log.incomplete());
        assert_eq!(log.committed_offset(), 8);
        assert_eq!(
            MergedLog::read_plain(&path, None, 0, 4096).unwrap().bytes,
            b"durable\n"
        );
        assert!(log.append(b"late").is_err());
    }
}
