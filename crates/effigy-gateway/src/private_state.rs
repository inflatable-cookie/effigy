//! Validation and preparation for caller-owned private gateway state.

use std::io;
use std::path::{Path, PathBuf};

use crate::error::GatewayError;

/// Select a private gateway state root for container and managed startup.
pub const PRIVATE_STATE_ROOT_ENV: &str = "EFFIGY_GATEWAY_PRIVATE_STATE_ROOT";

/// Validate a pre-created private state root before Effigy reads or writes it.
pub fn validate_root(path: &Path) -> Result<(), GatewayError> {
    let invalid = |reason: &str| {
        GatewayError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "unsafe private gateway state root {}: {reason}",
                path.display()
            ),
        ))
    };
    if !path.is_absolute()
        || path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err(invalid("the path must be absolute and cannot contain `..`"));
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        GatewayError::Io(io::Error::new(
            error.kind(),
            format!(
                "private gateway state root {} is unavailable: {error}",
                path.display()
            ),
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("the root must be a real directory, not a symlink"));
    }
    let canonical = std::fs::canonicalize(path).map_err(GatewayError::Io)?;
    if canonical != path {
        return Err(invalid("the path must already be canonical"));
    }
    validate_private_metadata(path, &metadata, true)?;
    validate_root_entries(path)
}

/// Create private gateway subdirectories and reject unsafe pre-existing state.
pub fn prepare_root(path: &Path) -> Result<(), GatewayError> {
    validate_root(path)?;
    for name in ["certs", "ca"] {
        let directory = path.join(name);
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(unsafe_child(&directory, "expected a real directory"));
                }
                validate_private_metadata(&directory, &metadata, true)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                create_owner_only_directory(&directory)?;
            }
            Err(error) => return Err(GatewayError::Io(error)),
        }
        validate_directory_entries(&directory)?;
    }
    validate_root_entries(path)
}

pub(crate) fn private_endpoint_path(path: &Path) -> PathBuf {
    path.join("gateway.addresses.json")
}

fn validate_root_entries(path: &Path) -> Result<(), GatewayError> {
    for entry in std::fs::read_dir(path).map_err(GatewayError::Io)? {
        let entry = entry.map_err(GatewayError::Io)?;
        let child = entry.path();
        // Certificate generation atomically replaces files while independent
        // managed profiles validate the shared private root. An entry that
        // vanished after `read_dir` no longer has a filesystem object to
        // follow or trust; every entry that still exists is fully inspected.
        let metadata = match std::fs::symlink_metadata(&child) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(GatewayError::Io(error)),
        };
        if metadata.file_type().is_symlink() {
            return Err(unsafe_child(&child, "symlinks are not supported"));
        }
        if metadata.is_dir() {
            if entry.file_name() == ".effigy" {
                validate_private_metadata(&child, &metadata, true)?;
                let op = crate::operator::context(
                    std::time::Duration::from_secs(3),
                    effigy_kernel::ctx::EffectClass::Observe,
                );
                match effigy_kernel::root::StateRoot::runtime_machine_observe(
                    &op.observe().expect("Observe context"),
                    path,
                ) {
                    effigy_kernel::observe::Observed::Verified(_, _)
                    | effigy_kernel::observe::Observed::Absent(_) => continue,
                    effigy_kernel::observe::Observed::Unknown(reason) => {
                        return Err(unsafe_child(
                            &child,
                            &format!("kernel state root unknown: {reason}"),
                        ))
                    }
                }
            }
            if !matches!(entry.file_name().to_str(), Some("certs" | "ca")) {
                return Err(unsafe_child(&child, "unexpected private state directory"));
            }
            validate_private_metadata(&child, &metadata, true)?;
            validate_directory_entries(&child)?;
        } else if metadata.is_file() {
            validate_private_metadata(&child, &metadata, false)?;
        } else {
            return Err(unsafe_child(&child, "expected a regular file or directory"));
        }
    }
    Ok(())
}

