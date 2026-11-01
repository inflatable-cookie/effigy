//! Safe files (K13): descriptor-relative, no-follow, owner- and mode-checked,
//! size-bounded opens, and the local-filesystem check.
// libc mode, nlink and statfs widths differ between Linux and macOS, so some
// casts are redundant on one platform and required on the other.
#![allow(clippy::unnecessary_cast)]

use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString, OsString};
use std::fmt;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

pub(crate) const DIR_MODE: u32 = 0o700;
pub(crate) const FILE_MODE: u32 = 0o600;
/// Reason reported when a trusted object carries group or other bits.
pub(crate) const OPEN_MODE_REASON: &str = "group- or other-accessible mode";

/// Fixed-shape error facts. `errno` lets callers tell ENOSPC from EPERM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IoError {
    pub op: &'static str,
    pub path: PathBuf,
    pub errno: Option<i32>,
    pub message: String,
}
impl IoError {
    pub(crate) fn new(op: &'static str, path: &Path, error: &io::Error) -> Self {
        Self {
            op,
            path: path.to_path_buf(),
            errno: error.raw_os_error(),
            message: error.to_string(),
        }
    }
    pub fn is_no_space(&self) -> bool {
        self.errno == Some(libc::ENOSPC)
    }
    pub fn is_permission(&self) -> bool {
        matches!(self.errno, Some(libc::EPERM | libc::EACCES))
    }
}
impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}: {}", self.op, self.path.display(), self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum FsError {
    /// Guest placement asked for host state; nothing was touched.
    HostAuthorityRequired,
    Untrusted {
        path: PathBuf,
        reason: String,
    },
    UnsupportedFilesystem {
        path: PathBuf,
        fs: String,
    },
    Io(IoError),
}
impl FsError {
    pub(crate) fn untrusted(path: &Path, reason: impl Into<String>) -> Self {
        Self::Untrusted {
            path: path.to_path_buf(),
            reason: reason.into(),
        }
    }
    pub(crate) fn io(op: &'static str, path: &Path, error: &io::Error) -> Self {
        // ELOOP is O_NOFOLLOW meeting a symlink: a trust failure, not I/O.
        if error.raw_os_error() == Some(libc::ELOOP) {
            return Self::untrusted(path, "symlinked component");
        }
        Self::Io(IoError::new(op, path, error))
    }
    pub(crate) fn is_not_found(&self) -> bool {
        matches!(self, Self::Io(error) if error.errno == Some(libc::ENOENT))
    }
    /// The only defect was a group/other-accessible mode on an otherwise
    /// trusted, operator-owned object.
    pub(crate) fn is_open_mode(&self) -> bool {
        matches!(self, Self::Untrusted { reason, .. } if reason == OPEN_MODE_REASON)
    }
}
impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HostAuthorityRequired => f.write_str(
                "host state is not available to a guest placement; run this on the host",
            ),
            Self::Untrusted { path, reason } if reason == OPEN_MODE_REASON => write!(
                f,
                "{} is readable by other users; run `chmod 700 {}`",
                path.display(),
                path.display()
            ),
            Self::Untrusted { path, reason } => write!(
                f,
                "{} is not trusted state ({reason}); {}",
                path.display(),
                trust_next_action(reason)
            ),
            Self::UnsupportedFilesystem { path, fs } => write!(
                f,
                "{} is on a network filesystem ({fs}); move the state root to a local disk",
                path.display()
            ),
            Self::Io(error) => error.fmt(f),
        }
    }
}

/// Why an object was refused, and what makes it acceptable again.
fn trust_next_action(reason: &str) -> &'static str {
    match reason {
        "wrong owner" => "it belongs to another user, so remove it or run as its owner",
        "symlinked component" => {
            "Effigy never follows a symlink in its state root, so replace it with a real path"
        }
        "hard-linked file" => "it has more than one link, so remove the extra link",
        "NUL in path" => "the path is invalid, so choose one without a NUL byte",
        _ => "fix its ownership and permissions, then retry",
    }
}
impl std::error::Error for FsError {}

/// Filesystem identity of a verified open directory, independent of its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryIdentity {
    /// The mount-time device number (`st_dev`). macOS assigns it at every
    /// mount, so it is never evidence across a reboot (effigy#337).
    pub device: u64,
    pub inode: u64,
}
impl DirectoryIdentity {
    /// Whether both identities name the same directory object. Only the inode
    /// is compared: a reboot renumbers the device of an unchanged directory.
    pub fn is_same_directory(&self, other: &Self) -> bool {
        self.inode == other.inode
    }
}

