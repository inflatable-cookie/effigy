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
    let mut checker = ReferenceChecker::new(catalogs, findings, statuses);
    for catalog in catalogs {
        for (task_name, task) in &catalog.manifest.tasks {
            for reference in referenced_task_selectors(task) {
                checker.validate_task_reference(
                    &catalog.catalog_root,
                    &catalog.manifest_path,
                    task_name,
                    reference,
                );
            }
        }
    }
}

struct ReferenceChecker<'a, 'b> {
    catalogs: &'a [LoadedCatalog],
    findings: &'b mut Vec<DoctorFinding>,
    statuses: &'b mut HashMap<String, DoctorSeverity>,
}

impl<'a, 'b> ReferenceChecker<'a, 'b> {
    fn new(
        catalogs: &'a [LoadedCatalog],
        findings: &'b mut Vec<DoctorFinding>,
        statuses: &'b mut HashMap<String, DoctorSeverity>,
    ) -> Self {
        Self {
            catalogs,
            findings,
            statuses,
        }
    }

    fn validate_task_reference(
        &mut self,
        reference_cwd: &Path,
        manifest_path: &Path,
        task_name: &str,
        reference: &str,
    ) {
        let (selector, _) = match parse_task_reference_invocation(reference) {
            Ok(value) => value,
            Err(error) => {
                self.push_resolution_error(
                    format!(
                        "{} task `{}` has invalid task reference `{}`: {}",
                        manifest_path.display(),
                        task_name,
                        reference,
                        error
                    ),
                    "Fix task reference syntax (`<task>` or `<catalog>/<task>`).",
                );
                return;
            }
        };

        if is_builtin_selector(&selector.task_name) {
            return;
        }

        let selection = match select_catalog_and_task(&selector, self.catalogs, reference_cwd) {
            Ok(selection) => selection,
            Err(error) => {
                self.push_resolution_error(
                    format!(
                        "{} task `{}` references `{}` but resolution failed: {}",
                        manifest_path.display(),
                        task_name,
                        reference,
                        error
                    ),
                    "Update task reference to an existing task selector.",
                );
                return;
            }
        };

        if selection.task.run.is_none() {
            self.push_resolution_error(
                format!(
                    "{} task `{}` references `{}` but target has no `run` command",
                    manifest_path.display(),
                    task_name,
                    reference
                ),
                "Add a `run` command to the referenced task or reference a runnable task.",
            );
        }
    }

    fn push_resolution_error(&mut self, evidence: String, remediation: &str) {
        super::add_finding(
            self.findings,
            self.statuses,
            DoctorFinding {
                check_id: "tasks.references.resolve".to_owned(),
                severity: DoctorSeverity::Error,
                evidence,
                remediation: remediation.to_owned(),
                fixable: false,
            },
        );
    }
}

fn referenced_task_selectors(task: &super::super::ManifestTask) -> Vec<&str> {
    let mut references: Vec<&str> = Vec::new();
    if let Some(run) = task.run.as_ref() {
        match run {
            super::super::ManifestManagedRun::Command(_) => {}
            super::super::ManifestManagedRun::Sequence(steps) => {
                for step in steps {
                    if let super::super::ManifestManagedRunStep::Step(table) = step {
                        if let Some(reference) = table.task.as_deref() {
                            references.push(reference);
                        }
                    }
                }
            }
        }
    }
    for entry in &task.concurrent {
        if let Some(reference) = entry.task.as_deref() {
            references.push(reference);
        }
    }
    for profile in task.profiles.values() {
        for entry in &profile.concurrent {
            if let Some(reference) = entry.task.as_deref() {
                references.push(reference);
            }
        }
    }
    references
}

fn is_builtin_selector(task_name: &str) -> bool {
    matches!(
        task_name,
        "help" | "config" | "doctor" | "test" | "tasks" | "catalogs"
    )
}
