//! Re-exec a replaced startup image (I-SV4); a same-volume pin copies no data.
use super::*;

pub(super) struct PrivateImage<'a> {
    dir: &'a DirFd,
    name: Option<FileName>,
    pub(super) executable: Executable,
}
impl Drop for PrivateImage<'_> {
    fn drop(&mut self) {
        if let Some(name) = &self.name {
            let _ = self.dir.remove(name, false);
        }
    }
}

pub(super) fn private_image<'a>(
    root: &'a StateRoot,
    executable: &Executable,
) -> Result<Option<PrivateImage<'a>>, RunError> {
    let Some(image) = executable.replaced_image() else {
        return Ok(None);
    };
    let prepare = || -> io::Result<PrivateImage<'a>> {
        crate::fault_point!(ImageBeforePin)?;
        let before = image.metadata()?;
        let pin = pin_image(root, executable, image)?;
        crate::fault_point!(ImageAfterPin)?;
        let after = image.metadata()?;
        // Unlink may change ctime before pinning; no identity field may change
        // during pinning, and content must still match the startup snapshot.
        if executable.replaced_image().is_none()
            || (
                before.dev(),
                before.ino(),
                before.len(),
                before.mtime(),
                before.mtime_nsec(),
                before.ctime(),
                before.ctime_nsec(),
            ) != (
                after.dev(),
                after.ino(),
                after.len(),
                after.mtime(),
                after.mtime_nsec(),
                after.ctime(),
                after.ctime_nsec(),
            )
        {
            return Err(io::Error::other("startup image changed while pinned"));
        }
        pin.executable.verify()?;
        crate::fault_point!(ImageVerified)?;
        Ok(pin)
    };
    prepare().map(Some).map_err(|error| {
        helper_not_started(
            SENTINEL,
            io::Error::new(error.kind(), format!("startup image pin refused: {error}")),
        )
    })
}

/// Pin the running image into the state root.
///
/// An APFS clone is preferred: it copies no data and is the running image's
/// content by construction. It only exists within one filesystem, so an
/// operator whose checkout or `[scratch]` state root is on another volume —
/// a project on an external drive, a build disk — cannot be cloned into. The
/// kernel refuses that with `EXDEV`, and a filesystem without cloning support
/// refuses with `ENOTSUP`; both fall back to a verified copy. The copy is read
/// back against the held image, flushed, and made read-only, so the pin is
/// exactly the running image and cannot change afterwards. Nothing else falls
/// back: a resource error (space, permissions, descriptors) still refuses.
#[cfg(target_os = "macos")]
fn pin_image<'a>(
    root: &'a StateRoot,
    executable: &Executable,
    image: &std::fs::File,
) -> io::Result<PrivateImage<'a>> {
    let name = FileName::new(&format!("exe-{}-{}", std::process::id(), nonce()))
        .map_err(io::Error::other)?;
    let cname = std::ffi::CString::new(name.as_str()).map_err(io::Error::other)?;
    crate::fault_point!(ImageClone)?;
    // SAFETY: both descriptors are live for this call, and cname owns the
    // initialized, NUL-terminated single-component destination. Flags are 0.
    if unsafe { libc::fclonefileat(image.as_raw_fd(), root.dir().raw(), cname.as_ptr(), 0) } != 0 {
        let error = io::Error::last_os_error();
        if !clone_capacity_refusal(&error) {
            return Err(error);
        }
        copy_image(root, &name, image)?;
    }
    // Install cleanup immediately after the pin exists, including capture and
    // injected failures. Keep the pin until the helper acknowledges.
    let mut pin = PrivateImage {
        dir: root.dir(),
        name: Some(name),
        executable: executable.clone(),
    };
    crate::fault_point!(ImageCloned)?;
    pin.executable =
        Executable::capture(&root.dir().join(cname.to_str().map_err(io::Error::other)?))?;
    if pin.executable.size != executable.size {
        return Err(io::Error::other("startup pin size changed"));
    }
    Ok(pin)
}

/// A clone the destination filesystem cannot hold, as opposed to a failure to
/// create the pin at all.
#[cfg(target_os = "macos")]
fn clone_capacity_refusal(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code)
            if code == libc::EXDEV || code == libc::ENOTSUP || code == libc::EOPNOTSUPP
    )
}

/// Copy the held image into the state root and prove the copy is that image.
///
/// The bytes come from the descriptor the run captured at startup, never from
/// the path, which a rebuild may already have replaced. After the copy is
/// flushed it is compared byte for byte with the held image, then made
/// read-only: the pin cannot change while the helper is deciding to exec it.
/// A refused or failed copy leaves no partial pin behind.
#[cfg(target_os = "macos")]
fn copy_image(root: &StateRoot, name: &FileName, image: &std::fs::File) -> io::Result<()> {
    let copy = copy_image_inner(root, name, image);
    if copy.is_err() {
        // The handle that failed is gone; the name is this process's own.
        let _ = root.dir().remove(name, false);
    }
    copy
}

