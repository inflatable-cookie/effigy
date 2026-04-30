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
        // `PathBuf::join("")` appends a trailing separator (e.g.
        // `/var/www/html` → `/var/www/html/`). Some container runtimes
        // (notably nerdctl/runc) treat that as a different path than the
        // container's WORKDIR and reject it as "outside of container mount
        // namespace root". When the host CWD equals the repo root, return
        // the container working dir unchanged.
        if relative.as_os_str().is_empty() {
            return Ok(self.container_working_dir.clone());
        }
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
#[path = "cwd/tests.rs"]
mod tests;