/// A single path component: no separator, NUL, `.` or `..`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileName(String);
impl FileName {
    pub const MAX_BYTES: usize = 200;
    pub fn new(name: &str) -> Result<Self, FsError> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.len() > Self::MAX_BYTES
            || name.contains(['/', '\0'])
        {
            return Err(FsError::untrusted(Path::new(name), "invalid file name"));
        }
        Ok(Self(name.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub(crate) fn with_suffix(&self, suffix: &str) -> Result<Self, FsError> {
        Self::new(&format!("{}{suffix}", self.0))
    }
}
impl fmt::Display for FileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Trust policy for one opened object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trust {
    /// Above the state root: only proves the component is a real directory.
    /// Its owner and mode are never checked, so a umask-002 checkout, a CI
    /// container or `/Users/Shared` cannot refuse the operator's own state.
    Ancestor,
    /// Operator-owned and not group- or other-writable (user files, `~/.effigy`).
    Operator,
    /// Operator-owned, no group or other bits (the state root, 0700/0600).
    Private,
}

/// An open directory verified on the way down. Every later open is relative
/// to it, so a path swap after verification cannot redirect a write.
#[derive(Debug)]
pub struct DirFd {
    fd: OwnedFd,
    path: PathBuf,
    setup_uid: Option<u32>,
}
impl Clone for DirFd {
    /// A duplicate descriptor for the same verified directory: SQLite and the
    /// store keep one open for their own lifetime without re-walking paths.
    fn clone(&self) -> Self {
        Self {
            fd: self.fd.try_clone().expect("duplicate directory descriptor"),
            path: self.path.clone(),
            setup_uid: self.setup_uid,
        }
    }
}
impl DirFd {
    /// Opens a user directory. The operator's own path spelling (for example
    /// macOS `/var` → `/private/var`) is resolved first; every component is
    /// then reopened no-follow from `/`. Only `.effigy/state` carries an
    /// ownership and mode check; every other directory is opened as it is.
    pub fn open(path: &Path) -> Result<Self, FsError> {
        Self::walk(path, Trust::Ancestor)
    }
    pub(crate) fn walk(path: &Path, last: Trust) -> Result<Self, FsError> {
        Self::walk_with_setup(path, last, None)
    }
    fn walk_with_setup(path: &Path, last: Trust, setup_uid: Option<u32>) -> Result<Self, FsError> {
        if !path.is_absolute() {
            return Err(FsError::untrusted(path, "relative path"));
        }
        let real = std::fs::canonicalize(path).map_err(|e| FsError::io("resolve", path, &e))?;
        let mut dir = Self {
            fd: open_raw(libc::AT_FDCWD, c"/", libc::O_RDONLY | libc::O_DIRECTORY, 0)
                .map_err(|e| FsError::io("open", Path::new("/"), &e))?,
            path: PathBuf::from("/"),
            setup_uid,
        };
        dir.check(&dir.stat()?, Trust::Ancestor, Kind::Dir, &dir.path)?;
        let parts: Vec<_> = real
            .components()
            .filter_map(|part| match part {
                Component::Normal(name) => Some(name.to_os_string()),
                _ => None,
            })
            .collect();
        for (index, name) in parts.iter().enumerate() {
            let trust = if index + 1 == parts.len() {
                last
            } else {
                Trust::Ancestor
            };
            dir = dir.descend(name.as_bytes(), trust, None)?;
        }
        Ok(dir)
    }
    /// A private child directory, created 0700 when `create` is set.
    /// `Ok(None)` means it does not exist and was not created.
    pub(crate) fn child(
        &self,
        name: &str,
        trust: Trust,
        create: bool,
    ) -> Result<Option<Self>, FsError> {
        match self.descend(name.as_bytes(), trust, create.then_some(DIR_MODE)) {
            Ok(dir) => Ok(Some(dir)),
            Err(error) if error.is_not_found() && !create => Ok(None),
            Err(error) => Err(error),
        }
    }
    /// A private child directory whose *only* acceptable repair is tightening
    /// an existing one's group/other bits to 0700.
    ///
    /// v0.14's `effigy state` created `.effigy/state/` with the process umask
    /// (0755), which Q-010 later made the kernel's private checkout state root
    /// (papercut 9e5134a4). For one release a checkout whose root already
    /// exists operator-owned, local and not writable by another user is
    /// adopted by tightening its mode; nothing looser is ever adopted, and the
    /// strict check still runs on the result. Observation paths never call
    /// this, so a foreign checkout's bad mode stays `Unknown` there.
    pub(crate) fn private_child_adopt(&self, name: &str) -> Result<Self, FsError> {
        match self.child(name, Trust::Private, true) {
            Ok(Some(dir)) => Ok(dir),
            Ok(None) => Err(FsError::untrusted(
                &self.join(name),
                "private directory was not created",
            )),
            Err(error) if error.is_open_mode() => {
                // `Trust::Operator` rejects a foreign owner and any
                // group/other *write* bit before the mode is tightened.
                let Some(existing) = self.child(name, Trust::Operator, false)? else {
                    return Err(error);
                };
                existing.require_local()?;
                existing.chmod(DIR_MODE)?;
                existing.sync()?;
                existing.check(
                    &existing.stat()?,
                    Trust::Private,
                    Kind::Dir,
                    existing.path(),
                )?;
                Ok(existing)
            }
            Err(error) => Err(error),
        }
    }
    fn descend(&self, name: &[u8], trust: Trust, create: Option<u32>) -> Result<Self, FsError> {
        let path = self.path.join(std::ffi::OsStr::from_bytes(name));
        let cname = cstring(name, &path)?;
        if let Some(mode) = create {
            // SAFETY: live directory fd and NUL-terminated single component.
            if unsafe { libc::mkdirat(self.raw(), cname.as_ptr(), mode as libc::mode_t) } != 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::EEXIST) {
                    return Err(FsError::io("mkdir", &path, &error));
                }
            }
        }
        let fd = open_raw(
            self.raw(),
            &cname,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            0,
        )
        .map_err(|e| {
            // O_DIRECTORY | O_NOFOLLOW meeting a symlink or file: ENOTDIR.
            if e.raw_os_error() == Some(libc::ENOTDIR) {
                FsError::untrusted(&path, "symlinked or non-directory component")
            } else {
                FsError::io("open", &path, &e)
            }
        })?;
        let dir = Self {
            fd,
            path,
            setup_uid: self.setup_uid,
        };
        dir.check(&dir.stat()?, trust, Kind::Dir, &dir.path)?;
        Ok(dir)
    }
    /// Setup-only directory preparation with a checked creation mode.
    /// Existing modes are verified, never relaxed.
    pub fn prepare_setup_child(
        &self,
        cx: &crate::ctx::EffectContext,
        name: &FileName,
        mode: u32,
    ) -> Result<Self, FsError> {
        if cx.operation().class() != crate::ctx::EffectClass::Mutate
            || mode & !0o777 != 0
            || mode & 0o022 != 0
        {
            return Err(FsError::untrusted(
                self.path(),
                "setup directory requires Mutate and a safe mode",
            ));
        }
        self.prepare_child_mode(cx, name, mode)
    }
    /// Setup-only owner transfer. Runtime state roots and Store continue to
    /// accept only the effective operator; this handle cannot open a Store.
    pub fn open_setup(
        cx: &crate::ctx::EffectContext,
        home: &Path,
        uid: u32,
    ) -> Result<Self, FsError> {
        cx.require(crate::ctx::Capability::HostAuthority)
            .map_err(|_| FsError::HostAuthorityRequired)?;
        if cx.operation().class() != crate::ctx::EffectClass::Mutate
            || (operator_uid() != 0 && operator_uid() != uid)
        {
            return Err(FsError::untrusted(home, "setup owner authority required"));
        }
        let dir = Self::walk_with_setup(home, Trust::Operator, Some(uid))?;
        if dir.stat()?.st_uid != uid {
            return Err(FsError::untrusted(home, "setup home owner mismatch"));
        }
        Ok(dir)
    }
    /// Descriptor-relative descendants never canonicalize a mutable child.
    pub fn setup_child(&self, name: &FileName) -> Result<Option<Self>, FsError> {
        if self.setup_uid.is_none() {
            return Err(FsError::untrusted(self.path(), "setup handle required"));
        }
        self.child(name.as_str(), Trust::Operator, false)
    }
    pub fn setup_entries(&self, max: usize) -> Result<Vec<OsString>, FsError> {
        if self.setup_uid.is_none() {
            return Err(FsError::untrusted(self.path(), "setup handle required"));
        }
        self.list(max)
    }
    /// Transfer one positive, bounded file/directory identity; no recursion,
    /// symlink following, path-based chmod/chown, or unchecked hard links.
    pub fn adopt_setup_entry(
        &self,
        cx: &crate::ctx::EffectContext,
        name: &FileName,
        directory: bool,
        max: u64,
    ) -> Result<(), FsError> {
        cx.require(crate::ctx::Capability::HostAuthority)
            .map_err(|_| FsError::HostAuthorityRequired)?;
        if cx.operation().class() != crate::ctx::EffectClass::Mutate {
            return Err(FsError::untrusted(self.path(), "setup mutation required"));
        }
        let uid = self
            .setup_uid
            .ok_or_else(|| FsError::untrusted(self.path(), "setup handle required"))?;
        cx.operation()
            .check("setup ownership")
            .map_err(|e| FsError::untrusted(self.path(), format!("{e:?}")))?;
        self.require_local()?;
        let path = self.join(name.as_str());
        let cname = cstring(name.as_str().as_bytes(), &path)?;
        let fd = match open_raw(
            self.raw(),
            &cname,
            libc::O_RDONLY
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | if directory { libc::O_DIRECTORY } else { 0 },
            0,
        ) {
            Ok(fd) => fd,
            Err(e) if e.raw_os_error() == Some(libc::ENOENT) => return Ok(()),
            Err(e) => return Err(FsError::io("setup open", &path, &e)),
        };
        let stat = fstat(fd.as_raw_fd()).map_err(|e| FsError::io("setup stat", &path, &e))?;
        self.check(
            &stat,
            Trust::Operator,
            if directory { Kind::Dir } else { Kind::File },
            &path,
        )?;
        if !directory && (stat.st_size as u64 > max || stat.st_nlink as u64 != 1) {
            return Err(FsError::untrusted(&path, "unbounded or linked setup input"));
        }
        // SAFETY: verified live descriptor; selected operator UID and fixed safe mode.
        if unsafe { libc::fchown(fd.as_raw_fd(), uid, !0) } != 0
            || unsafe { libc::fchmod(fd.as_raw_fd(), if directory { 0o700 } else { 0o600 }) } != 0
        {
            return Err(FsError::io(
                "setup transfer",
                &path,
                &io::Error::last_os_error(),
            ));
        }
        Ok(())
    }
    fn check(
        &self,
        stat: &libc::stat,
        trust: Trust,
        kind: Kind,
        path: &Path,
    ) -> Result<(), FsError> {
        if let Some(uid) = self.setup_uid {
            if stat.st_uid != 0 && stat.st_uid != uid {
                return Err(FsError::untrusted(path, "foreign setup owner"));
            }
            // Preserve the normal type/mode/link policy after checking the
            // explicitly selected setup owner, without changing process UID.
            let mut owned = *stat;
            owned.st_uid = operator_uid();
            check(&owned, trust, kind, path)
        } else {
            check(stat, trust, kind, path)
        }
    }
    pub fn identity(&self) -> Result<DirectoryIdentity, FsError> {
        let stat = self.stat()?;
        Ok(DirectoryIdentity {
            device: stat.st_dev as u64,
            inode: stat.st_ino as u64,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Create/open a source directory below an already verified descriptor.
    /// Prepare and Mutate only; derived paths never follow a symlink.
    /// Read an operator-owned child relative to this retained directory.
    /// Never creates or repairs it, and never follows a symlink.
    pub fn observe_child(
        &self,
        cx: &crate::ctx::ObserveContext,
        name: &FileName,
    ) -> Result<Option<Self>, FsError> {
        cx.check("directory observation")
            .map_err(|e| FsError::untrusted(self.path(), format!("{e:?}")))?;
        self.child(name.as_str(), Trust::Operator, false)
    }
    /// Bounded, read-only entry names from this retained directory.
    pub fn observe_entries(
        &self,
        cx: &crate::ctx::ObserveContext,
        max: usize,
    ) -> Result<Vec<OsString>, FsError> {
        cx.check("directory inventory")
            .map_err(|e| FsError::untrusted(self.path(), format!("{e:?}")))?;
        self.list(max)
    }
    pub fn prepare_child(
        &self,
        cx: &crate::ctx::EffectContext,
        name: &FileName,
    ) -> Result<Self, FsError> {
        self.prepare_child_mode(cx, name, 0o755)
    }
    /// Prepare a source-owned directory whose contents must stay private.
    /// Existing operator-owned directories are tightened through the opened
    /// descriptor, never by following a pathname.
    pub fn prepare_private_child(
        &self,
        cx: &crate::ctx::EffectContext,
        name: &FileName,
    ) -> Result<Self, FsError> {
        let child = self.prepare_child_mode(cx, name, DIR_MODE)?;
        child.chmod(DIR_MODE)?;
        Ok(child)
    }
    /// Remove one prepared child tree below this verified directory. Prepare
    /// and Mutate only; descriptor-relative, bounded, and never follows a link.
    pub fn remove_prepared_tree(
        &self,
        cx: &crate::ctx::EffectContext,
        name: &FileName,
        depth: usize,
    ) -> Result<(), FsError> {
        cx.require(crate::ctx::Capability::SourceStore)
            .map_err(|_| {
                FsError::untrusted(&self.path, "source removal requires Prepare or Mutate")
            })?;
        cx.require(crate::ctx::Capability::HostAuthority)
            .map_err(|_| FsError::HostAuthorityRequired)?;
        cx.operation()
            .check("source removal")
            .map_err(|error| FsError::untrusted(&self.path, format!("{error}")))?;
        self.require_local()?;
        self.remove_tree(name, Trust::Operator, depth)?;
        self.sync()
    }
    fn prepare_child_mode(
        &self,
        cx: &crate::ctx::EffectContext,
        name: &FileName,
        mode: u32,
    ) -> Result<Self, FsError> {
        cx.require(crate::ctx::Capability::SourceStore)
            .map_err(|_| {
                FsError::untrusted(&self.path, "source directory requires Prepare or Mutate")
            })?;
        cx.require(crate::ctx::Capability::HostAuthority)
            .map_err(|_| FsError::HostAuthorityRequired)?;
        cx.operation()
            .check("source directory")
            .map_err(|error| FsError::untrusted(&self.path, format!("{error:?}")))?;
        self.require_local()?;
        crate::fault_point!(SourceDirOpen)
            .map_err(|error| FsError::io("source directory", &self.path, &error))?;
        let directory = self.descend(name.as_str().as_bytes(), Trust::Operator, Some(mode))?;
        crate::fault_point!(SourceDirSync)
            .map_err(|error| FsError::io("source directory sync", &self.path, &error))?;
        self.sync()?;
        Ok(directory)
    }
    pub(crate) fn raw(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
    pub(crate) fn stat(&self) -> Result<libc::stat, FsError> {
        fstat(self.raw()).map_err(|e| FsError::io("stat", &self.path, &e))
    }
    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
    /// Opens a file no-follow and checks it is a trusted regular file.
    /// `create` passes O_CREAT with the final mode; `Ok(None)` is absence.
    pub(crate) fn open_file(
        &self,
        name: &FileName,
        flags: i32,
        create: Option<u32>,
        trust: Trust,
    ) -> Result<Option<(File, libc::stat)>, FsError> {
        let path = self.join(name.as_str());
        let cname = cstring(name.as_str().as_bytes(), &path)?;
        let flags = flags
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if create.is_some() { libc::O_CREAT } else { 0 };
        // macOS can fail one of two concurrent O_CREAT opens of the same new
        // name with ENOENT although the directory exists; the name exists by
        // the retry. A directory that really vanished still fails.
        let mut attempts = if create.is_some() { 16 } else { 1 };
        let fd = loop {
            attempts -= 1;
            match open_raw(self.raw(), &cname, flags, create.unwrap_or(0)) {
                Ok(fd) => break fd,
                Err(error) if error.raw_os_error() == Some(libc::ENOENT) && create.is_none() => {
                    return Ok(None)
                }
                Err(error) if error.raw_os_error() == Some(libc::ENOENT) && attempts > 0 => {
                    std::thread::yield_now();
                }
                Err(error) => return Err(FsError::io("open", &path, &error)),
            }
        };
        let stat = fstat(fd.as_raw_fd()).map_err(|e| FsError::io("stat", &path, &e))?;
        self.check(&stat, trust, Kind::File, &path)?;
        Ok(Some((File::from(fd), stat)))
    }
    /// `lstat` of an entry; `None` when it does not exist.
    pub(crate) fn entry(&self, name: &FileName) -> Result<Option<libc::stat>, FsError> {
        let path = self.join(name.as_str());
        let cname = cstring(name.as_str().as_bytes(), &path)?;
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: live directory fd, NUL-terminated name, writable storage.
        let result = unsafe {
            libc::fstatat(
                self.raw(),
                cname.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if result != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ENOENT) {
                return Ok(None);
            }
            return Err(FsError::io("stat", &path, &error));
        }
        // SAFETY: fstatat succeeded and initialized the structure.
        Ok(Some(unsafe { stat.assume_init() }))
    }
    pub(crate) fn rename(&self, from: &FileName, to: &FileName) -> Result<(), FsError> {
        let path = self.join(to.as_str());
        let a = cstring(from.as_str().as_bytes(), &path)?;
        let b = cstring(to.as_str().as_bytes(), &path)?;
        // SAFETY: both names are NUL-terminated components of one live dir fd.
        if unsafe { libc::renameat(self.raw(), a.as_ptr(), self.raw(), b.as_ptr()) } != 0 {
            return Err(FsError::io("rename", &path, &io::Error::last_os_error()));
        }
        Ok(())
    }
    /// Rename between two already verified directories, without resolving paths.
    pub(crate) fn move_to(
        &self,
        from: &FileName,
        target: &Self,
        to: &FileName,
    ) -> Result<(), FsError> {
        let path = target.join(to.as_str());
        let a = cstring(from.as_str().as_bytes(), &path)?;
        let b = cstring(to.as_str().as_bytes(), &path)?;
        // SAFETY: live directory descriptors and validated single components.
        if unsafe { libc::renameat(self.raw(), a.as_ptr(), target.raw(), b.as_ptr()) } != 0 {
            return Err(FsError::io("rename", &path, &io::Error::last_os_error()));
        }
        Ok(())
    }
    /// Rename between two already verified directories only when `to` does not
    /// exist: `Ok(false)` means another entry already holds the name and
    /// nothing moved. Linux uses `renameat2(RENAME_NOREPLACE)`, macOS
    /// `renameatx_np(RENAME_EXCL)`; a filesystem or platform without an exact
    /// no-replace rename (`ENOTSUP`, `EINVAL`) refuses instead of falling back
    /// to a replace-in-place rename.
    pub(crate) fn move_to_exclusive(
        &self,
        from: &FileName,
        target: &Self,
        to: &FileName,
    ) -> Result<bool, FsError> {
        let path = target.join(to.as_str());
        let a = cstring(from.as_str().as_bytes(), &path)?;
        let b = cstring(to.as_str().as_bytes(), &path)?;
        #[cfg(any(target_os = "linux", target_os = "android"))]
        // SAFETY: live directory descriptors and validated single components.
        let result = unsafe {
            libc::renameat2(
                self.raw(),
                a.as_ptr(),
                target.raw(),
                b.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        // SAFETY: live directory descriptors and validated single components.
        let result = unsafe {
            libc::renameatx_np(
                self.raw(),
                a.as_ptr(),
                target.raw(),
                b.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        #[cfg(not(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "ios"
        )))]
        let result = {
            return Err(FsError::io(
                "rename",
                &path,
                &io::Error::from_raw_os_error(libc::ENOTSUP),
            ));
        };
        if result != 0 {
            let error = io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(libc::EEXIST | libc::ENOTEMPTY)) {
                return Ok(false);
            }
            return Err(FsError::io("rename", &path, &error));
        }
        Ok(true)
    }
    pub(crate) fn link(&self, from: &FileName, to: &FileName) -> Result<(), FsError> {
        let path = self.join(to.as_str());
        let a = cstring(from.as_str().as_bytes(), &path)?;
        let b = cstring(to.as_str().as_bytes(), &path)?;
        // SAFETY: both names are NUL-terminated components of one live dir
        // fd; flags 0 never follows a symlinked source.
        if unsafe { libc::linkat(self.raw(), a.as_ptr(), self.raw(), b.as_ptr(), 0) } != 0 {
            return Err(FsError::io("link", &path, &io::Error::last_os_error()));
        }
        Ok(())
    }
    /// Removes a file or (with `dir`) an empty directory. Absence is success.
    pub(crate) fn remove(&self, name: &FileName, dir: bool) -> Result<(), FsError> {
        let path = self.join(name.as_str());
        let cname = cstring(name.as_str().as_bytes(), &path)?;
        let flags = if dir { libc::AT_REMOVEDIR } else { 0 };
        // SAFETY: live directory fd and NUL-terminated single component.
        if unsafe { libc::unlinkat(self.raw(), cname.as_ptr(), flags) } != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ENOENT) {
                return Err(FsError::io("remove", &path, &error));
            }
        }
        Ok(())
    }
    pub(crate) fn chmod(&self, mode: u32) -> Result<(), FsError> {
        // SAFETY: live directory fd.
        if unsafe { libc::fchmod(self.raw(), mode as libc::mode_t) } != 0 {
            return Err(FsError::io(
                "chmod",
                &self.path,
                &io::Error::last_os_error(),
            ));
        }
        Ok(())
    }
    /// Removes `name` and everything under it, descriptor-relative, without
    /// following links. Bounded in depth and entries.
    pub(crate) fn remove_tree(
        &self,
        name: &FileName,
        trust: Trust,
        depth: usize,
    ) -> Result<(), FsError> {
        let Some(stat) = self.entry(name)? else {
            return Ok(());
        };
        if stat.st_mode as u32 & libc::S_IFMT as u32 != libc::S_IFDIR as u32 {
            return self.remove(name, false);
        }
        if depth == 0 {
            return Err(FsError::untrusted(
                &self.join(name.as_str()),
                "tree too deep",
            ));
        }
        let child = self
            .child(name.as_str(), trust, false)?
            .ok_or_else(|| FsError::untrusted(&self.join(name.as_str()), "vanished"))?;
        if trust == Trust::Private {
            // A tree an older build sealed read-only is repaired here: removal
            // needs write on the directory, never on the files in it.
            child.chmod(DIR_MODE)?;
        }
        for entry in child.list(1 << 16)? {
            let entry = entry
                .to_str()
                .and_then(|entry| FileName::new(entry).ok())
                .ok_or_else(|| FsError::untrusted(child.path(), "unexpected entry name"))?;
            child.remove_tree(&entry, trust, depth - 1)?;
        }
        self.remove(name, true)
    }
    pub(crate) fn sync(&self) -> Result<(), FsError> {
        // SAFETY: live directory fd.
        if unsafe { libc::fsync(self.raw()) } != 0 {
            return Err(FsError::io(
                "fsync",
                &self.path,
                &io::Error::last_os_error(),
            ));
        }
        Ok(())
    }
    /// Entry names, bounded; `.` and `..` excluded.
    pub(crate) fn list(&self, max: usize) -> Result<Vec<OsString>, FsError> {
        // SAFETY: dup of a live fd; fdopendir takes ownership of the copy.
        let copy = unsafe { libc::dup(self.raw()) };
        if copy < 0 {
            return Err(FsError::io("list", &self.path, &io::Error::last_os_error()));
        }
        // SAFETY: copy is a fresh directory fd owned by the stream from here.
        let stream = unsafe { libc::fdopendir(copy) };
        if stream.is_null() {
            let error = io::Error::last_os_error();
            // SAFETY: fdopendir failed, so copy is still ours to close.
            unsafe { libc::close(copy) };
            return Err(FsError::io("list", &self.path, &error));
        }
        // SAFETY: rewind so a reused description lists from the start.
        unsafe { libc::rewinddir(stream) };
        let mut names = Vec::new();
        let result = loop {
            // SAFETY: stream is a live DIR*; readdir returns null at the end.
            let entry = unsafe { libc::readdir(stream) };
            if entry.is_null() {
                break Ok(());
            }
            // SAFETY: d_name is a NUL-terminated array inside a live entry.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            if names.len() == max {
                break Err(FsError::untrusted(&self.path, "too many entries"));
            }
            names.push(OsString::from_vec(name.to_vec()));
        };
        // SAFETY: closes the stream and its fd exactly once.
        unsafe { libc::closedir(stream) };
        result.map(|()| names)
    }
    /// Refuses mutation on a network filesystem; Observe never calls this.
    pub(crate) fn require_local(&self) -> Result<(), FsError> {
        let fs = filesystem(self.raw()).map_err(|e| FsError::io("statfs", &self.path, &e))?;
        #[cfg(all(test, feature = "fault-injection"))]
        if crate::fault::network_fs() {
            return Err(FsError::UnsupportedFilesystem {
                path: self.path.clone(),
                fs: "nfs (injected)".into(),
            });
        }
        if fs.network {
            return Err(FsError::UnsupportedFilesystem {
                path: self.path.clone(),
                fs: fs.name,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Dir,
    File,
}
pub(crate) fn operator_uid() -> u32 {
    #[cfg(all(test, feature = "fault-injection"))]
    if let Some(uid) = crate::fault::operator_uid() {
        return uid;
    }
    // SAFETY: geteuid has no arguments or preconditions.
    unsafe { libc::geteuid() }
}
pub(crate) fn check(
    stat: &libc::stat,
    trust: Trust,
    kind: Kind,
    path: &Path,
) -> Result<(), FsError> {
    let mode = stat.st_mode as u32;
    let wanted = match kind {
        Kind::Dir => libc::S_IFDIR as u32,
        Kind::File => libc::S_IFREG as u32,
    };
    if mode & libc::S_IFMT as u32 != wanted {
        return Err(FsError::untrusted(
            path,
            match kind {
                Kind::Dir => "not a directory",
                Kind::File => "not a regular file",
            },
        ));
    }
    // An ancestor above the state root is only reopened no-follow.
    let operator = operator_uid();
    let owner_ok = match trust {
        Trust::Ancestor => true,
        Trust::Operator | Trust::Private => stat.st_uid == operator,
    };
    if !owner_ok {
        return Err(FsError::untrusted(path, "wrong owner"));
    }
    let mode_ok = match trust {
        Trust::Ancestor => true,
        Trust::Operator => mode & 0o022 == 0,
        Trust::Private => mode & 0o077 == 0,
    };
    if !mode_ok {
        return Err(FsError::untrusted(path, OPEN_MODE_REASON));
    }
    if kind == Kind::File && trust == Trust::Private && stat.st_nlink as u64 > 1 {
        return Err(FsError::untrusted(path, "hard-linked file"));
    }
    Ok(())
}

fn cstring(bytes: &[u8], path: &Path) -> Result<CString, FsError> {
    CString::new(bytes).map_err(|_| FsError::untrusted(path, "NUL in path"))
}
pub(crate) fn open_raw(dir: RawFd, name: &CStr, flags: i32, mode: u32) -> io::Result<OwnedFd> {
    // SAFETY: name is NUL-terminated; dir is a live fd or AT_FDCWD; the mode
    // argument is read only when flags include O_CREAT.
    let fd = unsafe {
        libc::openat(
            dir,
            name.as_ptr(),
            flags | libc::O_CLOEXEC,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openat returned a fresh valid fd; ownership transfers once.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}
pub(crate) fn fstat(fd: RawFd) -> io::Result<libc::stat> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: live fd and aligned writable storage for the full structure.
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful fstat initialized the structure.
    Ok(unsafe { stat.assume_init() })
}
pub(crate) fn fsync(file: &File) -> io::Result<()> {
    // SAFETY: live fd owned by file.
    if unsafe { libc::fsync(file.as_raw_fd()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub(crate) fn fchmod(file: &File, mode: u32) -> io::Result<()> {
    // SAFETY: live fd owned by file.
    if unsafe { libc::fchmod(file.as_raw_fd(), mode as libc::mode_t) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
/// `flock` without blocking. `Ok(false)` means another description holds it.
pub(crate) fn try_flock(file: &File, op: i32) -> io::Result<bool> {
    // SAFETY: live fd owned by file; LOCK_NB never blocks.
    if unsafe { libc::flock(file.as_raw_fd(), op | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Ok(false)
    } else {
        Err(error)
    }
}
pub(crate) fn unlock(file: &File) {
    // SAFETY: live fd owned by file. Failure leaves the lock to close().
    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
}
/// Replaces a file's body in place: `pwrite` at 0, then `ftruncate`.
pub(crate) fn rewrite(file: &File, bytes: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(bytes, 0)?;
    file.set_len(bytes.len() as u64)
}
/// Reads at most `max` bytes from offset 0; `None` when longer.
pub(crate) fn read_bounded(file: &File, max: u64) -> io::Result<Option<Vec<u8>>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    let mut handle = file;
    std::io::Seek::seek(&mut handle, std::io::SeekFrom::Start(0))?;
    handle.take(max + 1).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= max).then_some(bytes))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FsKind {
    pub name: String,
    pub network: bool,
}
#[cfg(target_os = "macos")]
fn filesystem(fd: RawFd) -> io::Result<FsKind> {
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: live fd and aligned writable storage for the full structure.
    if unsafe { libc::fstatfs(fd, stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstatfs succeeded and initialized the structure.
    let stat = unsafe { stat.assume_init() };
    // SAFETY: f_fstypename is a NUL-terminated fixed array.
    let name = unsafe { CStr::from_ptr(stat.f_fstypename.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    let local = stat.f_flags & libc::MNT_LOCAL as u32 != 0;
    Ok(FsKind {
        network: network_name(&name) || !local,
        name,
    })
}
#[cfg(target_os = "linux")]
fn filesystem(fd: RawFd) -> io::Result<FsKind> {
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: live fd and aligned writable storage for the full structure.
    if unsafe { libc::fstatfs(fd, stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstatfs succeeded and initialized the structure.
    let stat = unsafe { stat.assume_init() };
    let magic = (stat.f_type as u64) & 0xffff_ffff;
    Ok(FsKind {
        name: format!("{magic:#x}"),
        network: network_magic(magic),
    })
}
/// macOS `f_fstypename` values for remote and userspace filesystems.
#[cfg(any(target_os = "macos", all(test, feature = "fault-injection")))]
pub(crate) fn network_name(name: &str) -> bool {
    matches!(
        name,
        "nfs" | "smbfs" | "cifs" | "webdav" | "afpfs" | "ftp" | "sshfs"
    ) || name.contains("fuse")
}
/// Linux `statfs.f_type` magics for NFS, SMB/CIFS, FUSE (sshfs), AFS, Coda,
/// NCP, 9P and Ceph, from linux/magic.h and the filesystems' sources.
#[cfg(any(target_os = "linux", all(test, feature = "fault-injection")))]
pub(crate) fn network_magic(magic: u64) -> bool {
    matches!(
        magic,
        0x6969          // NFS
            | 0x517b    // SMB
            | 0xff53_4d42 // CIFS
            | 0xfe53_4d42 // SMB2
            | 0x6573_5546 // FUSE
            | 0x5346_414f // AFS
            | 0x7375_7245 // Coda
            | 0x564c    // NCP
            | 0x0102_1997 // 9P
            | 0x00c3_6400 // Ceph
    )
}

#[cfg(all(test, feature = "fault-injection"))]
#[test]
fn source_directory_refuses_wrong_class_guest_symlink_and_faults() {
    use crate::ctx::*;
    use crate::fault::{self, Fault, Point};
    use std::time::{Duration, Instant};
    let tmp = tempfile::tempdir().unwrap();
    let dir = DirFd::open(tmp.path()).unwrap();
    let name = FileName::new("sources").unwrap();
    let context = |class, placement| {
        OperationContext::new(
            Deadline::At(Instant::now() + Duration::from_secs(5)),
            CancelToken::default(),
            Limits {
                capture_tail: 1024,
                log_rotate: 4096,
                log_keep: 1,
                line_max: 1024,
                queue_bytes: 4096,
                probe_concurrency: 1,
            },
            class,
            placement,
        )
        .effects()
        .unwrap()
    };
    for (class, placement) in [
        (EffectClass::RefreshDerived, Placement::Host),
        (EffectClass::Prepare, Placement::Guest),
    ] {
        assert!(dir
            .prepare_child(&context(class, placement), &name)
            .is_err());
        assert!(!tmp.path().join("sources").exists());
    }
    let cx = context(EffectClass::Prepare, Placement::Host);
    for point in [Point::SourceDirOpen, Point::SourceDirSync] {
        fault::arm_point(point, Fault::Enospc);
        assert!(dir.prepare_child(&cx, &name).is_err());
        fault::arm(None);
        assert!(dir.prepare_child(&cx, &name).is_ok());
    }
    let setup = FileName::new("setup").unwrap();
    assert!(dir.prepare_setup_child(&cx, &setup, 0o700).is_err());
    let mutate = context(EffectClass::Mutate, Placement::Host);
    assert!(dir.prepare_setup_child(&mutate, &setup, 0o777).is_err());
    assert!(!tmp.path().join("setup").exists());
    for point in [Point::SourceDirOpen, Point::SourceDirSync] {
        fault::arm_point(point, Fault::Enospc);
        assert!(dir.prepare_setup_child(&mutate, &setup, 0o700).is_err());
        fault::arm(None);
        assert!(dir.prepare_setup_child(&mutate, &setup, 0o700).is_ok());
    }
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(tmp.path().join("setup"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let foreign = tmp.path().join("foreign");
    std::fs::create_dir(&foreign).unwrap();
    std::os::unix::fs::symlink(&foreign, tmp.path().join("link")).unwrap();
    assert!(dir
        .prepare_child(&cx, &FileName::new("link").unwrap())
        .is_err());
    assert_eq!(std::fs::read_dir(foreign).unwrap().count(), 0);
}
