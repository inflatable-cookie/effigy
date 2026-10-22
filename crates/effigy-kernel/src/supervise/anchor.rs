//! Same-image group anchor. Startup verification precedes its ready PID frame.
use super::*;
use std::ffi::CString;
use std::os::fd::OwnedFd;

pub(crate) struct Image<'a> {
    _pin: Option<PrivateImage<'a>>,
    pub(crate) path: CString,
    pub(crate) metadata: CString,
    pub(crate) fd: Option<OwnedFd>,
}
impl<'a> Image<'a> {
    pub(crate) fn prepare(root: &'a StateRoot, executable: &Executable) -> Result<Self, RunError> {
        let pin = private_image(root, executable)?;
        let expected = pin
            .as_ref()
            .map_or(executable, |pin| &pin.executable)
            .clone();
        Self::from_image(pin, &expected)
    }

    /// Foreground anchors use the verified image without creating a runtime
    /// pin. Replacement is refused before a workload can cross its barrier.
    pub(crate) fn memory(executable: &Executable) -> Result<Self, RunError> {
        Self::from_image(None, executable)
    }

    fn from_image(pin: Option<PrivateImage<'a>>, expected: &Executable) -> Result<Self, RunError> {
        expected.verify()?;
        #[cfg(target_os = "linux")]
        let fd = Some(crate::custody::spawn::high(
            match (&expected.image, expected.exec_fd) {
                (Some(image), _) => image.try_clone()?,
                (_, Some(fd)) => std::fs::File::from(private_dup(fd)?),
                _ => open_executable(&expected.path)?,
            }
            .into(),
        )?);
        #[cfg(target_os = "macos")]
        let fd = None;
        let expected = expected.clone();
        #[cfg(target_os = "linux")]
        let expected = {
            let mut expected = expected;
            expected.exec_fd = Some(5);
            expected
        };
        let path = CString::new(expected.path.as_os_str().as_bytes()).map_err(io::Error::other)?;
        let metadata = CString::new(serde_json::to_vec(&expected)?).map_err(io::Error::other)?;
        Ok(Self {
            _pin: pin,
            path,
            metadata,
            fd,
        })
    }
}

pub(super) fn entry() -> Result<(), RunError> {
    let expected: Executable = serde_json::from_str(
        &std::env::args()
            .nth(2)
            .ok_or_else(|| io::Error::other("anchor image frame missing"))?,
    )?;
    expected.verify()?;
    if std::env::var("EFFIGY_CUSTODY_NONCE")
        .map_err(io::Error::other)?
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
        // Image verification has finished and closed its temporary fds; the
        // main-only helper dispatch and signal router open no descriptors.
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
