//! One-time legacy cutover. The caller holds the admitted host-process gate;
//! no predecessor can still own this path. No legacy payload is read here.
use crate::fs::{self, DirFd, FileName, Trust};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static STAGING: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u64,
}
impl FileIdentity {
    // libc device/inode widths differ across supported Unix targets.
    #[allow(clippy::unnecessary_cast)]
    fn of(stat: &libc::stat) -> Self {
        Self {
            device: stat.st_dev as u64,
            inode: stat.st_ino as u64,
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    version: u8,
    owner: [u8; 32],
    legacy: Option<FileIdentity>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    version: u8,
    owner: [u8; 32],
    canonical: FileIdentity,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Boundary {
    BeforeRename,
    AfterRename,
    AfterCreate,
    BeforeReady,
    AfterReady,
}

fn refused(reason: &str) -> io::Error {
    io::Error::other(format!("host log cutover refused: {reason}; retain the legacy data and repair the named ownership/identity condition before restarting"))
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
        FileName::new(&format!(".{hash}.host-log-intent")).map_err(io)?,
        FileName::new(&format!(".{hash}.host-log-ready")).map_err(io)?,
    ))
}
fn record<T: serde::de::DeserializeOwned>(dir: &DirFd, name: &FileName) -> io::Result<Option<T>> {
    let Some((file, stat)) = dir
        .open_file(name, libc::O_RDONLY, None, Trust::Private)
        .map_err(io)?
    else {
        return Ok(None);
    };
    if stat.st_size > 4096 {
        return Err(refused("unbounded provenance record"));
    }
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| refused("invalid provenance record"))
}
fn publish(dir: &DirFd, name: &FileName, value: &impl Serialize, nonce: &str) -> io::Result<()> {
    // A killed stage is private and ignored; it is retained for operator
    // handling rather than removing a file whose creation was not observed.
    let stage = name
        .with_suffix(&format!(
            ".{nonce}.{}",
            STAGING.fetch_add(1, Ordering::Relaxed)
        ))
        .map_err(io)?;
    let (mut file, _) = dir
        .open_file(
            &stage,
            libc::O_WRONLY | libc::O_EXCL,
            Some(0o600),
            Trust::Private,
        )
        .map_err(io)?
        .ok_or_else(|| refused("stage creation missing"))?;
    fs::fchmod(&file, 0o600)?;
    file.write_all(&serde_json::to_vec(value).map_err(io)?)?;
    file.sync_all()?;
    dir.rename(&stage, name).map_err(io)?;
    dir.sync().map_err(io)
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
    if proof.version != 1 || FileIdentity::of(&stat) != proof.canonical {
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

pub(super) fn prepare(
    path: &Path,
    owner: [u8; 32],
    nonce: &str,
    mut boundary: impl FnMut(Boundary) -> io::Result<()>,
) -> io::Result<File> {
    let (dir, name, intent_name, ready) = names(path)?;
    if let Some(proof) = record::<Provenance>(&dir, &ready)? {
        if proof.owner != owner {
            return Err(refused("canonical provenance belongs to another owner"));
        }
        let file = trusted(&dir, &name, &proof)?;
        file.sync_all()?;
        dir.sync().map_err(io)?;
        return Ok(file);
    }
    let intent = match record::<Intent>(&dir, &intent_name)? {
        Some(intent) => intent,
        None => {
            let legacy =
                opened(&dir, &name, Trust::Operator)?.map(|(_, stat)| FileIdentity::of(&stat));
            let intent = Intent {
                version: 1,
                owner,
                legacy,
            };
            publish(&dir, &intent_name, &intent, nonce)?;
            intent
        }
    };
    if intent.version != 1 || intent.owner != owner {
        return Err(refused(
            "migration intent belongs to another owner or version",
        ));
    }
    if let Some(legacy) = intent.legacy {
        let quarantine = dir
            .child(".legacy-host-logs", Trust::Private, true)
            .map_err(io)?
            .ok_or_else(|| refused("quarantine missing"))?;
        if quarantine.stat().map_err(io)?.st_dev != dir.stat().map_err(io)?.st_dev {
            return Err(refused("quarantine is on another filesystem"));
        }
        let destination =
            FileName::new(&format!("{:x}-{:x}.legacy", legacy.device, legacy.inode)).map_err(io)?;
        let retained = opened(&quarantine, &destination, Trust::Private)?;
        match retained {
            Some((_, stat)) if FileIdentity::of(&stat) == legacy => {
                if dir
                    .entry(&name)
                    .map_err(io)?
                    .is_some_and(|stat| FileIdentity::of(&stat) == legacy)
                {
                    return Err(refused("legacy identity appears at both paths"));
                }
            }
            Some(_) => {
                return Err(refused(
                    "quarantine destination contains other operator data",
                ))
            }
            None => {
                let (file, stat) = opened(&dir, &name, Trust::Operator)?
                    .ok_or_else(|| refused("legacy identity missing at both paths"))?;
                if FileIdentity::of(&stat) != legacy {
                    return Err(refused(
                        "legacy public identity changed; newer file was left untouched",
                    ));
                }
                fs::fchmod(&file, 0o600)?;
                file.sync_all()?;
                boundary(Boundary::BeforeRename)?;
                if dir
                    .entry(&name)
                    .map_err(io)?
                    .map(|stat| FileIdentity::of(&stat))
                    != Some(legacy)
                {
                    return Err(refused("legacy identity changed before rename"));
                }
                dir.move_to(&name, &quarantine, &destination).map_err(io)?;
                quarantine.sync().map_err(io)?;
                dir.sync().map_err(io)?;
                boundary(Boundary::AfterRename)?;
            }
        }
    }
    let (file, stat) = match opened(&dir, &name, Trust::Private)? {
        Some((file, stat)) if stat.st_size == 0 => (file, stat),
        Some(_) => {
            return Err(refused(
                "unproven replacement contains data; left untouched",
            ))
        }
        None => dir
            .open_file(
                &name,
                libc::O_RDWR | libc::O_APPEND | libc::O_EXCL,
                Some(0o600),
                Trust::Private,
            )
            .map_err(io)?
            .ok_or_else(|| refused("canonical creation missing"))?,
    };
    fs::fchmod(&file, 0o600)?;
    file.sync_all()?;
    dir.sync().map_err(io)?;
    boundary(Boundary::AfterCreate)?;
    boundary(Boundary::BeforeReady)?;
    publish(
        &dir,
        &ready,
        &Provenance {
            version: 1,
            owner,
            canonical: FileIdentity::of(&stat),
        },
        nonce,
    )?;
    boundary(Boundary::AfterReady)?;
    Ok(file)
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
    let proof = record::<Provenance>(&dir, &ready)?.ok_or_else(|| {
        refused("log has not completed canonical cutover; reopen after its host owner starts")
    })?;
    let (file, stat) = dir
        .open_file(&name, libc::O_RDONLY, None, Trust::Private)
        .map_err(io)?
        .ok_or_else(|| refused("canonical log missing"))?;
    if proof.version != 1 || proof.canonical != FileIdentity::of(&stat) || stat.st_nlink != 1 {
        return Err(refused("canonical read identity changed"));
    }
    let log = PlainLog {
        identity: proof.canonical,
        length: stat.st_size as u64,
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

fn tap_name(name: &FileName) -> io::Result<FileName> {
    name.with_suffix(".styled-tap").map_err(io)
}
pub(super) fn tap(path: &Path, id: FileIdentity) -> io::Result<super::super::styled_tap::Tap> {
    let (dir, name, _, _) = names(path)?;
    Ok(super::super::styled_tap::Tap::new(
        dir,
        tap_name(&name)?,
        [id.device, id.inode],
        64 * 1024,
    ))
}
pub(super) fn styles(path: &Path, read: &PlainRead) -> Vec<effigy_core::terminal_text::StyledSpan> {
    let fallback = || {
        vec![effigy_core::terminal_text::StyledSpan {
            text: String::from_utf8(read.bytes.clone()).unwrap_or_default(),
            style: Default::default(),
        }]
    };
    let Ok((dir, name, _, _)) = names(path) else {
        return fallback();
    };
    let Ok(name) = tap_name(&name) else {
        return fallback();
    };
    if !identity(path).is_ok_and(|log| log.identity == read.log.identity) {
        return fallback();
    }
    super::super::styled_tap::read(
        &dir,
        &name,
        [read.log.identity.device, read.log.identity.inode],
        read.from,
        &read.bytes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    #[test]
    fn cutover_quarantines_raw_prefix_and_retries_every_boundary() {
        for stop in [
            Boundary::BeforeRename,
            Boundary::AfterRename,
            Boundary::AfterCreate,
            Boundary::BeforeReady,
            Boundary::AfterReady,
        ] {
            let home = effigy_test_support::short_tempdir("lm-").unwrap();
            let path = home.path().join("app.log");
            let raw = b"\x1b[31mlegacy-secret-457\x1b[0m\n";
            std::fs::write(&path, raw).unwrap();
            let old = std::fs::metadata(&path).unwrap();
            assert!(prepare(&path, [3; 32], "fault", |at| if at == stop {
                Err(io::Error::other("crash"))
            } else {
                Ok(())
            })
            .is_err());
            // Unproven prefixes are never openable through the canonical API.
            if stop != Boundary::AfterReady {
                assert!(open(&path).is_err());
            }
            let mut canonical = prepare(&path, [3; 32], "retry", |_| Ok(())).unwrap();
            canonical.write_all(b"safe\n").unwrap();
            canonical.sync_all().unwrap();
            assert_ne!(canonical.metadata().unwrap().ino(), old.ino());
            assert_eq!(std::fs::read(&path).unwrap(), b"safe\n");
            assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
            let private = home.path().join(".legacy-host-logs");
            assert_eq!(std::fs::metadata(&private).unwrap().mode() & 0o777, 0o700);
            let entries = std::fs::read_dir(private)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(entries.len(), 1);
            assert!(!entries[0].path().to_str().unwrap().ends_with(".log"));
            assert_eq!(std::fs::read(entries[0].path()).unwrap(), raw);
            assert_eq!(entries[0].metadata().unwrap().mode() & 0o777, 0o600);
            let again = prepare(&path, [3; 32], "restart", |_| Ok(())).unwrap();
            assert_eq!(
                again.metadata().unwrap().ino(),
                canonical.metadata().unwrap().ino()
            );
        }
    }
    #[test]
    fn direct_reader_resets_on_new_inode_even_when_new_file_is_larger() {
        let home = effigy_test_support::short_tempdir("lm-").unwrap();
        let path = home.path().join("app.log");
        std::fs::write(&path, b"\x1b[31mlegacy-secret\n").unwrap();
        let old = std::fs::metadata(&path).unwrap();
        let old_identity = FileIdentity {
            device: old.dev(),
            inode: old.ino(),
        };
        assert!(read(&path, Some(old_identity), old.len(), 64 * 1024).is_err());
        let mut writer = prepare(&path, [3; 32], "cutover", |_| Ok(())).unwrap();
        let safe = "safe output\n".repeat(100);
        writer.write_all(safe.as_bytes()).unwrap();
        writer.sync_all().unwrap();
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
            std::fs::write(&other, b"retain all operator bytes").unwrap();
            match kind {
                0 => std::os::unix::fs::symlink(&other, &path).unwrap(),
                1 => std::fs::create_dir(&path).unwrap(),
                _ => std::fs::hard_link(&other, &path).unwrap(),
            }
            assert!(prepare(&path, [3; 32], "refusal", |_| Ok(())).is_err());
            assert!(identity(&path).is_err());
            assert_eq!(std::fs::read(&other).unwrap(), b"retain all operator bytes");
            assert!(!home.path().join(".legacy-host-logs").exists());
        }
    }
    #[test]
    fn replacement_and_foreign_owner_are_left_untouched() {
        let home = effigy_test_support::short_tempdir("lm-").unwrap();
        let path = home.path().join("app.log");
        let file = prepare(&path, [3; 32], "first", |_| Ok(())).unwrap();
        drop(file);
        assert!(prepare(&path, [4; 32], "other", |_| Ok(())).is_err());
        let replacement = home.path().join("replacement");
        std::fs::write(&replacement, b"operator data").unwrap();
        std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::rename(replacement, &path).unwrap();
        assert!(prepare(&path, [3; 32], "retry", |_| Ok(())).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"operator data");
    }
}
