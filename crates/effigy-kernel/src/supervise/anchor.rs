//! Same-image group anchor. It announces its ready PID before go, and exits on
//! SIGTERM or when settlement retires it.
use super::*;
use std::ffi::CString;
use std::os::fd::OwnedFd;

pub(crate) struct Image {
    pub(crate) path: CString,
    pub(crate) fd: Option<OwnedFd>,
}
impl Image {
    pub(crate) fn prepare(executable: &Executable) -> Result<Self, RunError> {
        #[cfg(target_os = "linux")]
        let fd = Some(crate::custody::spawn::high(
            std::fs::File::open(&executable.path)?.into(),
        )?);
        #[cfg(target_os = "macos")]
        let fd = None;
        let path =
            CString::new(executable.path.as_os_str().as_bytes()).map_err(io::Error::other)?;
        Ok(Self { path, fd })
    }
}

pub(super) fn entry() -> Result<(), RunError> {
    if std::env::var("EFFIGY_CUSTODY_NONCE")
        .unwrap_or_default()
        .is_empty()
    {
        return Err(RunError::broken("anchor nonce missing"));
    }
    // SAFETY: supported signals and SIG_IGN/SIG_DFL; this single-threaded helper
    // never forwards or handles workload signals. INT and HUP stay ignored
    // because a terminal delivers them to the whole foreground group, where the
    // anchor is only the group's evidence. TERM is the cooperative exit, so its
    // default action is restored even when the binary installed a signal router
    // before dispatching here; settlement still stops it last.
    unsafe {
        for signal in [libc::SIGHUP, libc::SIGINT] {
            libc::signal(signal, libc::SIG_IGN);
        }
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
        // A blocked TERM inherited across exec would otherwise survive the
        // default action, so the anchor unblocks it here.
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
        // spawn::Blocked supplied only 0..=4 (plus Linux image fd 5).
        // The main-only helper dispatch and signal router open no descriptors.
        for fd in [0, 1, 2, 5] {
            libc::close(fd);
        }
    }
    let mut go = std::fs::File::from(private_dup(3)?);
    let mut ready = std::fs::File::from(private_dup(4)?);
    // SAFETY: the protocol's original descriptors are no longer needed; only
    // the two validated, freshly owned duplicates above remain in use.
    unsafe {
        libc::close(3);
        libc::close(4);
    }
    ready.write_all(&std::process::id().to_ne_bytes())?;
    drop(ready);
    let mut byte = [0u8];
    go.read_exact(&mut byte)?; // pre-go owner death closes this pipe
    drop(go);
    if byte != [1] {
        return Err(RunError::broken("invalid anchor go"));
    }
    loop {
        // SAFETY: pause has no arguments. INT and HUP stay ignored, and TERM's
        // default action ends the process; no fds remain either way.
        unsafe {
            libc::pause();
        }
    }
}
