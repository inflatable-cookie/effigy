use crate::MAX_LOCAL_FILE_BYTES;
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static PENDING_TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

const DIRECTORY_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Authority {
    pub format: String,
    pub version: u32,
    pub holder: String,
    pub endpoint: PathBuf,
    pub epoch: u64,
    pub pid: u32,
    #[serde(rename = "startIdentity")]
    pub start_identity: String,
}

#[derive(Debug)]
pub enum TrustError {
    Io(std::io::Error),
    Invalid(&'static str),
    Json(serde_json::Error),
    Unsupported,
}

impl std::fmt::Display for TrustError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "untrusted or unavailable host-run state: {error}"),
            Self::Invalid(reason) => write!(f, "untrusted host-run state: {reason}"),
            Self::Json(error) => write!(f, "invalid host-run authority: {error}"),
            Self::Unsupported => f.write_str("host-run peer proof is unsupported on this platform"),
        }
    }
}
impl std::error::Error for TrustError {}
impl From<std::io::Error> for TrustError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for TrustError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

pub struct HostRunRoot {
    pub(super) path: PathBuf,
    pub(super) directory: File,
    uid: u32,
}

impl HostRunRoot {
    /// Discover `~/.local/state/host-run`; no directory is created or repaired.
    pub fn discover(home: &Path) -> Result<(Self, Authority), TrustError> {
        Self::open(home.join(".local/state/host-run"))
    }

