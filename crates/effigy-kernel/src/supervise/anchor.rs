//! Same-image group anchor. It announces its ready PID before go.
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
        return Err(RunError::Invalid("anchor nonce missing".into()));
    }
    // SAFETY: supported signals and SIG_IGN; this single-threaded helper never
    // forwards or handles workload signals. Its only exit after go is KILL.
    unsafe {
        for signal in [libc::SIGHUP, libc::SIGINT, libc::SIGTERM] {
            libc::signal(signal, libc::SIG_IGN);
        }
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
        return Err(RunError::Invalid("invalid anchor go".into()));
    }
    loop {
        // SAFETY: pause has no arguments; ignored signals cannot end the
        // anchor, and a caught signal merely causes another wait. No fds remain.
        unsafe {
            libc::pause();
        }
    }
}
