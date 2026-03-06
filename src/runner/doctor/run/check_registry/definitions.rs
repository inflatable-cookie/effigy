use std::path::Path;

use super::super::super::{
    conflicts, environment, generated_assets, god_files, health, references, DoctorState,
    ManifestSnapshot,
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
    pub(super) run: DoctorCheckFn,
}

const DOCTOR_CHECKS: [DoctorCheckDefinition; 6] = [
    DoctorCheckDefinition {
        name: "manifest_conflicts",
        run: run_manifest_conflicts_check,
    },
    DoctorCheckDefinition {
        name: "environment_tools",
        run: run_environment_tools_check,
    },
    DoctorCheckDefinition {
        name: "task_references",
        run: run_task_references_check,
    },
    DoctorCheckDefinition {
        name: "god_files",
        run: run_god_files_check,
    },
    DoctorCheckDefinition {
        name: "generated_assets",
        run: run_generated_assets_check,
    },
    DoctorCheckDefinition {
        name: "health_task",
        run: run_health_task_check,
    },
];

pub(super) fn doctor_check_definitions() -> &'static [DoctorCheckDefinition] {
    &DOCTOR_CHECKS
}

fn run_manifest_conflicts_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    conflicts::check_manifest_alias_conflicts(&context.manifest.parsed_catalogs, state);
}

fn run_environment_tools_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    environment::check_environment_tools(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        context.manifest.preferred_js_pm,
        state,
    );
}

fn run_task_references_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    references::check_task_references(&context.manifest.parsed_catalogs, state);
}

fn run_health_task_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    health::check_health_task(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        state,
    );
}

fn run_god_files_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    god_files::check_god_files(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        state,
    );
}

fn run_generated_assets_check(context: &DoctorCheckContext<'_>, state: &mut DoctorState) {
    generated_assets::check_generated_assets(
        context.resolved_root,
        &context.manifest.parsed_catalogs,
        state,
    );
}
