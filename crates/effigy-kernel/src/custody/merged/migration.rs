//! The canonical host log's admission. The caller holds the admitted
//! host-process gate, so no predecessor can still own this path. A log without
//! this writer's provenance is never read, renamed or overwritten: the writer
//! refuses and leaves it for its operator to move aside once.
use crate::fs::{self, DirFd, FileName, Trust};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::path::Path;

use super::atomic::publish;
pub use super::atomic::FileIdentity;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    version: u8,
    owner: [u8; 32],
    canonical: FileIdentity,
    committed: u64,
}
fn refused(reason: &str) -> io::Error {
    io::Error::other(format!("host log admission refused: {reason}; retain the named data and repair the ownership/identity condition before restarting"))
}
fn io(error: impl std::fmt::Display) -> io::Error {
    std::io::Error::other(error.to_string())
}
fn names(path: &Path) -> io::Result<(DirFd, FileName, FileName, FileName)> {
    let parent = path.parent().ok_or_else(|| refused("log parent missing"))?;
    let name = FileName::new(
        path.file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| refused("invalid log name"))?,
    )
    .map_err(io)?;
    let hash = crate::id::Digest::of(name.as_str().as_bytes())
        .0
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok((
        DirFd::walk(parent, Trust::Operator).map_err(io)?,
        name,
        FileName::new(&format!(".{hash}.host-log-stage")).map_err(io)?,
        FileName::new(&format!(".{hash}.host-log-ready")).map_err(io)?,
    ))
}
fn record<T: serde::de::DeserializeOwned>(dir: &DirFd, name: &FileName) -> io::Result<Option<T>> {
    Ok(record_identity(dir, name)?.map(|(value, _)| value))
}
fn record_identity<T: serde::de::DeserializeOwned>(
    dir: &DirFd,
    name: &FileName,
) -> io::Result<Option<(T, FileIdentity)>> {
    let Some((file, stat)) = dir
        .open_file(name, libc::O_RDONLY, None, Trust::Private)
        .map_err(io)?
    else {
        return Ok(None);
    };
    if stat.st_size < 0 || stat.st_size > 4096 {
        return Err(refused("unbounded provenance record"));
    }
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    serde_json::from_slice(&bytes)
        .map(|value| Some((value, FileIdentity::of(&stat))))
        .map_err(|_| refused("invalid provenance record"))
}
fn opened(dir: &DirFd, name: &FileName, trust: Trust) -> io::Result<Option<(File, libc::stat)>> {
    let result = dir
        .open_file(name, libc::O_RDWR | libc::O_APPEND, None, trust)
        .map_err(io)?;
    if result.as_ref().is_some_and(|(_, stat)| stat.st_nlink != 1) {
        return Err(refused("linked log"));
    }
    Ok(result)
}
fn trusted(dir: &DirFd, name: &FileName, proof: &Provenance) -> io::Result<File> {
    let (file, stat) =
        opened(dir, name, Trust::Private)?.ok_or_else(|| refused("canonical log missing"))?;
    if proof.version != 2 || FileIdentity::of(&stat) != proof.canonical {
        return Err(refused("canonical log identity changed"));
    }
    Ok(file)
}

#[cfg(test)]
pub(super) fn open(path: &Path) -> io::Result<File> {
    let (dir, name, _, ready) = names(path)?;
    let proof = record::<Provenance>(&dir, &ready)?.ok_or_else(|| {
        refused("no durable canonical provenance; restart through the host-process owner")
    })?;
    trusted(&dir, &name, &proof)
}

