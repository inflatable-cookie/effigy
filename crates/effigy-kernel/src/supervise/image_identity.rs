//! Unwired executable observations for future workspace helper qualification.
//!
//! These are not a receipt or a capability. On macOS, `proc_pidpath` and
//! metadata observe a path and vnode, not the child's mapped image. Replacement
//! between observations and exec, including an ABA replacement, is outside the
//! accepted accidental-skew model. Unknown observations must refuse.
use crate::ident::{self, ProcessIdentity};
use std::fs::{File, Metadata};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
struct ImageIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

impl ImageIdentity {
    fn from_metadata(metadata: &Metadata) -> io::Result<Self> {
        if !metadata.is_file() {
            return Err(io::Error::other("helper image is not a regular file"));
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.size(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
        })
    }
}

/// Holds the creator's observed executable open throughout qualification.
struct CreatorImage {
    file: File,
    launch_path: PathBuf,
    identity: ImageIdentity,
}

impl CreatorImage {
    fn capture() -> io::Result<Self> {
        let launch_path = current_image_path()?;
        let file = File::open(&launch_path)?;
        let identity = ImageIdentity::from_metadata(&file.metadata()?)?;
        Ok(Self {
            file,
            launch_path,
            identity,
        })
    }

    fn launch_path(&self) -> io::Result<&Path> {
        if ImageIdentity::from_metadata(&self.file.metadata()?)? != self.identity
            || ImageIdentity::from_metadata(&std::fs::metadata(&self.launch_path)?)?
                != self.identity
        {
            return Err(io::Error::other("creator image changed"));
        }
        Ok(&self.launch_path)
    }

    fn verify_process(&self, expected: &ProcessIdentity) -> io::Result<()> {
        if ident::process_identity(expected.pid).verified() != Some(expected) {
            return Err(io::Error::other(
                "helper process identity unavailable or changed",
            ));
        }
        let path = process_image_path(expected.pid)?;
        if ImageIdentity::from_metadata(&std::fs::metadata(path)?)? != self.identity
            || ident::process_identity(expected.pid).verified() != Some(expected)
        {
            return Err(io::Error::other("helper image or process identity changed"));
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn current_image_path() -> io::Result<PathBuf> {
    Ok(PathBuf::from("/proc/self/exe"))
}

#[cfg(target_os = "macos")]
fn current_image_path() -> io::Result<PathBuf> {
    std::env::current_exe()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn current_image_path() -> io::Result<PathBuf> {
    Err(unsupported_image_identity())
}

#[cfg(target_os = "linux")]
fn process_image_path(pid: u32) -> io::Result<PathBuf> {
    Ok(PathBuf::from(format!("/proc/{pid}/exe")))
}

#[cfg(target_os = "macos")]
fn process_image_path(pid: u32) -> io::Result<PathBuf> {
    ident::process_executable(pid)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_image_path(_pid: u32) -> io::Result<PathBuf> {
    Err(unsupported_image_identity())
}

#[cfg(any(test, not(any(target_os = "linux", target_os = "macos"))))]
fn unsupported_image_identity() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "workspace helper image identity unsupported on this platform",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_image_matches_and_wrong_pid_refuses() {
        let image = CreatorImage::capture().expect("capture executable");
        let identity = ident::process_identity(std::process::id())
            .verified()
            .cloned()
            .expect("current process identity");
        image.launch_path().expect("unchanged image");
        image
            .verify_process(&identity)
            .expect("same image and process");
        let mut wrong = identity;
        wrong.pid = 0;
        assert!(image.verify_process(&wrong).is_err());
    }

    #[test]
    fn non_regular_file_is_not_an_image() {
        let metadata = std::fs::metadata("/dev/null").expect("null device");
        assert!(ImageIdentity::from_metadata(&metadata).is_err());
    }

    #[test]
    fn unsupported_platform_has_named_refusal() {
        let error = unsupported_image_identity();
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
        assert_eq!(
            error.to_string(),
            "workspace helper image identity unsupported on this platform"
        );
    }
}
