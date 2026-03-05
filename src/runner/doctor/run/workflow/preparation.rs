use std::path::Path;

use super::super::super::super::RunnerError;
use super::super::super::{DoctorState, ManifestSnapshot};

pub(super) fn prepare_manifest_snapshot_with<C, A>(
    resolved_root: &Path,
    should_fix: bool,
    state: &mut DoctorState,
    mut collect_snapshot: C,
    mut apply_fixers: A,
) -> Result<ManifestSnapshot, RunnerError>
where
    C: FnMut(&Path, &mut DoctorState) -> Result<ManifestSnapshot, RunnerError>,
    A: FnMut(&Path, &ManifestSnapshot, &mut DoctorState),
{
    let mut machine = ManifestPreparationMachine::new(should_fix);
    let mut snapshot = collect_snapshot(resolved_root, state)?;
    while let Some(step) = machine.next_step() {
        match step {
            ManifestPreparationStep::ApplyFixers => {
                apply_fixers(resolved_root, &snapshot, state);
            }
            ManifestPreparationStep::RecollectAfterFix => {
                snapshot = collect_snapshot(resolved_root, state)?;
            }
        }
    }
    Ok(snapshot)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestPreparationStep {
    ApplyFixers,
    RecollectAfterFix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestPreparationPhase {
    AfterInitialCollect,
    AfterFixersApplied,
    Done,
}

#[derive(Debug, Clone, Copy)]
struct ManifestPreparationMachine {
    phase: ManifestPreparationPhase,
    should_fix: bool,
}

impl ManifestPreparationMachine {
    fn new(should_fix: bool) -> Self {
        Self {
            phase: ManifestPreparationPhase::AfterInitialCollect,
            should_fix,
        }
    }

    fn next_step(&mut self) -> Option<ManifestPreparationStep> {
        match self.phase {
            ManifestPreparationPhase::AfterInitialCollect if self.should_fix => {
                self.phase = ManifestPreparationPhase::AfterFixersApplied;
                Some(ManifestPreparationStep::ApplyFixers)
            }
            ManifestPreparationPhase::AfterInitialCollect => {
                self.phase = ManifestPreparationPhase::Done;
                None
            }
            ManifestPreparationPhase::AfterFixersApplied => {
                self.phase = ManifestPreparationPhase::Done;
                Some(ManifestPreparationStep::RecollectAfterFix)
            }
            ManifestPreparationPhase::Done => None,
        }
    }
}

#[cfg(test)]
#[path = "preparation/tests.rs"]
mod tests;
