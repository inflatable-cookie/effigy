//! Recorded clean baselines for Cargo lockfiles a link resolves.
//!
//! A Cargo link resolves the linked crates to local path sources inside the
//! consumer lockfiles. That resolution is machine-local, do-not-commit state,
//! and any later plain Cargo invocation may rewrite the same file again. The
//! record here keeps the exact committed bytes each affected lockfile had when
//! the link was applied (as a Git blob id), so an unlink can restore the clean
//! file byte for byte instead of depending on a fresh resolution, and can
//! refuse when the commit itself moved since the link.
//!
//! The record is bookkeeping, not desired state: it never decides whether a
//! link exists, and a missing record only falls back to the tracked baseline.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::state::write_atomic;
use crate::{DepsError, ProcessRequest, ReadOnlyProcess};

pub const CARGO_LOCK_BASELINES_SCHEMA: &str = "effigy.deps.cargo-lock-baselines.v1";
pub const CARGO_LOCK_BASELINES_SCHEMA_VERSION: u32 = 1;

const CARGO_LOCK_BASELINES_PATH: &str = ".effigy/local/cargo-lock-baselines.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CargoLockBaselines {
    pub schema: String,
    pub schema_version: u32,
    #[serde(default)]
    pub entries: Vec<CargoLockBaseline>,
}

impl CargoLockBaselines {
    pub fn empty() -> Self {
        Self {
            schema: CARGO_LOCK_BASELINES_SCHEMA.to_owned(),
            schema_version: CARGO_LOCK_BASELINES_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }

    pub fn normalize(&mut self) {
        self.entries
            .sort_by(|left, right| left.lockfile.cmp(&right.lockfile));
        self.entries.dedup();
    }

    pub fn baseline_for(&self, lockfile: &Path) -> Option<&CargoLockBaseline> {
        self.entries.iter().find(|entry| entry.lockfile == lockfile)
    }
}

impl Default for CargoLockBaselines {
    fn default() -> Self {
        Self::empty()
    }
}

/// One tracked lockfile's committed blob when the link was applied.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CargoLockBaseline {
    pub lockfile: PathBuf,
    pub blob: String,
}

#[derive(Debug, Clone)]
pub struct CargoLockBaselineStore {
    path: PathBuf,
}

impl CargoLockBaselineStore {
    pub fn for_checkout(repo_root: impl AsRef<Path>) -> Self {
        Self {
            path: crate::repo_state_root(repo_root.as_ref()).join(CARGO_LOCK_BASELINES_PATH),
        }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&self) -> Result<CargoLockBaselines, DepsError> {
        let Some(raw) = read_optional(&self.path)? else {
            return Ok(CargoLockBaselines::empty());
        };
        let mut record: CargoLockBaselines = serde_json::from_slice(&raw)
            .map_err(|error| DepsError::json("parse", &self.path, error))?;
        if record.schema != CARGO_LOCK_BASELINES_SCHEMA
            || record.schema_version != CARGO_LOCK_BASELINES_SCHEMA_VERSION
        {
            return Err(DepsError::invalid(
                &self.path,
                format!(
                    "unsupported Cargo lockfile baseline record schema `{}` version {}; expected `{}` version {}",
                    record.schema,
                    record.schema_version,
                    CARGO_LOCK_BASELINES_SCHEMA,
                    CARGO_LOCK_BASELINES_SCHEMA_VERSION
                ),
            ));
        }
        record.normalize();
        Ok(record)
    }

    /// Render the record with the stable shape a planned change carries.
    pub fn render(&self, record: &CargoLockBaselines) -> Result<String, DepsError> {
        let mut record = record.clone();
        record.normalize();
        let mut rendered = serde_json::to_string_pretty(&record)
            .map_err(|error| DepsError::json("render", &self.path, error))?;
        rendered.push('\n');
        Ok(rendered)
    }

    pub fn write(&self, record: &CargoLockBaselines) -> Result<(), DepsError> {
        let rendered = self.render(record)?;
        write_atomic(&self.path, rendered.as_bytes(), false)
    }

    /// Persist a record, removing the file once no baseline remains so an
    /// unlinked checkout leaves no machine-local residue behind.
    pub fn save(&self, record: &CargoLockBaselines) -> Result<(), DepsError> {
        if !record.entries.is_empty() {
            return self.write(record);
        }
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(DepsError::io("remove", &self.path, error)),
        }
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, DepsError> {
    match std::fs::read(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(DepsError::io("read", path, error)),
    }
}

/// The blob `path` has in `HEAD`, or `None` when Git does not track it there.
pub(crate) fn head_blob(
    repo_root: &Path,
    path: &Path,
    process: &impl ReadOnlyProcess,
) -> Result<Option<String>, DepsError> {
    let Some(relative) = repo_relative_to_git_root(repo_root, path, process)? else {
        return Ok(None);
    };
    let output = process.run(&ProcessRequest {
        program: "git".to_owned(),
        args: vec!["rev-parse".to_owned(), format!("HEAD:{relative}")],
        cwd: repo_root.to_path_buf(),
    });
    match output {
        Ok(output) => {
            let blob = output.stdout.trim();
            Ok((!blob.is_empty()).then(|| blob.to_owned()))
        }
        Err(_) => Ok(None),
    }
}

/// The exact bytes behind a recorded Git blob id.
pub(crate) fn blob_contents(
    repo_root: &Path,
    blob: &str,
    process: &impl ReadOnlyProcess,
) -> Result<String, DepsError> {
    process
        .run(&ProcessRequest {
            program: "git".to_owned(),
            args: vec!["cat-file".to_owned(), "blob".to_owned(), blob.to_owned()],
            cwd: repo_root.to_path_buf(),
        })
        .map(|output| output.stdout)
}

fn repo_relative_to_git_root(
    repo_root: &Path,
    path: &Path,
    process: &impl ReadOnlyProcess,
) -> Result<Option<String>, DepsError> {
    let toplevel = process.run(&ProcessRequest {
        program: "git".to_owned(),
        args: vec!["rev-parse".to_owned(), "--show-toplevel".to_owned()],
        cwd: repo_root.to_path_buf(),
    })?;
    let git_root = PathBuf::from(toplevel.stdout.trim());
    Ok(path
        .strip_prefix(&git_root)
        .ok()
        .map(|relative| relative.display().to_string()))
}