fn validate_directory_entries(path: &Path) -> Result<(), GatewayError> {
    for entry in std::fs::read_dir(path).map_err(GatewayError::Io)? {
        let entry = entry.map_err(GatewayError::Io)?;
        let child = entry.path();
        let metadata = match std::fs::symlink_metadata(&child) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(GatewayError::Io(error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(unsafe_child(
                &child,
                "expected a regular file, not a symlink",
            ));
        }
        validate_private_metadata(&child, &metadata, false)?;
    }
    Ok(())
}

fn validate_private_metadata(
    path: &Path,
    metadata: &std::fs::Metadata,
    directory: bool,
) -> Result<(), GatewayError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let uid = nix::unistd::Uid::effective().as_raw();
        if metadata.uid() != uid {
            return Err(unsafe_child(path, "the current user must own it"));
        }
        let mode = metadata.mode();
        let permissions_are_private = if directory {
            mode & 0o777 == 0o700
        } else {
            mode & 0o022 == 0
        };
        if !permissions_are_private {
            return Err(unsafe_child(
                path,
                if directory {
                    "directories must have mode 0700"
                } else {
                    "files must deny group and other writes"
                },
            ));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (path, metadata, directory);
        Err(GatewayError::Io(io::Error::new(
            io::ErrorKind::Unsupported,
            "private gateway state currently requires Unix ownership and permission checks",
        )))
    }
}

/// Create a directory only this user may use, with its final mode set
/// explicitly.
///
/// `mkdir`'s mode argument is masked by the process umask, so a caller running
/// with a permissive Linux umask such as `0002` would otherwise get a
/// group-writable directory that this module's own validation refuses. The
/// directory is created owner-only and then re-set, so the result never
/// depends on the inherited umask and no unsafe window is needed.
#[cfg(unix)]
pub(crate) fn create_owner_only_directory(path: &Path) -> Result<(), GatewayError> {
    use std::os::unix::fs::DirBuilderExt;

    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(unsafe_child(path, "expected a real directory"));
            }
            return Ok(());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(GatewayError::Io(error)),
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(path).map_err(GatewayError::Io)?;
    set_private_permissions(path)
}

/// Non-Unix placeholder: private state already refuses to run without Unix
/// ownership and permission checks.
#[cfg(not(unix))]
pub(crate) fn create_owner_only_directory(path: &Path) -> Result<(), GatewayError> {
    std::fs::create_dir_all(path).map_err(GatewayError::Io)
}

fn set_private_permissions(path: &Path) -> Result<(), GatewayError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(GatewayError::Io)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(GatewayError::Io(io::Error::new(
            io::ErrorKind::Unsupported,
            "private gateway state currently requires Unix permissions",
        )))
    }
}

fn unsafe_child(path: &Path, reason: &str) -> GatewayError {
    GatewayError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "unsafe private gateway state at {}: {reason}",
            path.display()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_root_requires_existing_canonical_owner_only_directory() {
        let fixture = tempfile::tempdir().expect("fixture");
        let root = fixture.path().join("gateway");
        std::fs::create_dir(&root).expect("root");
        set_private_permissions(&root).expect("private permissions");
        let root = std::fs::canonicalize(&root).expect("canonical root");

        validate_root(&root).expect("valid private root");
        prepare_root(&root).expect("prepare private root");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(root.join("ca"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755))
                .expect("broaden fixture permissions");
            assert!(validate_root(&root).is_err());
        }
    }

    #[test]
    fn private_root_refuses_symlink_and_unexpected_directory() {
        let fixture = tempfile::tempdir().expect("fixture");
        let target = fixture.path().join("target");
        std::fs::create_dir(&target).expect("target");
        set_private_permissions(&target).expect("target permissions");
        let target = std::fs::canonicalize(&target).expect("canonical target");
        let link = fixture.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        assert!(validate_root(&link).is_err());

        let root = fixture.path().join("gateway");
        std::fs::create_dir(&root).expect("root");
        set_private_permissions(&root).expect("root permissions");
        let root = std::fs::canonicalize(&root).expect("canonical root");
        let foreign = root.join("foreign");
        std::fs::create_dir(&foreign).expect("foreign directory");
        set_private_permissions(&foreign).expect("foreign permissions");
        assert!(validate_root(&root).is_err());
    }
}
