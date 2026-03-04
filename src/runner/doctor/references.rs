use std::path::Path;

use super::super::catalog::select_catalog_and_task;
use super::super::util::parse_task_reference_invocation;
use super::super::LoadedCatalog;
use super::contracts::{check_id, remediation};
use super::task_graph;
use super::DoctorState;

pub(super) fn check_task_references(catalogs: &[LoadedCatalog], state: &mut DoctorState) {
    let mut checker = ReferenceChecker::new(catalogs, state);
    for catalog in catalogs {
        task_graph::for_each_manifest_task_reference(&catalog.manifest, |task_name, reference| {
            checker.validate_task_reference(
                &catalog.catalog_root,
                &catalog.manifest_path,
                task_name,
                reference,
            );
        });
    }
}

struct ReferenceChecker<'a, 'b> {
    catalogs: &'a [LoadedCatalog],
    state: &'b mut DoctorState,
}

impl<'a, 'b> ReferenceChecker<'a, 'b> {
    fn new(catalogs: &'a [LoadedCatalog], state: &'b mut DoctorState) -> Self {
        Self { catalogs, state }
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
                    remediation::FIX_TASK_REFERENCE_SYNTAX,
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
                    remediation::UPDATE_TASK_REFERENCE_TARGET,
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
                remediation::REFERENCE_RUNNABLE_TASK,
            );
        }
    }

    fn push_resolution_error(&mut self, evidence: String, remediation: &str) {
        self.state
            .add_check_error(check_id::TASK_REFERENCES_RESOLVE, evidence, remediation);
    }
}

fn is_builtin_selector(task_name: &str) -> bool {
    matches!(
        task_name,
        "help" | "config" | "doctor" | "test" | "tasks" | "catalogs"
    )
}
