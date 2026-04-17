use std::path::Path;

use effigy_tasks::ResolutionMode;

use super::super::super::model::constants::TASK_MANIFEST_FILE;
use super::super::contracts::{check_id, remediation};
use super::super::report::DoctorState;

pub(in crate::runner::doctor) enum WorkflowFinding<'a> {
    RootResolution {
        resolved_root: &'a Path,
        resolution_mode: ResolutionMode,
    },
    MissingManifestFiles {
        resolved_root: &'a Path,
    },
    NoValidManifests,
}

impl WorkflowFinding<'_> {
    pub(in crate::runner::doctor) fn emit(self, state: &mut DoctorState) {
        match self {
            Self::RootResolution {
                resolved_root,
                resolution_mode,
            } => {
                let root_mode = match resolution_mode {
                    ResolutionMode::Explicit => "explicit (--repo)",
                    ResolutionMode::AutoNearest => "auto (nearest root)",
                    ResolutionMode::AutoPromoted => "auto (promoted workspace root)",
                };
                state.add_check_info(
                    check_id::WORKSPACE_ROOT_RESOLUTION,
                    format!(
                        "resolved root `{}` using mode {root_mode}",
                        resolved_root.display()
                    ),
                    remediation::USE_REPO_OVERRIDE,
                );
            }
            Self::MissingManifestFiles { resolved_root } => {
                state.add_check_warning(
                    check_id::MANIFEST_PARSE,
                    format!(
                        "no `{}` files were discovered under {}",
                        TASK_MANIFEST_FILE,
                        resolved_root.display()
                    ),
                    remediation::ADD_MANIFEST,
                );
            }
            Self::NoValidManifests => {
                state.add_check_error(
                    check_id::MANIFEST_PARSE,
                    "no valid manifests were available for downstream checks",
                    remediation::FIX_MANIFEST_ERRORS_FIRST,
                );
            }
        }
    }
}
