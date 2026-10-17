//! Durable in-place file replacement for runtime-generated inputs.
//!
//! Runtime Compose payloads (`<env>.generated.compose.yml`, the workspace,
//! ownership and DNS overrides, and their regeneration checksums) are rewritten
//! on every policy resolution, including the managed shell/exec readiness
//! probes that run once per second while a container environment starts. A
//! plain `fs::write` truncates the destination first, so a concurrent reader —
//! a strict detached-activation admission verify, for example — can observe a
//! missing or partially written input between policy resolutions and refuse
//! publication even though the rendered content is byte-stable. Writing to a
//! unique temporary file in the destination directory and renaming it into
//! place keeps every reader on one complete generation of the file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Replace `path` with `bytes` atomically: write a uniquely named temporary
/// file beside the destination, then rename it over the destination. A
/// same-directory rename is atomic on the supported platforms, so concurrent
/// readers see either the previous complete file or the new complete file and
/// never a truncated intermediate. The destination's parent directory is
/// created when missing. The temporary file is removed if the write or rename
/// fails, leaving any previous destination untouched.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&dir)?;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());
    let temp = dir.join(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    if let Err(error) = fs::write(&temp, bytes) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_replaces_content_and_leaves_no_temporary_files() {
        let dir = tempfile::tempdir().expect("temporary fixture");
        let path = dir.path().join("payload.yml");

        write_atomic(&path, b"first\n").expect("first atomic write");
        assert_eq!(fs::read(&path).expect("first read"), b"first\n");

        write_atomic(&path, b"second\n").expect("second atomic write");
        assert_eq!(fs::read(&path).expect("second read"), b"second\n");

        let leftovers: Vec<_> = fs::read_dir(dir.path()).expect("fixture dir").collect();
        assert_eq!(leftovers.len(), 1, "no temporary files remain");
    }

    #[test]
    fn write_atomic_creates_missing_directories() {
        let dir = tempfile::tempdir().expect("temporary fixture");
        let path = dir.path().join("nested/runtime/payload.yml");

        write_atomic(&path, b"content\n").expect("atomic write into new dirs");
        assert_eq!(fs::read(&path).expect("read"), b"content\n");
    }

    #[test]
    fn concurrent_readers_never_observe_a_truncated_generation() {
        let dir = tempfile::tempdir().expect("temporary fixture");
        let path = dir.path().join("payload.yml");
        let content = "services:\n".to_owned() + &"  key: value\n".repeat(64);
        write_atomic(&path, content.as_bytes()).expect("seed atomic write");

        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer_stop = stop.clone();
        let writer_path = path.clone();
        let writer_content = content.clone();
        let writer = std::thread::spawn(move || {
            while !writer_stop.load(std::sync::atomic::Ordering::Relaxed) {
                write_atomic(&writer_path, writer_content.as_bytes()).expect("atomic rewrite");
            }
        });
        let mut reads = 0;
        while reads < 5000 {
            let observed = fs::read_to_string(&path).expect("read stays complete");
            assert_eq!(
                observed, content,
                "a reader observed a torn generation after {reads} clean reads"
            );
            reads += 1;
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        writer.join().expect("writer joins");
    }
}
