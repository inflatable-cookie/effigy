//! Run-bound identity of the host backend used by remote custody.
//!
//! The cleared-environment sentinel must not search a new PATH. This record
//! identifies one executable and working directory; it contains no guest
//! environment, credentials, nonce or output.
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use effigy_kernel::id::Digest;
use serde::{Deserialize, Serialize};

const MAX_EXECUTABLE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileIdentity {
    device: u64,
    inode: u64,
    size: u64,
    mode: u32,
    modified_seconds: i64,
    modified_nanos: i64,
    changed_seconds: i64,
    changed_nanos: i64,
}

impl From<&std::fs::Metadata> for FileIdentity {
    fn from(metadata: &std::fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            mode: metadata.mode(),
            modified_seconds: metadata.mtime(),
            modified_nanos: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanos: metadata.ctime_nsec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryBinding {
    program: String,
    executable: PathBuf,
    executable_identity: FileIdentity,
    executable_digest: Digest,
    workdir: PathBuf,
    workdir_device: u64,
    workdir_inode: u64,
}

impl RecoveryBinding {
    /// Capture only the already selected backend. An unresolved or oversized
    /// executable fails before a remote intent or workload is started.
    pub(super) fn capture(
        program: &str,
        workdir: &Path,
        deadline: Instant,
    ) -> Result<Self, String> {
        if !matches!(program, "docker" | "colima") {
            return Err("unsupported recovery backend".into());
        }
        let selected = PathBuf::from(crate::compose::resolve_host_cli_program(program));
        Self::capture_selected(program, &selected, workdir, deadline)
    }

    fn capture_selected(
        program: &str,
        selected: &Path,
        workdir: &Path,
        deadline: Instant,
    ) -> Result<Self, String> {
        if !selected.is_absolute() {
            return Err("backend executable is unresolved".into());
        }
        let executable =
            std::fs::canonicalize(selected).map_err(|_| "backend executable unavailable")?;
        let workdir = std::fs::canonicalize(workdir).map_err(|_| "backend workdir unavailable")?;
        let directory = std::fs::metadata(&workdir).map_err(|_| "backend workdir unavailable")?;
        if !directory.is_dir() {
            return Err("backend workdir is not a directory".into());
        }
        let (executable_identity, executable_digest) = inspect(&executable, deadline)?;
        Ok(Self {
            program: program.into(),
            executable,
            executable_identity,
            executable_digest,
            workdir,
            workdir_device: directory.dev(),
            workdir_inode: directory.ino(),
        })
    }

    /// Recheck immediately before each backend call. File generation includes
    /// ctime as well as inode, size and mtime, so an in-place write is a move.
    /// Recovery also hashes the file once in `verify_record` before this path.
    pub(super) fn verify(
        &self,
        program: &str,
        deadline: Instant,
    ) -> Result<(&Path, &Path), String> {
        if !matches!(self.program.as_str(), "docker" | "colima")
            || self.program != program
            || !self.executable.is_absolute()
            || !self.workdir.is_absolute()
        {
            return Err("backend binding differs".into());
        }
        if std::fs::canonicalize(&self.executable).ok().as_deref()
            != Some(self.executable.as_path())
            || std::fs::canonicalize(&self.workdir).ok().as_deref() != Some(self.workdir.as_path())
        {
            return Err("backend binding path moved".into());
        }
        let directory =
            std::fs::metadata(&self.workdir).map_err(|_| "backend workdir unavailable")?;
        if !directory.is_dir()
            || directory.dev() != self.workdir_device
            || directory.ino() != self.workdir_inode
        {
            return Err("backend workdir moved".into());
        }
        let file =
            std::fs::metadata(&self.executable).map_err(|_| "backend executable unavailable")?;
        if Instant::now() >= deadline || FileIdentity::from(&file) != self.executable_identity {
            return Err("backend executable moved or changed".into());
        }
        Ok((&self.executable, &self.workdir))
    }

    pub(super) fn verify_record(&self, deadline: Instant) -> Result<(&Path, &Path), String> {
        self.verify(&self.program, deadline)?;
        let (identity, digest) = inspect(&self.executable, deadline)?;
        if identity != self.executable_identity || digest != self.executable_digest {
            return Err("backend executable moved or changed".into());
        }
        Ok((&self.executable, &self.workdir))
    }
}

fn inspect(path: &Path, deadline: Instant) -> Result<(FileIdentity, Digest), String> {
    if Instant::now() >= deadline {
        return Err("backend identity deadline exceeded".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "backend executable unavailable")?;
    let before = file
        .metadata()
        .map_err(|_| "backend executable unavailable")?;
    if !before.is_file() || before.mode() & 0o111 == 0 || before.len() > MAX_EXECUTABLE_BYTES {
        return Err("backend executable is not a bounded executable file".into());
    }
    let mut bytes = Vec::new();
    (&file)
        .take(MAX_EXECUTABLE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "backend executable unreadable")?;
    let after = file
        .metadata()
        .map_err(|_| "backend executable unavailable")?;
    let path_metadata = std::fs::metadata(path).map_err(|_| "backend executable unavailable")?;
    if Instant::now() >= deadline
        || u64::try_from(bytes.len()).ok() != Some(before.len())
        || FileIdentity::from(&before) != FileIdentity::from(&after)
        || FileIdentity::from(&before) != FileIdentity::from(&path_metadata)
    {
        return Err("backend executable changed or deadline expired".into());
    }
    Ok((FileIdentity::from(&before), Digest::of(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    #[test]
    fn recovery_binding_rejects_changed_or_missing_executable_and_directory() {
        let scratch = std::env::var_os("TMPDIR").expect("run TMPDIR");
        let root = tempfile::Builder::new()
            .prefix("signal-backend-binding-")
            .tempdir_in(scratch)
            .unwrap()
            .keep();
        let binary = root.join("backend");
        let workdir = root.join("work");
        std::fs::create_dir(&workdir).unwrap();
        std::fs::write(&binary, b"#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let deadline = || Instant::now() + Duration::from_secs(5);
        let binding =
            RecoveryBinding::capture_selected("docker", &binary, &workdir, deadline()).unwrap();
        assert!(binding.verify("docker", deadline()).is_ok());
        assert!(binding.verify("colima", deadline()).is_err());
        std::fs::write(&binary, b"#!/bin/sh\nexit 1\n").unwrap();
        assert!(binding.verify("docker", deadline()).is_err());
        let changed =
            RecoveryBinding::capture_selected("docker", &binary, &workdir, deadline()).unwrap();
        std::fs::rename(&workdir, root.join("moved-work")).unwrap();
        assert!(changed.verify("docker", deadline()).is_err());
        std::fs::rename(root.join("moved-work"), &workdir).unwrap();
        assert!(changed.verify("docker", deadline()).is_ok());
        std::fs::rename(&binary, root.join("moved-backend")).unwrap();
        assert!(changed.verify("docker", deadline()).is_err());
    }

    #[test]
    fn hostcli_recovery_refuses_missing_or_changed_binding() {
        use super::super::{BackendCli, HostCli};
        let scratch = std::env::var_os("TMPDIR").expect("run TMPDIR");
        let root = tempfile::Builder::new()
            .prefix("signal-hostcli-binding-")
            .tempdir_in(scratch)
            .unwrap()
            .keep();
        let binary = root.join("backend");
        std::fs::write(&binary, b"#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let deadline = || Instant::now() + Duration::from_secs(5);
        let binding =
            RecoveryBinding::capture_selected("docker", &binary, &root, deadline()).unwrap();
        let encoded = serde_json::to_value(&binding).unwrap();
        let mut cli = HostCli {
            workdir: root.clone(),
            binding: None,
        };
        assert!(cli.restore_recovery(None, deadline()).is_err());
        assert!(cli.launch_program("docker", deadline()).is_err());
        assert!(cli.restore_recovery(Some(&encoded), deadline()).is_ok());
        assert_eq!(
            cli.launch_program("docker", deadline()).unwrap(),
            binary.as_os_str()
        );
        assert!(cli.launch_program("colima", deadline()).is_err());
        std::fs::write(&binary, b"#!/bin/sh\nexit 1\n").unwrap();
        assert!(cli.launch_program("docker", deadline()).is_err());
        let mut fresh = HostCli {
            workdir: root,
            binding: None,
        };
        assert!(fresh.restore_recovery(Some(&encoded), deadline()).is_err());
    }
}