#[cfg(target_os = "macos")]
fn copy_image_inner(root: &StateRoot, name: &FileName, image: &std::fs::File) -> io::Result<()> {
    use std::os::unix::fs::FileExt as _;
    let source = image.metadata()?;
    if !source.is_file() {
        return Err(io::Error::other("startup image is not a regular file"));
    }
    let (destination, _) = root
        .dir()
        .open_file(
            name,
            libc::O_RDWR | libc::O_TRUNC,
            Some(0o600),
            Trust::Private,
        )
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::other("startup image copy was not created"))?;
    let mut buffer = vec![0u8; 1 << 20];
    let mut copied = 0u64;
    loop {
        let read = image.read_at(&mut buffer, copied)?;
        if read == 0 {
            break;
        }
        destination.write_all_at(&buffer[..read], copied)?;
        copied += read as u64;
    }
    if copied != source.len() {
        return Err(io::Error::other("startup image copy is short"));
    }
    crate::fs::fsync(&destination)?;
    let mut held = vec![0u8; 1 << 20];
    let mut pinned = vec![0u8; 1 << 20];
    let mut offset = 0u64;
    loop {
        let read = image.read_at(&mut held, offset)?;
        if read == 0 {
            break;
        }
        destination.read_exact_at(&mut pinned[..read], offset)?;
        if held[..read] != pinned[..read] {
            return Err(io::Error::other(
                "startup image copy differs from the running image",
            ));
        }
        offset += read as u64;
    }
    if offset != copied {
        return Err(io::Error::other("startup image changed while copied"));
    }
    crate::fs::fchmod(&destination, 0o500)?;
    crate::fs::fsync(&destination)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn pin_image<'a>(
    root: &'a StateRoot,
    executable: &Executable,
    image: &std::fs::File,
) -> io::Result<PrivateImage<'a>> {
    crate::fault_point!(ImageFd)?;
    let mut held = fd_executable(&executable.path, image.as_raw_fd())?;
    held.exec_fd = Some(
        held.image
            .as_ref()
            .ok_or_else(|| io::Error::other("held image missing"))?
            .as_raw_fd(),
    );
    Ok(PrivateImage {
        dir: root.dir(),
        name: None,
        executable: held,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn pin_image<'a>(
    _: &'a StateRoot,
    _: &Executable,
    _: &std::fs::File,
) -> io::Result<PrivateImage<'a>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no zero-copy startup image execution on this platform",
    ))
}

#[cfg(target_os = "linux")]
pub(super) fn fd_executable(path: &Path, fd: RawFd) -> io::Result<Executable> {
    use std::os::fd::FromRawFd;
    if fd < 3 {
        return Err(io::Error::other("invalid startup image descriptor"));
    }
    // SAFETY: fcntl validates the integer descriptor and returns a new owned,
    // CLOEXEC fd, or -1. No ownership of the supplied descriptor is assumed.
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if duplicate < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: duplicate is a newly owned fd from the successful fcntl above.
    let image = unsafe { std::fs::File::from_raw_fd(duplicate) };
    let metadata = image.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::other("binary is not a regular file"));
    }
    Ok(Executable {
        path: path.into(),
        device: metadata.dev(),
        inode: metadata.ino(),
        size: metadata.len(),
        modified: (metadata.mtime(), metadata.mtime_nsec()),
        changed: (metadata.ctime(), metadata.ctime_nsec()),
        image: Some(std::sync::Arc::new(image)),
        exec_fd: None,
    })
}

#[cfg(target_os = "linux")]
pub(super) fn exec_held(
    command: &mut Command,
    executable: &Executable,
    fd: RawFd,
    entry: &str,
) -> io::Result<()> {
    // Keep the parent's descriptor alive as long as Command, even if the pin
    // is dropped before spawn. All allocation happens before fork.
    let image = executable
        .image
        .clone()
        .ok_or_else(|| io::Error::other("held image missing"))?;
    let arg0 =
        std::ffi::CString::new(executable.path.as_os_str().as_bytes()).map_err(io::Error::other)?;
    let arg1 = std::ffi::CString::new(entry).map_err(io::Error::other)?;
    // `helper` clears the ambient environment. Snapshot the explicit entries
    // only after the caller installed its capability marker and launch inputs.
    let env: Vec<std::ffi::CString> = command
        .get_envs()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .map(|(key, value)| {
            let mut entry = key.as_bytes().to_vec();
            entry.push(b'=');
            entry.extend_from_slice(value.as_bytes());
            std::ffi::CString::new(entry).map_err(io::Error::other)
        })
        .collect::<io::Result<_>>()?;
    const ENV_SLOTS: usize = 4096;
    if env.len() >= ENV_SLOTS {
        return Err(io::Error::other("helper environment exceeds 4095 entries"));
    }
    crate::fault_point!(ImageExec)?;
    // SAFETY: after fork only async-signal-safe fcntl and execveat run. The
    // closure owns the live image and both C strings; argv/envp are terminated
    // stack arrays of valid pointers. No Rust allocation or lock is used.
    unsafe {
        command.pre_exec(move || {
            let _keep_alive = &image;
            // Scripts need this fd in their interpreter; the sentinel also
            // verifies this exact inherited descriptor before acknowledging.
            if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            let argv = [arg0.as_ptr(), arg1.as_ptr(), std::ptr::null()];
            // Bounded stack storage avoids allocating in the forked child.
            // Every CString and its terminator remain owned by this closure.
            let mut envp = [std::ptr::null(); ENV_SLOTS];
            for (slot, entry) in envp.iter_mut().zip(&env) {
                *slot = entry.as_ptr();
            }
            libc::syscall(
                libc::SYS_execveat,
                fd,
                c"".as_ptr(),
                argv.as_ptr(),
                envp.as_ptr(),
                libc::AT_EMPTY_PATH,
            );
            Err(io::Error::last_os_error())
        });
    }
    Ok(())
}
