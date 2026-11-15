//! Atomic publication for the JSON files the state surface writes.
//!
//! State reports (`.effigy/reports/state/<stack>/latest-<kind>.json`, their
//! history and compatibility copies) and staged artifact metadata are read
//! while another process may be rewriting them: `StateStackHistoryReport::scan`
//! lists and parses every report in a stack directory, and a later state apply
//! reads a staged `effigy-artifact.json` while a re-stage of the same source
//! rewrites it. A plain `fs::write` truncates the destination first, so a
//! concurrent reader can parse an empty or partial report and report the
//! generation as malformed.
//!
//! Writing one complete generation to a uniquely named temporary file beside
//! the destination, syncing it, renaming it over the destination and syncing
//! the directory keeps every reader on either the previous complete file or
//! the new complete file.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// How many temp names to try before reporting the collision, in case a
/// crashed writer left a temporary file behind under a reused process id.
const TEMP_ATTEMPTS: u32 = 8;

/// Replace `path` with `bytes` atomically.
///
/// The temporary file lives in the destination directory so the rename stays a
/// same-directory rename (atomic on the supported platforms). The destination's
/// parent directory is created when missing, the written file and the directory
/// entry are synced before returning, and the temporary file is removed if the
/// write or rename fails, leaving any previous destination untouched.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&dir)?;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());

    let mut collision = None;
    for _ in 0..TEMP_ATTEMPTS {
        let temporary = dir.join(format!(
            ".{file_name}.tmp-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return publish(file, &temporary, path, &dir, bytes),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => collision = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(collision.unwrap_or_else(|| io::Error::other("no temporary name available")))
}

fn publish(
    mut file: File,
    temporary: &Path,
    path: &Path,
    dir: &Path,
    bytes: &[u8],
) -> io::Result<()> {
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let result = written
        .and_then(|()| fs::rename(temporary, path))
        .and_then(|()| sync_directory(dir));
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(unix)]
fn sync_directory(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_dir: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir(label: &str) -> PathBuf {
        effigy_test_support::Fixture::process_lifetime(&format!("effigy-state-atomic-{label}-"))
            .expect("fixture directory")
    }

    #[test]
    fn write_atomic_replaces_content_and_leaves_no_temporary_files() {
        let dir = fixture_dir("replace");
        let path = dir.join("report.json");

        write_atomic(&path, b"first\n").expect("first atomic write");
        assert_eq!(fs::read(&path).expect("first read"), b"first\n");

        write_atomic(&path, b"second\n").expect("second atomic write");
        assert_eq!(fs::read(&path).expect("second read"), b"second\n");

        let entries: Vec<_> = fs::read_dir(&dir)
            .expect("fixture dir")
            .map(|entry| entry.expect("dir entry").file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("report.json")]);
    }

    #[test]
    fn write_atomic_creates_missing_directories() {
        let dir = fixture_dir("nested");
        let path = dir.join("nested/reports/report.json");

        write_atomic(&path, b"content\n").expect("atomic write into new dirs");
        assert_eq!(fs::read(&path).expect("read"), b"content\n");
    }
}
