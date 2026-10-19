//! Safe files (K13): descriptor-relative, no-follow, owner- and mode-checked,
//! size-bounded opens, and the local-filesystem check.
// libc mode, nlink and statfs widths differ between Linux and macOS, so some
// casts are redundant on one platform and required on the other.
#![allow(clippy::unnecessary_cast)]

use serde::Serialize;
use std::ffi::{CStr, CString, OsString};
use std::fmt;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

pub(crate) const DIR_MODE: u32 = 0o700;
pub(crate) const FILE_MODE: u32 = 0o600;

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
}
impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HostAuthorityRequired => f.write_str("host authority required"),
            Self::Untrusted { path, reason } => {
                write!(f, "untrusted {}: {reason}", path.display())
            }
            Self::UnsupportedFilesystem { path, fs } => {
                write!(f, "{} is on a network filesystem ({fs})", path.display())
            }
            Self::Io(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for FsError {}

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
    /// Above the trusted root: owned by root or the operator, and not
    /// writable by others unless sticky.
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
}
impl DirFd {
    /// Opens a user directory. The operator's own path spelling (for example
    /// macOS `/var` → `/private/var`) is resolved first; every component is
    /// then reopened no-follow from `/` and owner-checked.
    pub fn open(path: &Path) -> Result<Self, FsError> {
        Self::walk(path, Trust::Operator)
    }
    pub(crate) fn walk(path: &Path, last: Trust) -> Result<Self, FsError> {
        if !path.is_absolute() {
            return Err(FsError::untrusted(path, "relative path"));
        }
        let real = std::fs::canonicalize(path).map_err(|e| FsError::io("resolve", path, &e))?;
        let mut dir = Self {
            fd: open_raw(libc::AT_FDCWD, c"/", libc::O_RDONLY | libc::O_DIRECTORY, 0)
                .map_err(|e| FsError::io("open", Path::new("/"), &e))?,
            path: PathBuf::from("/"),
        };
        check(&dir.stat()?, Trust::Ancestor, Kind::Dir, &dir.path)?;
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
        let dir = Self { fd, path };
        check(&dir.stat()?, trust, Kind::Dir, &dir.path)?;
        Ok(dir)
    }
    pub fn path(&self) -> &Path {
        &self.path
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
        check(&stat, trust, Kind::File, &path)?;
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
    pub(crate) fn remove_tree(&self, name: &FileName, depth: usize) -> Result<(), FsError> {
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
            .child(name.as_str(), Trust::Private, false)?
            .ok_or_else(|| FsError::untrusted(&self.join(name.as_str()), "vanished"))?;
        // Published objects are read-only; removal needs write on the dir.
        child.chmod(DIR_MODE)?;
        for entry in child.list(1 << 16)? {
            let entry = entry
                .to_str()
                .and_then(|entry| FileName::new(entry).ok())
                .ok_or_else(|| FsError::untrusted(child.path(), "unexpected entry name"))?;
            child.remove_tree(&entry, depth - 1)?;
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
    let operator = operator_uid();
    let owner_ok = match trust {
        Trust::Ancestor => stat.st_uid == 0 || stat.st_uid == operator,
        Trust::Operator | Trust::Private => stat.st_uid == operator,
    };
    if !owner_ok {
        return Err(FsError::untrusted(path, "wrong owner"));
    }
    let sticky = mode & libc::S_ISVTX as u32 != 0;
    let mode_ok = match trust {
        Trust::Ancestor => mode & 0o022 == 0 || (sticky && kind == Kind::Dir),
        Trust::Operator => mode & 0o022 == 0,
        Trust::Private => mode & 0o077 == 0,
    };
    if !mode_ok {
        return Err(FsError::untrusted(path, "group- or other-accessible mode"));
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
