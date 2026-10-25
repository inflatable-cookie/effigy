//! Anonymous cleanup pins for foreground descendant custody.
//!
//! The owner keeps the writing endpoint. Every descendant workload holds a
//! write-only copy at [`CLOSURE_FD`], so the run's memory guard sees EOF only
//! after the last descendant has gone. This is custody of the run's own
//! descendants, not an authority any child can use or renew.
use std::io;
use std::os::fd::AsRawFd;
use std::sync::Arc;

/// The reserved slot a workload receives its write-only cleanup pin at.
pub(crate) const CLOSURE_FD: i32 = 10;

#[derive(Debug, Clone)]
pub(crate) struct ClosurePin {
    writer: Arc<std::fs::File>,
}
impl ClosurePin {
    pub(crate) fn new() -> io::Result<(std::fs::File, Self)> {
        // The standard pipe keeps both original handles close-on-exec;
        // only explicit duplicates can reach a workload or guard.
        let (reader, writer) = std::io::pipe()?;
        // High CLOEXEC copies never collide with helper stdio/error pipes.
        let reader = std::fs::File::from(crate::gate::duplicate_custody(&reader)?);
        let writer = std::fs::File::from(crate::gate::duplicate_custody(&writer)?);
        pipe_identity(&writer, libc::O_WRONLY)?;
        Ok((
            reader,
            Self {
                writer: Arc::new(writer),
            },
        ))
    }
    pub(crate) fn custody_fd(&self) -> io::Result<std::os::fd::OwnedFd> {
        crate::gate::duplicate_custody(self.writer.as_ref())
    }
}

/// A custody pin is a write-only anonymous pipe owned by the operator.
pub(crate) fn pipe_identity(file: &std::fs::File, access: i32) -> io::Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let metadata = file.metadata()?;
    // SAFETY: file retains the descriptor across this read-only fcntl.
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    if !metadata.file_type().is_fifo()
        || metadata.uid() != crate::fs::operator_uid()
        || flags & libc::O_ACCMODE != access
    {
        return Err(refused("untrusted custody pipe or access mode"));
    }
    Ok(())
}

pub(crate) fn refused(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custody_pipe_pins_close_only_after_the_last_writer_and_check_access_mode() {
        use std::io::Read;
        let (mut watch, pin) = ClosurePin::new().expect("anonymous custody pipe");
        assert!(pipe_identity(&watch, libc::O_RDONLY).is_ok());
        assert!(pipe_identity(&watch, libc::O_WRONLY).is_err());
        assert!(pipe_identity(pin.writer.as_ref(), libc::O_RDONLY).is_err());
        let sibling = pin.clone();
        // SAFETY: watch owns this live descriptor across both fcntl calls.
        let flags = unsafe { libc::fcntl(watch.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0);
        // SAFETY: only O_NONBLOCK changes on this test's own pipe description.
        assert_eq!(
            unsafe { libc::fcntl(watch.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        drop(pin);
        assert_eq!(
            watch
                .read(&mut [0])
                .expect_err("sibling still pins cleanup")
                .kind(),
            io::ErrorKind::WouldBlock
        );
        drop(sibling);
        assert_eq!(watch.read(&mut [0]).expect("all writers closed"), 0);
    }

    #[test]
    fn an_ordinary_file_is_never_a_custody_pin() {
        let fixture = tempfile::tempdir().unwrap();
        let file = std::fs::File::create(fixture.path().join("ordinary")).unwrap();
        assert!(pipe_identity(&file, libc::O_WRONLY).is_err());
        let (_watch, pin) = ClosurePin::new().unwrap();
        let first = pin.custody_fd().unwrap();
        let second = pin.custody_fd().unwrap();
        assert_ne!(first.as_raw_fd(), second.as_raw_fd());
        assert!(first.as_raw_fd() >= 128 && second.as_raw_fd() >= 128);
    }
}
