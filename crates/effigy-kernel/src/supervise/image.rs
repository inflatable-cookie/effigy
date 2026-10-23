//! Re-exec a replaced startup image without copying its data (I-SV4).
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
            io::Error::new(
                error.kind(),
                format!("startup image pin refused (no copy fallback): {error}"),
            ),
        )
    })
}

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
        return Err(io::Error::last_os_error());
    }
    // Install cleanup immediately after clone success, including capture and
    // injected failures. Keep the clone until the helper acknowledges.
    let mut pin = PrivateImage {
        dir: root.dir(),
        name: Some(name),
        executable: executable.clone(),
    };
    crate::fault_point!(ImageCloned)?;
    pin.executable =
        Executable::capture(&root.dir().join(cname.to_str().map_err(io::Error::other)?))?;
    if pin.executable.size != executable.size {
        return Err(io::Error::other("startup clone size changed"));
    }
    Ok(pin)
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
