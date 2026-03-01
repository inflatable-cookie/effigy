use std::collections::HashMap;
use std::path::Path;

use super::super::catalog::select_catalog_and_task;
use super::super::util::parse_task_reference_invocation;
use super::super::LoadedCatalog;
use super::{DoctorFinding, DoctorSeverity};

pub(super) fn check_task_references(
    catalogs: &[LoadedCatalog],
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    for catalog in catalogs {
        for (task_name, task) in &catalog.manifest.tasks {
            if let Some(run) = task.run.as_ref() {
                match run {
                    super::super::ManifestManagedRun::Command(_) => {}
                    super::super::ManifestManagedRun::Sequence(steps) => {
                        for step in steps {
                            if let super::super::ManifestManagedRunStep::Step(table) = step {
                                if let Some(reference) = table.task.as_ref() {
                                    validate_task_reference(
                                        catalogs,
                                        &catalog.catalog_root,
                                        &catalog.manifest_path,
                                        task_name,
                                        reference,
                                        findings,
                                        statuses,
                                    );
                                }
                            }
                        }
                    }
                }
            }

            for entry in &task.concurrent {
                if let Some(reference) = entry.task.as_ref() {
                    validate_task_reference(
                        catalogs,
                        &catalog.catalog_root,
                        &catalog.manifest_path,
                        task_name,
                        reference,
                        findings,
                        statuses,
                    );
                }
            }
            for profile in task.profiles.values() {
                for entry in &profile.concurrent {
                    if let Some(reference) = entry.task.as_ref() {
                        validate_task_reference(
                            catalogs,
                            &catalog.catalog_root,
                            &catalog.manifest_path,
                            task_name,
                            reference,
                            findings,
                            statuses,
                        );
                    }
                }
            }
        }
    }
}

fn validate_task_reference(
    catalogs: &[LoadedCatalog],
    reference_cwd: &Path,
    manifest_path: &Path,
    task_name: &str,
    reference: &str,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let (selector, _) = match parse_task_reference_invocation(reference) {
        Ok(value) => value,
        Err(error) => {
            super::add_finding(
                findings,
                statuses,
                DoctorFinding {
                    check_id: "tasks.references.resolve".to_owned(),
                    severity: DoctorSeverity::Error,
                    evidence: format!(
                        "{} task `{}` has invalid task reference `{}`: {}",
                        manifest_path.display(),
                        task_name,
                        reference,
                        error
                    ),
                    remediation: "Fix task reference syntax (`<task>` or `<catalog>/<task>`)."
                        .to_owned(),
                    fixable: false,
                },
            );
            return;
        }
    };

    if is_builtin_selector(&selector.task_name) {
        return;
    }

    let selection = match select_catalog_and_task(&selector, catalogs, reference_cwd) {
        Ok(selection) => selection,
        Err(error) => {
            super::add_finding(
                findings,
                statuses,
                DoctorFinding {
                    check_id: "tasks.references.resolve".to_owned(),
                    severity: DoctorSeverity::Error,
                    evidence: format!(
                        "{} task `{}` references `{}` but resolution failed: {}",
                        manifest_path.display(),
                        task_name,
                        reference,
                        error
                    ),
                    remediation: "Update task reference to an existing task selector.".to_owned(),
                    fixable: false,
                },
            );
            return;
        }
    };

    if selection.task.run.is_none() {
        super::add_finding(
            findings,
            statuses,
            DoctorFinding {
                check_id: "tasks.references.resolve".to_owned(),
                severity: DoctorSeverity::Error,
                evidence: format!(
                    "{} task `{}` references `{}` but target has no `run` command",
                    manifest_path.display(),
                    task_name,
                    reference
                ),
                remediation:
                    "Add a `run` command to the referenced task or reference a runnable task."
                        .to_owned(),
                fixable: false,
            },
        );
    }
}

fn is_builtin_selector(task_name: &str) -> bool {
    matches!(
        task_name,
        "help" | "config" | "doctor" | "test" | "tasks" | "catalogs"
    )
}
