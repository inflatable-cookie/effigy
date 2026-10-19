//! Existing gateway lifecycle exclusion; kernel adoption belongs to the gateway engine.

use crate::{identity, GatewayError};
use std::fs::{self, File, OpenOptions};
use std::path::Path;

/// Owner-only exclusive lock covering one `up`/`down` command.
#[derive(Debug)]
pub struct GatewayTransitionLock(File);

impl GatewayTransitionLock {
    /// Acquire `gateway.transition.lock` without waiting.
    pub fn try_acquire(pid_path: &Path) -> Result<Self, GatewayError> {
        let parent = pid_path
            .parent()
            .ok_or_else(|| invalid_record("gateway PID path has no parent"))?;
        let owner_uid = identity::ensure_trusted_gateway_parent(pid_path)?;
        acquire_transition_lock(parent, owner_uid, false)
    }

    /// Acquire `gateway.transition.lock`, waiting for an active transition.
    /// Certificate generation uses this form because multiple managed
    /// profiles can request different certificates from the same private CA.
    pub fn acquire(pid_path: &Path) -> Result<Self, GatewayError> {
        let parent = pid_path
            .parent()
            .ok_or_else(|| invalid_record("gateway PID path has no parent"))?;
        let owner_uid = identity::ensure_trusted_gateway_parent(pid_path)?;
        acquire_transition_lock(parent, owner_uid, true)
    }
}

#[cfg(unix)]
fn acquire_transition_lock(
    parent: &Path,
    owner_uid: u32,
    wait: bool,
) -> Result<GatewayTransitionLock, GatewayError> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let lock_path = parent.join("gateway.transition.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(&lock_path)
        .map_err(GatewayError::Io)?;
    let metadata = file.metadata().map_err(GatewayError::Io)?;
    let path_metadata = fs::symlink_metadata(&lock_path).map_err(GatewayError::Io)?;
    if path_metadata.file_type().is_symlink()
        || path_metadata.dev() != metadata.dev()
        || path_metadata.ino() != metadata.ino()
        || !metadata.is_file()
        || metadata.mode() & 0o077 != 0
        || (metadata.uid() != owner_uid && metadata.uid() != 0)
    {
        return Err(invalid_record("gateway transition lock is unsafe"));
    }
    if nix::unistd::Uid::effective().is_root() && metadata.uid() != owner_uid {
        nix::unistd::fchown(
            &file,
            Some(nix::unistd::Uid::from_raw(owner_uid)),
            Some(nix::unistd::Gid::from_raw(metadata.gid())),
        )
        .map_err(|error| GatewayError::Io(error.into()))?;
    }
    let lock_result = if wait {
        fs2::FileExt::lock_exclusive(&file)
    } else {
        fs2::FileExt::try_lock_exclusive(&file)
    };
    match lock_result {
        Ok(()) => Ok(GatewayTransitionLock(file)),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            Err(GatewayError::TransitionLockHeld)
        }
        Err(error) => Err(GatewayError::Io(error)),
    }
}

#[cfg(not(unix))]
fn acquire_transition_lock(
    _parent: &Path,
    _owner_uid: u32,
    _wait: bool,
) -> Result<GatewayTransitionLock, GatewayError> {
    Err(GatewayError::Io(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "gateway transition lock is unsupported on this platform",
    )))
}

#[cfg(unix)]
impl Drop for GatewayTransitionLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

fn invalid_record(message: &str) -> GatewayError {
    GatewayError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message.to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn private_gateway_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("fixture directory");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn transition_lock_refuses_contention() {
        let dir = private_gateway_dir();
        let pid_path = dir.path().join("gateway.pid");
        fs::write(&pid_path, "4242\n").unwrap();
        let first = GatewayTransitionLock::try_acquire(&pid_path).expect("first lock");
        let second = GatewayTransitionLock::try_acquire(&pid_path);
        assert!(matches!(second, Err(GatewayError::TransitionLockHeld)));
        drop(first);
        GatewayTransitionLock::try_acquire(&pid_path).expect("lock after release");
    }
    #[cfg(unix)]
    #[test]
    fn transition_lock_can_wait_for_private_tls_generation() {
        use std::sync::mpsc;
        use std::time::Duration;

        let dir = private_gateway_dir();
        let pid_path = dir.path().join("gateway.pid");
        fs::write(&pid_path, "4242\n").unwrap();
        let held = GatewayTransitionLock::try_acquire(&pid_path).expect("held lock");
        let (started_tx, started_rx) = mpsc::channel();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let contender_path = pid_path.clone();
        let contender = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let _lock = GatewayTransitionLock::acquire(&contender_path).unwrap();
            acquired_tx.send(()).unwrap();
        });

        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(acquired_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(held);
        acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        contender.join().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn transition_lock_refuses_symlinked_effigy_without_mutating_target() {
        use std::os::unix::fs::{symlink, MetadataExt};

        let root = tempfile::tempdir().expect("fixture directory");
        let home = root.path().join("home");
        let outside = root.path().join("outside");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o700)).unwrap();
        let sentinel = outside.join("sentinel");
        fs::write(&sentinel, b"keep").unwrap();
        let outside_names = || {
            fs::read_dir(&outside)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<std::collections::BTreeSet<_>>()
        };
        let before_outside = outside_names();
        let before_sentinel = fs::read(&sentinel).unwrap();
        let before_ino = fs::symlink_metadata(&outside).unwrap().ino();
        symlink(&outside, home.join(".effigy")).unwrap();
        let pid_path = home.join(".effigy").join("gateway").join("gateway.pid");
        match GatewayTransitionLock::try_acquire(&pid_path) {
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message.contains("unsafe"),
                    "expected ancestor-trust refusal, got {message}"
                );
            }
            Ok(_) => panic!("symlink .effigy must refuse lock acquisition"),
        }

        assert_eq!(outside_names(), before_outside);
        assert_eq!(fs::read(&sentinel).unwrap(), before_sentinel);
        assert!(!outside.join("gateway").exists());
        assert_eq!(fs::symlink_metadata(&outside).unwrap().ino(), before_ino);
        assert!(home
            .join(".effigy")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
    }
    #[cfg(unix)]
    #[test]
    fn transition_lock_creates_genuine_absent_parent() {
        use std::os::unix::fs::MetadataExt;

        let root = tempfile::tempdir().expect("fixture directory");
        let home = root.path().join("home");
        fs::create_dir(&home).unwrap();
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
        let pid_path = home.join(".effigy").join("gateway").join("gateway.pid");

        let lock = GatewayTransitionLock::try_acquire(&pid_path).expect("genuine absent parent");
        drop(lock);

        let effigy = home.join(".effigy");
        let gateway = effigy.join("gateway");
        assert!(effigy.symlink_metadata().unwrap().is_dir());
        assert!(!effigy.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(
            fs::symlink_metadata(&effigy).unwrap().uid(),
            fs::metadata(&home).unwrap().uid()
        );
        assert!(gateway.is_dir());
        assert!(gateway.join("gateway.transition.lock").is_file());
        assert!(!pid_path.exists());
    }
}
