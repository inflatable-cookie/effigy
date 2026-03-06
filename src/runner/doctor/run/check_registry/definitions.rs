use std::path::Path;

use super::super::super::report::{DoctorState, ManifestSnapshot};
use super::catalog_checks::{
    run_environment_tools_check, run_health_task_check, run_manifest_conflicts_check,
    run_task_references_check,
};
use super::scan_checks::{
    run_attention_markers_check, run_comment_ratio_check, run_duplicate_blocks_check,
    run_generated_assets_check, run_generated_in_src_check, run_god_files_check,
    run_stale_suppressions_check,
};

pub(super) struct DoctorCheckContext<'a> {
    pub(super) resolved_root: &'a Path,
    pub(super) manifest: &'a ManifestSnapshot,
}

impl<'a> DoctorCheckContext<'a> {
    pub(super) fn new(resolved_root: &'a Path, manifest: &'a ManifestSnapshot) -> Self {
        Self {
            resolved_root,
            manifest,
        }
    }
}

pub(super) type DoctorCheckFn = fn(&DoctorCheckContext<'_>, &mut DoctorState);

#[derive(Clone, Copy)]
pub(super) struct DoctorCheckDefinition {
    pub(super) name: &'static str,
    pub(super) progress_label: Option<&'static str>,
    pub(super) run: DoctorCheckFn,
}

const DOCTOR_CHECKS: [DoctorCheckDefinition; 11] = [
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
        name: "god_files",
        progress_label: Some("Doctor scan: god-files"),
        run: run_god_files_check,
    },
    DoctorCheckDefinition {
        name: "duplicate_blocks",
        progress_label: Some("Doctor scan: duplicate-blocks"),
        run: run_duplicate_blocks_check,
    },
    DoctorCheckDefinition {
        name: "comment_ratio",
        progress_label: Some("Doctor scan: comment-ratio"),
        run: run_comment_ratio_check,
    },
    DoctorCheckDefinition {
        name: "generated_assets",
        progress_label: Some("Doctor scan: generated-assets"),
        run: run_generated_assets_check,
    },
    DoctorCheckDefinition {
        name: "generated_in_src",
        progress_label: Some("Doctor scan: generated-in-src"),
        run: run_generated_in_src_check,
    },
    DoctorCheckDefinition {
        name: "attention_markers",
        progress_label: Some("Doctor scan: attention-markers"),
        run: run_attention_markers_check,
    },
    DoctorCheckDefinition {
        name: "stale_suppressions",
        progress_label: Some("Doctor scan: stale-suppressions"),
        run: run_stale_suppressions_check,
    },
    DoctorCheckDefinition {
        name: "health_task",
        progress_label: None,
        run: run_health_task_check,
    },
];

pub(super) fn doctor_check_definitions() -> &'static [DoctorCheckDefinition] {
    &DOCTOR_CHECKS
}
