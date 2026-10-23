//! Socket names relative to a verified directory, without process-wide chdir.
use crate::fs::{DirFd, FileName};
use std::io;
use std::path::Path;

/// The directory descriptor remains live through the operation. On Darwin a
/// fresh joined thread owns its own cwd; it cannot outlive the owner process
/// or alter the caller's thread/process cwd. No pathname alias is published.
pub(crate) fn in_directory<T: Send>(
    directory: &DirFd,
    name: &FileName,
    operation: impl FnOnce(&Path) -> T + Send,
) -> io::Result<T> {
    #[cfg(target_os = "macos")]
    {
        // Apple's operation contract and C signature, pinned together:
        // libpthread 42d026df5b07825070f60134b980a1ec2552dfee,
        // private/pthread/private.h:65-87 and src/pthread_cwd.c:30-34.
        // This is a dedicated pthread, never a reusable dispatch worker.
        unsafe extern "C" {
            fn pthread_fchdir_np(fd: libc::c_int) -> libc::c_int;
        }
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("effigy-socket".into())
                .spawn_scoped(scope, || {
                    // SAFETY: directory borrows an open verified directory fd for
                    // the entire joined call. The C API consumes only this integer;
                    // no pointers, buffers or ownership cross the boundary. It sets
                    // only this fresh thread's cwd and returns -1/errno on failure.
                    if unsafe { pthread_fchdir_np(directory.raw()) } != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(operation(Path::new(name.as_str())))
                })?
                .join()
                .map_err(|_| io::Error::other("socket directory thread panicked"))?
        })
    }
    #[cfg(target_os = "linux")]
    {
        // procfs follows this process's retained directory description; it
        // does not resolve the mutable original pathname. The resulting name
        // is bounded by the fd and one validated socket component.
        let path = std::path::PathBuf::from(format!("/proc/self/fd/{}", directory.raw()))
            .join(name.as_str());
        Ok(operation(&path))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Ok(operation(&directory.path().join(name.as_str())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::{UnixListener, UnixStream};

    #[test]
    fn long_socket_directories_are_independent_and_never_change_caller_cwd() {
        let fixture = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::thread::scope(|scope| {
            for index in 0..8 {
                let path = fixture.path().join(format!("{index}-{}", "a".repeat(180)));
                std::fs::create_dir(&path).unwrap();
                let original = &original;
                scope.spawn(move || {
                    let directory = DirFd::open(&path).unwrap();
                    let name = FileName::new("control.sock").unwrap();
                    let listener = in_directory(&directory, &name, |name| UnixListener::bind(name))
                        .unwrap()
                        .unwrap();
                    assert!(path.join(name.as_str()).exists());
                    let stream = in_directory(&directory, &name, |name| UnixStream::connect(name))
                        .unwrap()
                        .unwrap();
                    drop(listener.accept().unwrap());
                    drop(stream);
                    assert_eq!(&std::env::current_dir().unwrap(), original);
                    std::fs::remove_file(path.join(name.as_str())).unwrap();
                });
            }
        });
        assert_eq!(std::env::current_dir().unwrap(), original);
    }
}