/// Admit this writer's canonical log, or refuse and leave any other content
/// untouched. `nonce` names the ready publication's staging file.
pub(super) fn prepare(path: &Path, owner: [u8; 32], nonce: &str) -> io::Result<File> {
    let (dir, name, stage, ready) = names(path)?;
    if let Some(proof) = record::<Provenance>(&dir, &ready)? {
        if proof.owner != owner {
            return Err(refused("canonical provenance belongs to another owner"));
        }
        // A crash between its ready record and its rename leaves the bound
        // stage private; publishing it is the only recovery this needs.
        if dir.entry(&name).map_err(io)?.is_none() {
            if let Some((_, stat)) = opened(&dir, &stage, Trust::Private)? {
                if FileIdentity::of(&stat) == proof.canonical && stat.st_size == 0 {
                    super::atomic::rename(
                        &dir,
                        &stage,
                        &dir,
                        &name,
                        super::atomic::Rename::Exclusive,
                    )?;
                    dir.sync().map_err(io)?;
                }
            }
        }
        let file = trusted(&dir, &name, &proof)?;
        let _lock = super::atomic::FileLock::exclusive(trusted(&dir, &name, &proof)?)?;
        if super::atomic::pending_extent(&dir, &ready, proof.canonical)?.is_some() {
            return Err(refused("a failed or interrupted append remains pending; preserve the log and pending record for operator recovery"));
        }
        if file.metadata()?.len() != proof.committed {
            return Err(refused("canonical log has a torn or unacknowledged suffix; producer refused, preserve the file for operator recovery"));
        }
        file.sync_all()?;
        dir.sync().map_err(io)?;
        return Ok(file);
    }
    // No provenance: whatever holds this name came from another writer. It is
    // never served as canonical and never removed automatically.
    if dir.entry(&name).map_err(io)?.is_some() {
        return Err(refused(
            "the log at this path was written by an older Effigy; move it aside once and restart, because untrusted old content is never served",
        ));
    }
    let (file, stat) = dir
        .open_file(
            &stage,
            libc::O_RDWR | libc::O_APPEND | libc::O_EXCL,
            Some(0o600),
            Trust::Private,
        )
        .map_err(io)?
        .ok_or_else(|| refused("canonical stage creation missing"))?;
    fs::fchmod(&file, 0o600)?;
    file.sync_all()?;
    let canonical = FileIdentity::of(&stat);
    publish(
        &dir,
        &ready,
        &Provenance {
            version: 2,
            owner,
            canonical,
            committed: 0,
        },
        nonce,
        None,
    )?;
    super::atomic::rename(&dir, &stage, &dir, &name, super::atomic::Rename::Exclusive)?;
    if dir
        .entry(&name)
        .map_err(io)?
        .map(|stat| FileIdentity::of(&stat))
        != Some(canonical)
    {
        dir.sync().map_err(io)?;
        return Err(refused(
            "published canonical inode changed; no ready provenance granted",
        ));
    }
    dir.sync().map_err(io)?;
    Ok(file)
}

/// Durable extent publication. This is writer commit metadata, never terminal
/// parser state or replay/checkpoint machinery. Readers must clamp to it.
#[derive(Debug)]
pub(super) struct Commit {
    dir: DirFd,
    ready: FileName,
    name: FileName,
    proof: Provenance,
    record_identity: FileIdentity,
}
impl Commit {
    pub(super) fn lock(&self) -> io::Result<super::atomic::FileLock> {
        super::atomic::FileLock::exclusive(trusted(&self.dir, &self.name, &self.proof)?)
    }
    pub(super) fn open(path: &Path, file: &File) -> io::Result<Self> {
        let (dir, name, _, ready) = names(path)?;
        let (proof, record_identity) = record_identity::<Provenance>(&dir, &ready)?
            .ok_or_else(|| refused("ready provenance missing"))?;
        if proof.version != 2
            || FileIdentity::of(&fs::fstat(file.as_raw_fd())?) != proof.canonical
            || file.metadata()?.len() != proof.committed
        {
            return Err(refused(
                "canonical extent or identity changed; producer refused",
            ));
        }
        Ok(Self {
            dir,
            ready,
            name,
            proof,
            record_identity,
        })
    }
    pub(super) fn extent(&self) -> u64 {
        self.proof.committed
    }
    pub(super) fn begin(&self) -> io::Result<()> {
        super::atomic::begin_append(
            &self.dir,
            &self.ready,
            self.proof.canonical,
            self.proof.committed,
        )
    }
    pub(super) fn publish(&mut self, extent: u64) -> io::Result<bool> {
        if self
            .dir
            .entry(&self.name)
            .map_err(io)?
            .map(|stat| FileIdentity::of(&stat))
            != Some(self.proof.canonical)
        {
            return Err(refused("canonical inode changed before commit"));
        }
        let mut proof = self.proof.clone();
        proof.committed = extent;
        self.record_identity = publish(
            &self.dir,
            &self.ready,
            &proof,
            "commit",
            Some(self.record_identity),
        )?;
        self.proof = proof;
        crate::fault_point!(CaptureCommitAfterPublish)?;
        super::atomic::finish_append(&self.dir, &self.ready)
    }
}

