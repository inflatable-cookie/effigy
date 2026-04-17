//! Runtime ports the doctor orchestration layer uses to reach back
//! into the runner.
//!
//! Two narrow methods:
//!
//! - `run_manifest_task` executes the repo-defined `health` task
//!   during the health check.
//! - `select_deferral` is consulted by the explain subcommand to
//!   describe how a selection error would be fallback-routed.
//!
//! The runner provides `RunnerDoctorPorts` at the runner edge.

use std::path::{Path, PathBuf};

use effigy_cli::TaskInvocation;
use effigy_manifest::{DeferredCommand, LoadedCatalog};
use effigy_tasks::TaskSelector;

use crate::DoctorError;

pub trait DoctorRuntimePorts {
    fn run_manifest_task(
        &self,
        invocation: &TaskInvocation,
        cwd: PathBuf,
    ) -> Result<String, DoctorError>;

    fn select_deferral(
        &self,
        selector: &TaskSelector,
        catalogs: &[LoadedCatalog],
        cwd: &Path,
        workspace_root: &Path,
    ) -> Option<DeferredCommand>;
}
