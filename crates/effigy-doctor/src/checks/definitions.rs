use std::path::Path;
use std::time::{Duration, Instant};

use super::catalog_checks::{
    run_environment_tools_check, run_health_task_check, run_manifest_conflicts_check,
    run_task_references_check,
};
use super::scan_checks::{run_duplicate_blocks_check, run_god_files_check};
use super::source_checks::run_pinned_source_drift_check;
use crate::progress::{ScanLabel, DUPLICATE_BLOCKS, GOD_FILES, HEALTH_TASK};
use crate::{manifest_snapshot::ManifestSnapshot, DoctorRuntimePorts, DoctorState};

pub(super) struct DoctorCheckContext<'a> {
    pub(super) resolved_root: &'a Path,
    pub(super) manifest: &'a ManifestSnapshot,
    pub(super) ports: &'a dyn DoctorRuntimePorts,
    pub(super) deadline: Option<Instant>,
}

impl<'a> DoctorCheckContext<'a> {
    pub(super) fn new(
        resolved_root: &'a Path,
        manifest: &'a ManifestSnapshot,
        ports: &'a dyn DoctorRuntimePorts,
        deadline: Option<Instant>,
    ) -> Self {
        Self {
            resolved_root,
            manifest,
            ports,
            deadline,
        }
    }

    pub(super) fn remaining_budget(&self) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }
}

pub(super) type DoctorCheckFn = fn(&DoctorCheckContext<'_>, &mut DoctorState);

#[derive(Clone, Copy)]
pub(super) struct DoctorCheckDefinition {
    pub(super) name: &'static str,
    pub(super) progress_label: Option<ScanLabel>,
    pub(super) run: DoctorCheckFn,
}

const STRUCTURAL_CHECKS: [DoctorCheckDefinition; 4] = [
    DoctorCheckDefinition {
        name: "manifest_conflicts",
        progress_label: None,
        run: run_manifest_conflicts_check,
    },
    DoctorCheckDefinition {
        name: "environment_tools",
        progress_label: None,
        run: run_environment_tools_check,
    },
    DoctorCheckDefinition {
        name: "task_references",
        progress_label: None,
        run: run_task_references_check,
    },
    DoctorCheckDefinition {
        name: "pinned_sources",
        progress_label: None,
        run: run_pinned_source_drift_check,
    },
];

const DEEP_CHECKS: [DoctorCheckDefinition; 3] = [
    DoctorCheckDefinition {
        name: "god_files",
        progress_label: Some(GOD_FILES),
        run: run_god_files_check,
    },
    DoctorCheckDefinition {
        name: "duplicate_blocks",
        progress_label: Some(DUPLICATE_BLOCKS),
        run: run_duplicate_blocks_check,
    },
    DoctorCheckDefinition {
        name: "health_task",
        progress_label: Some(HEALTH_TASK),
        run: run_health_task_check,
    },
];

pub(super) fn structural_check_definitions() -> &'static [DoctorCheckDefinition] {
    &STRUCTURAL_CHECKS
}

pub(super) fn deep_check_definitions() -> &'static [DoctorCheckDefinition] {
    &DEEP_CHECKS
}

#[cfg(test)]
pub(super) fn doctor_check_definitions() -> Vec<DoctorCheckDefinition> {
    STRUCTURAL_CHECKS
        .iter()
        .chain(DEEP_CHECKS.iter())
        .copied()
        .collect()
}