/// Identity and snapshot extent of a canonical file, without exposing a writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlainLog {
    pub identity: FileIdentity,
    pub length: u64,
}
/// A bounded read addressed in canonical file bytes. Cutover is metadata, never
/// a replacement marker inserted into child text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainRead {
    pub log: PlainLog,
    pub from: u64,
    pub bytes: Vec<u8>,
    pub cutover: bool,
}
fn readable(path: &Path) -> io::Result<(DirFd, FileName, File, PlainLog)> {
    let (dir, name, _, ready) = names(path)?;
    let (file, stat) = dir
        .open_file(&name, libc::O_RDONLY, None, Trust::Private)
        .map_err(io)?
        .ok_or_else(|| refused("canonical log missing"))?;
    let _lock = super::atomic::FileLock::shared(&file)?;
    let proof = record::<Provenance>(&dir, &ready)?.ok_or_else(|| {
        refused("log has no canonical provenance yet; reopen after its host owner starts")
    })?;
    if proof.version != 2
        || proof.canonical != FileIdentity::of(&stat)
        || stat.st_nlink != 1
        || stat.st_size < 0
        || (stat.st_size as u64) < proof.committed
    {
        return Err(refused("canonical read identity changed"));
    }
    let committed = super::atomic::pending_extent(&dir, &ready, proof.canonical)?
        .map_or(proof.committed, |before| before.min(proof.committed));
    let log = PlainLog {
        identity: proof.canonical,
        length: committed,
    };
    Ok((dir, name, file, log))
}
pub(super) fn identity(path: &Path) -> io::Result<PlainLog> {
    readable(path).map(|(_, _, _, log)| log)
}
pub(super) fn read(
    path: &Path,
    expected: Option<FileIdentity>,
    offset: u64,
    max: usize,
) -> io::Result<PlainRead> {
    use std::io::{Seek, SeekFrom};
    let (dir, name, mut file, log) = readable(path)?;
    let cutover = expected.is_some_and(|id| id != log.identity)
        || (expected.is_none() && offset != 0)
        || offset > log.length;
    let from = if cutover { 0 } else { offset };
    file.seek(SeekFrom::Start(from))?;
    let mut bytes = Vec::new();
    file.take((log.length - from).min(max.min(64 * 1024) as u64))
        .read_to_end(&mut bytes)?;
    // Hold incomplete scalars instead of lossily constructing a secret. This
    // validates framing; it never strips or interprets terminal controls.
    match std::str::from_utf8(&bytes) {
        Ok(text) if text.chars().any(|ch| ch.is_control() && ch != '\n') => {
            return Err(refused("canonical log contains controls"))
        }
        Ok(_) => {}
        Err(error) if error.error_len().is_none() => {
            bytes.truncate(error.valid_up_to());
            if std::str::from_utf8(&bytes)
                .map_err(io)?
                .chars()
                .any(|ch| ch.is_control() && ch != '\n')
            {
                return Err(refused("canonical log contains controls"));
            }
        }
        Err(_) => {
            return Err(refused(
                "canonical log contains invalid text; no bytes were exposed",
            ))
        }
    }
    if dir
        .entry(&name)
        .map_err(io)?
        .map(|stat| FileIdentity::of(&stat))
        != Some(log.identity)
    {
        return Err(refused(
            "log was replaced during read; reopen at offset zero",
        ));
    }
    Ok(PlainRead {
        log,
        from,
        bytes,
        cutover,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    fn write_committed(path: &Path, file: &mut File, bytes: &[u8]) {
        let mut commit = Commit::open(path, file).unwrap();
        let _lock = commit.lock().unwrap();
        let next = commit.extent() + bytes.len() as u64;
        commit.begin().unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
        assert!(commit.publish(next).unwrap());
    }
    #[test]
    fn a_log_without_this_writers_provenance_is_refused_and_kept() {
        let home = effigy_test_support::short_tempdir("lm-").unwrap();
        let path = home.path().join("app.log");
        let raw = b"\x1b[31mlegacy-secret-457\x1b[0m\n";
        effigy_test_support::write_private(&path, raw).unwrap();
        let old = std::fs::metadata(&path).unwrap();
        let error = prepare(&path, [3; 32], "legacy").unwrap_err().to_string();
        assert!(error.contains("move it aside"), "{error}");
        // The bytes, their inode and their mode are untouched, and no reader
        // can serve them as canonical.
        let now = std::fs::metadata(&path).unwrap();
        assert_eq!((now.ino(), now.mode() & 0o777), (old.ino(), 0o600));
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        assert!(open(&path).is_err() && identity(&path).is_err());
        // Once the operator moves it aside the same path admits a canonical
        // log, and a second start reopens that one.
        std::fs::rename(&path, home.path().join("legacy-app.log")).unwrap();
        let mut canonical = prepare(&path, [3; 32], "fresh").unwrap();
        write_committed(&path, &mut canonical, b"safe\n");
        assert_eq!(std::fs::read(&path).unwrap(), b"safe\n");
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        let again = prepare(&path, [3; 32], "restart").unwrap();
        assert_eq!(
            again.metadata().unwrap().ino(),
            canonical.metadata().unwrap().ino()
        );
    }
    #[test]
    fn a_private_stage_left_by_a_crash_is_published_by_the_next_start() {
        let home = effigy_test_support::short_tempdir("ls-").unwrap();
        let path = home.path().join("app.log");
        // Simulate the crash by writing the ready record and stage by hand,
        // exactly as `prepare` does before its rename.
        let (dir, name, stage, ready) = names(&path).unwrap();
        let (file, stat) = dir
            .open_file(
                &stage,
                libc::O_RDWR | libc::O_APPEND | libc::O_EXCL,
                Some(0o600),
                Trust::Private,
            )
            .unwrap()
            .unwrap();
        file.sync_all().unwrap();
        let canonical = FileIdentity::of(&stat);
        publish(
            &dir,
            &ready,
            &Provenance {
                version: 2,
                owner: [3; 32],
                canonical,
                committed: 0,
            },
            "crash",
            None,
        )
        .unwrap();
        assert!(dir.entry(&name).unwrap().is_none());
        drop(file);
        let mut recovered = prepare(&path, [3; 32], "restart").unwrap();
        let recovered_stat = crate::fs::fstat(recovered.as_raw_fd()).unwrap();
        assert_eq!(FileIdentity::of(&recovered_stat), canonical);
        write_committed(&path, &mut recovered, b"safe\n");
        assert_eq!(std::fs::read(&path).unwrap(), b"safe\n");
    }
    #[test]
    fn direct_reader_resets_on_new_inode_even_when_new_file_is_larger() {
        let home = effigy_test_support::short_tempdir("lr-").unwrap();
        let path = home.path().join("app.log");
        effigy_test_support::write_private(&path, b"\x1b[31mlegacy-secret\n").unwrap();
        let old = std::fs::metadata(&path).unwrap();
        let old_identity = FileIdentity {
            device: old.dev(),
            inode: old.ino(),
        };
        assert!(read(&path, Some(old_identity), old.len(), 64 * 1024).is_err());
        std::fs::rename(&path, home.path().join("moved-aside")).unwrap();
        let mut writer = prepare(&path, [3; 32], "cutover").unwrap();
        let safe = "safe output\n".repeat(100);
        write_committed(&path, &mut writer, safe.as_bytes());
        let result = read(&path, Some(old_identity), old.len(), 64 * 1024).unwrap();
        assert!(result.cutover);
        assert_eq!(result.from, 0);
        assert_eq!(result.bytes, safe.as_bytes());
        let next = read(
            &path,
            Some(result.log.identity),
            result.bytes.len() as u64,
            64 * 1024,
        )
        .unwrap();
        assert!(!next.cutover);
        assert!(next.bytes.is_empty());
    }

    #[test]
    fn cutover_refuses_symlinks_directories_and_linked_operator_data() {
        for kind in 0..3 {
            let home = effigy_test_support::short_tempdir("lr-").unwrap();
            let path = home.path().join("app.log");
            let other = home.path().join("operator-data");
            effigy_test_support::write_private(&other, b"retain all operator bytes").unwrap();
            match kind {
                0 => std::os::unix::fs::symlink(&other, &path).unwrap(),
                1 => std::fs::create_dir(&path).unwrap(),
                _ => std::fs::hard_link(&other, &path).unwrap(),
            }
            assert!(prepare(&path, [3; 32], "refusal").is_err());
            assert!(identity(&path).is_err());
            assert_eq!(std::fs::read(&other).unwrap(), b"retain all operator bytes");
        }
    }
    #[test]
    fn replacement_and_foreign_owner_are_left_untouched() {
        let home = effigy_test_support::short_tempdir("lm-").unwrap();
        let path = home.path().join("app.log");
        let file = prepare(&path, [3; 32], "first").unwrap();
        drop(file);
        assert!(prepare(&path, [4; 32], "other").is_err());
        let replacement = home.path().join("replacement");
        effigy_test_support::write_private(&replacement, b"operator data").unwrap();
        std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::rename(replacement, &path).unwrap();
        assert!(prepare(&path, [3; 32], "retry").is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"operator data");
    }
    // Overrides the expected owner through `crate::fault`, which exists only with
    // `fault-injection`; ordinary builds keep the rest of the migration tests.
    #[cfg(feature = "fault-injection")]
    #[test]
    fn unowned_log_is_refused_without_changing_operator_data() {
        struct RestoreUid;
        impl Drop for RestoreUid {
            fn drop(&mut self) {
                crate::fault::set_operator_uid(None);
            }
        }
        let home = effigy_test_support::short_tempdir("lu-").unwrap();
        let path = home.path().join("app.log");
        effigy_test_support::write_private(&path, b"operator data").unwrap();
        let (dir, name, _, _) = names(&path).unwrap();
        let owner = std::fs::metadata(&path).unwrap().uid();
        let _restore = RestoreUid;
        // The existing strict fake changes the expected owner only on this
        // thread. No host UID, real file owner or other test is changed.
        crate::fault::set_operator_uid(Some(owner.wrapping_add(1)));
        assert!(opened(&dir, &name, Trust::Operator)
            .unwrap_err()
            .to_string()
            .contains("wrong owner"));
        assert!(prepare(&path, [3; 32], "unowned").is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"operator data");
    }
}
