//! Working directory mapping between host and container.
//!
//! When the user is at `~/projects/client/app/Models/` on the host, and
//! the repo root maps to `/var/www/html` in the container, the exec
//! command should run with CWD `/var/www/html/app/Models/`.
//!
//! This module handles that path translation, including:
//! - Resolving the relative path from repo root to host CWD
//! - Mapping it to the container working directory
//! - Validating that the host CWD is inside the repo root

use std::path::{Path, PathBuf};

use crate::error::ExecError;

/// Maps working directories between host and container.
#[derive(Debug, Clone)]
pub struct CwdMapper {
    /// Absolute path to the repo root on the host.
    host_repo_root: PathBuf,

    /// Absolute path to the repo root inside the container.
    container_working_dir: PathBuf,
}

impl CwdMapper {
    /// Create a new CWD mapper.
    ///
    /// `host_repo_root` is the absolute path to the repo root on the host
    /// (e.g., `/Users/tom/projects/client`).
    ///
    /// `container_working_dir` is the absolute path where the repo is
    /// mounted inside the container (e.g., `/var/www/html`).
    pub fn new(host_repo_root: PathBuf, container_working_dir: PathBuf) -> Self {
        Self {
            host_repo_root,
            container_working_dir,
        }
    }

    /// Map a host working directory to the corresponding container path.
    ///
    /// Returns the container-side path that corresponds to the given host
    /// CWD. Returns an error if the host CWD is outside the repo root.
    pub fn host_to_container(&self, host_cwd: &Path) -> Result<PathBuf, ExecError> {
        let relative = self.relative_from_repo_root(host_cwd)?;
        Ok(self.container_working_dir.join(relative))
    }

    /// Map a container path back to the host path.
    ///
    /// Returns the host-side path that corresponds to the given container
    /// path. Returns an error if the container path is outside the
    /// container working directory.
    pub fn container_to_host(&self, container_path: &Path) -> Result<PathBuf, ExecError> {
        let relative = container_path
            .strip_prefix(&self.container_working_dir)
            .map_err(|_| ExecError::CwdOutsideRepo {
                cwd: container_path.to_path_buf(),
                repo_root: self.container_working_dir.clone(),
            })?;
        Ok(self.host_repo_root.join(relative))
    }

    /// Compute the relative path from the repo root to the given host path.
    ///
    /// Returns an error if the path is outside the repo root.
    fn relative_from_repo_root(&self, host_path: &Path) -> Result<PathBuf, ExecError> {
        // Canonicalize both paths to handle symlinks and `.` / `..`.
        let canonical_root = self.canonicalize_or_use(&self.host_repo_root);
        let canonical_cwd = self.canonicalize_or_use(host_path);

        canonical_cwd
            .strip_prefix(&canonical_root)
            .map(|p| p.to_path_buf())
            .map_err(|_| ExecError::CwdOutsideRepo {
                cwd: host_path.to_path_buf(),
                repo_root: self.host_repo_root.clone(),
            })
    }

    /// Try to canonicalize a path, falling back to the original if the
    /// path doesn't exist on disk (useful for testing with virtual paths).
    fn canonicalize_or_use(&self, path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    /// The host repo root.
    pub fn host_repo_root(&self) -> &Path {
        &self.host_repo_root
    }

    /// The container working directory.
    pub fn container_working_dir(&self) -> &Path {
        &self.container_working_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapper() -> CwdMapper {
        CwdMapper::new(
            PathBuf::from("/Users/tom/projects/client"),
            PathBuf::from("/var/www/html"),
        )
    }

    #[test]
    fn map_repo_root_to_container_root() {
        let m = mapper();
        let result = m
            .host_to_container(Path::new("/Users/tom/projects/client"))
            .unwrap();
        assert_eq!(result, PathBuf::from("/var/www/html"));
    }

    #[test]
    fn map_subdirectory() {
        let m = mapper();
        let result = m
            .host_to_container(Path::new("/Users/tom/projects/client/app/Models"))
            .unwrap();
        assert_eq!(result, PathBuf::from("/var/www/html/app/Models"));
    }

    #[test]
    fn map_deep_subdirectory() {
        let m = mapper();
        let result = m
            .host_to_container(Path::new(
                "/Users/tom/projects/client/app/Http/Controllers/Api",
            ))
            .unwrap();
        assert_eq!(
            result,
            PathBuf::from("/var/www/html/app/Http/Controllers/Api")
        );
    }

    #[test]
    fn reject_path_outside_repo() {
        let m = mapper();
        let result = m.host_to_container(Path::new("/Users/tom/other-project"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ExecError::CwdOutsideRepo { .. }));
    }

    #[test]
    fn reject_parent_directory_of_repo() {
        let m = mapper();
        let result = m.host_to_container(Path::new("/Users/tom/projects"));
        assert!(result.is_err());
    }

    #[test]
    fn container_to_host_roundtrip() {
        let m = mapper();
        let container_path = Path::new("/var/www/html/app/Models");
        let host_path = m.container_to_host(container_path).unwrap();
        assert_eq!(
            host_path,
            PathBuf::from("/Users/tom/projects/client/app/Models")
        );
    }

    #[test]
    fn container_to_host_root() {
        let m = mapper();
        let host = m
            .container_to_host(Path::new("/var/www/html"))
            .unwrap();
        assert_eq!(host, PathBuf::from("/Users/tom/projects/client"));
    }

    #[test]
    fn container_to_host_outside_working_dir() {
        let m = mapper();
        let result = m.container_to_host(Path::new("/tmp/something"));
        assert!(result.is_err());
    }

    #[test]
    fn works_with_real_filesystem_paths() {
        let dir = tempfile::tempdir().unwrap();
        let subdir = dir.path().join("src/app");
        std::fs::create_dir_all(&subdir).unwrap();

        let m = CwdMapper::new(
            dir.path().to_path_buf(),
            PathBuf::from("/workspace"),
        );

        let result = m.host_to_container(&subdir).unwrap();
        assert_eq!(result, PathBuf::from("/workspace/src/app"));
    }
}
