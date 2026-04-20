use std::path::Path;

use effigy_manifest::{resolve_task_execution_binding_from_systems, LoadedCatalog};
use effigy_routing::select_catalog_and_task;
use effigy_tasks::parse_task_reference_invocation;

use super::task_graph;
use crate::{task_references, DoctorState};

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
                task_references::add_invalid_reference_syntax(
                    self.state,
                    manifest_path,
                    task_name,
                    reference,
                    &error.to_string(),
                );
                return;
            }
        };

        if task_references::is_builtin_selector(&selector.task_name) {
            return;
        }

        let selection = match select_catalog_and_task(&selector, self.catalogs, reference_cwd) {
            Ok(selection) => selection,
            Err(error) => {
                task_references::add_unresolved_reference(
                    self.state,
                    manifest_path,
                    task_name,
                    reference,
                    &error.to_string(),
                );
                return;
            }
        };

        let execution_binding = resolve_task_execution_binding_from_systems(
            selection.catalog.manifest.systems.as_ref(),
            &selector.task_name,
            selection.task,
        )
        .ok()
        .flatten();

        if selection.task.run.is_none()
            && selection.task.mode.is_none()
            && execution_binding.is_none()
        {
            task_references::add_non_runnable_reference(
                self.state,
                manifest_path,
                task_name,
                reference,
            );
        }
    }
}