    /// Open an explicit subtree root, mainly for private fixture directories.
    pub fn open(path: impl AsRef<Path>) -> Result<(Self, Authority), TrustError> {
        let path = std::fs::canonicalize(path)?;
        let cpath = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| TrustError::Invalid("root path contains NUL"))?;
        let fd = unsafe {
            libc::open(
                cpath.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let directory = unsafe { File::from_raw_fd(fd) };
        let uid = unsafe { libc::geteuid() } as u32;
        verify_fd(&directory, libc::S_IFDIR as u32, DIRECTORY_MODE, uid)?;
        let root = Self {
            path,
            directory,
            uid,
        };
        let authority = root.read_authority()?;
        root.verify_socket(&authority)?;
        Ok((root, authority))
    }

    pub(super) fn read_authority(&self) -> Result<Authority, TrustError> {
        let file = open_file(self.directory.as_raw_fd(), "authority.json", self.uid)?;
        let mut bytes = Vec::new();
        file.take((MAX_LOCAL_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_LOCAL_FILE_BYTES {
            return Err(TrustError::Invalid("authority file exceeds size limit"));
        }
        let authority: Authority = serde_json::from_slice(&bytes)?;
        if authority.format != "host.run.authority"
            || authority.version != 1
            || authority.epoch == 0
            || authority.pid == 0
            || authority.holder.is_empty()
            || authority.start_identity.is_empty()
        {
            return Err(TrustError::Invalid(
                "unsupported authority format, version, epoch or identity",
            ));
        }
        let expected = self.path.join("run/scheduler.sock");
        if authority.endpoint != expected {
            return Err(TrustError::Invalid(
                "authority endpoint escapes the owned subtree",
            ));
        }
        Ok(authority)
    }

    pub(super) fn verify_socket(&self, authority: &Authority) -> Result<(), TrustError> {
        let run = open_dir(self.directory.as_raw_fd(), "run", self.uid)?;
        let name = CString::new("scheduler.sock").expect("static name");
        let mut stat = std::mem::MaybeUninit::<libc::stat>::zeroed();
        let result = unsafe {
            libc::fstatat(
                run.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let stat = unsafe { stat.assume_init() };
        if (stat.st_mode as u32 & libc::S_IFMT as u32) != libc::S_IFSOCK as u32
            || stat.st_uid as u32 != self.uid
            || (stat.st_mode as u32 & 0o7777) != FILE_MODE
            || authority.endpoint != self.path.join("run/scheduler.sock")
        {
            return Err(TrustError::Invalid(
                "scheduler socket has unsafe type, owner, mode or path",
            ));
        }
        Ok(())
    }

    pub(super) fn open_token_key(&self) -> Result<Vec<u8>, TrustError> {
        let mut file = open_file(self.directory.as_raw_fd(), "token.key", self.uid)?;
        read_bounded(&mut file, MAX_LOCAL_FILE_BYTES)
    }

    pub(super) fn open_pending_facts(&self) -> Result<File, TrustError> {
        open_or_create_private_file(self.directory.as_raw_fd(), "pending-facts.jsonl", self.uid)
    }

    pub(super) fn open_pending_lock(&self) -> Result<File, TrustError> {
        open_or_create_private_file(self.directory.as_raw_fd(), "pending-facts.lock", self.uid)
    }

    pub(super) fn replace_pending_facts(&self, bytes: &[u8]) -> Result<(), TrustError> {
        let parent = self.directory.as_raw_fd();
        let mut temp_name = None;
        let mut file = None;
        for _ in 0..8 {
            let suffix = PENDING_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let name = format!(".pending-facts.{}.{}.tmp", std::process::id(), suffix);
            let cname = CString::new(name.as_str()).expect("generated filename has no NUL");
            let fd = unsafe {
                libc::openat(
                    parent,
                    cname.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_CLOEXEC
                        | libc::O_NOFOLLOW,
                    FILE_MODE as libc::mode_t as libc::c_uint,
                )
            };
            if fd >= 0 {
                temp_name = Some(name);
                file = Some(unsafe { File::from_raw_fd(fd) });
                break;
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(error.into());
            }
        }
        let name = temp_name.ok_or(TrustError::Invalid(
            "could not reserve a pending-facts temp file",
        ))?;
        let mut file = file.expect("reserved temp file");
        let result = (|| {
            verify_fd(&file, libc::S_IFREG as u32, FILE_MODE, self.uid)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            let from = CString::new(name.as_str()).expect("generated filename has no NUL");
            let to = CString::new("pending-facts.jsonl").expect("static name");
            if unsafe { libc::renameat(parent, from.as_ptr(), parent, to.as_ptr()) } != 0 {
                return Err(TrustError::Io(std::io::Error::last_os_error()));
            }
            self.directory.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let cname = CString::new(name.as_str()).expect("generated filename has no NUL");
            unsafe {
                libc::unlinkat(parent, cname.as_ptr(), 0);
            }
        }
        result
    }

    pub(super) fn uid(&self) -> u32 {
        self.uid
    }
}

pub(super) fn read_bounded(reader: &mut impl Read, max: usize) -> Result<Vec<u8>, TrustError> {
    let mut bytes = Vec::new();
    reader.take((max + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(TrustError::Invalid("file exceeds size limit"));
    }
    Ok(bytes)
}

fn open_dir(parent: RawFd, name: &str, uid: u32) -> Result<File, TrustError> {
    let name = CString::new(name).map_err(|_| TrustError::Invalid("invalid path component"))?;
    let fd = unsafe {
        libc::openat(
            parent,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    verify_fd(&file, libc::S_IFDIR as u32, DIRECTORY_MODE, uid)?;
    Ok(file)
}

fn open_file(parent: RawFd, name: &str, uid: u32) -> Result<File, TrustError> {
    let name = CString::new(name).map_err(|_| TrustError::Invalid("invalid path component"))?;
    let fd = unsafe {
        libc::openat(
            parent,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    verify_fd(&file, libc::S_IFREG as u32, FILE_MODE, uid)?;
    Ok(file)
}

fn open_or_create_private_file(parent: RawFd, name: &str, uid: u32) -> Result<File, TrustError> {
    let name = CString::new(name).map_err(|_| TrustError::Invalid("invalid path component"))?;
    let fd = unsafe {
        libc::openat(
            parent,
            name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            FILE_MODE as libc::mode_t as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    verify_fd(&file, libc::S_IFREG as u32, FILE_MODE, uid)?;
    Ok(file)
}

fn verify_fd(file: &File, file_type: u32, mode: u32, uid: u32) -> Result<(), TrustError> {
    let metadata = file.metadata()?;
    let actual_type = metadata.mode() as u32 & libc::S_IFMT as u32;
    if !metadata.is_owned_by(uid)
        || actual_type != file_type
        || metadata.mode() as u32 & 0o7777 != mode
    {
        return Err(TrustError::Invalid("unsafe owner, type or permissions"));
    }
    Ok(())
}

use std::os::unix::fs::MetadataExt;
trait MetadataOwner {
    fn is_owned_by(&self, uid: u32) -> bool;
}
impl MetadataOwner for std::fs::Metadata {
    fn is_owned_by(&self, uid: u32) -> bool {
        self.uid() == uid
    }
}
