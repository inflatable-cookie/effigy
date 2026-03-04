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
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn empty_manifest_snapshot() -> ManifestSnapshot {
        ManifestSnapshot {
            manifest_paths: Vec::new(),
            parsed_catalogs: Vec::new(),
            preferred_js_pm: None,
            parse_ok_any: false,
        }
    }

    fn collect_step_sequence(should_fix: bool) -> Vec<ManifestPreparationStep> {
        let mut machine = ManifestPreparationMachine::new(should_fix);
        let mut sequence = Vec::<ManifestPreparationStep>::new();
        while let Some(step) = machine.next_step() {
            sequence.push(step);
        }
        sequence
    }

    #[test]
    fn manifest_preparation_machine_transitions_to_done_without_fix_steps() {
        assert_eq!(
            collect_step_sequence(false),
            Vec::<ManifestPreparationStep>::new()
        );
    }

    #[test]
    fn manifest_preparation_machine_transitions_with_fix_follow_contract_sequence() {
        assert_eq!(
            collect_step_sequence(true),
            vec![
                ManifestPreparationStep::ApplyFixers,
                ManifestPreparationStep::RecollectAfterFix,
            ]
        );
    }

    #[test]
    fn manifest_preparation_machine_done_state_is_idempotent() {
        let mut machine = ManifestPreparationMachine::new(true);
        assert_eq!(
            machine.next_step(),
            Some(ManifestPreparationStep::ApplyFixers)
        );
        assert_eq!(
            machine.next_step(),
            Some(ManifestPreparationStep::RecollectAfterFix)
        );
        assert_eq!(machine.next_step(), None);
        assert_eq!(machine.next_step(), None);
    }

    #[test]
    fn prepare_manifest_snapshot_collects_once_when_fix_is_disabled() {
        let mut state = DoctorState::new();
        let resolved_root = Path::new("/tmp/doctor-workspace");
        let mut collect_count = 0usize;
        let mut apply_count = 0usize;

        let manifest = prepare_manifest_snapshot_with(
            resolved_root,
            false,
            &mut state,
            |_, _| {
                collect_count += 1;
                Ok(empty_manifest_snapshot())
            },
            |_, _, _| {
                apply_count += 1;
            },
        )
        .expect("fix disabled branch");

        assert_eq!(collect_count, 1);
        assert_eq!(apply_count, 0);
        assert!(!manifest.parse_ok_any);
    }

    #[test]
    fn prepare_manifest_snapshot_recollects_once_when_fix_is_enabled() {
        let mut state = DoctorState::new();
        let resolved_root = Path::new("/tmp/doctor-workspace");
        let mut collect_count = 0usize;
        let mut apply_count = 0usize;

        let manifest = prepare_manifest_snapshot_with(
            resolved_root,
            true,
            &mut state,
            |_, _| {
                collect_count += 1;
                if collect_count == 1 {
                    return Ok(empty_manifest_snapshot());
                }
                Ok(ManifestSnapshot {
                    manifest_paths: vec![PathBuf::from("/tmp/doctor-workspace/effigy.toml")],
                    parsed_catalogs: Vec::new(),
                    preferred_js_pm: None,
                    parse_ok_any: true,
                })
            },
            |_, _, _| {
                apply_count += 1;
            },
        )
        .expect("fix enabled branch");

        assert_eq!(collect_count, 2);
        assert_eq!(apply_count, 1);
        assert!(manifest.parse_ok_any);
    }

    #[test]
    fn prepare_manifest_snapshot_propagates_recollect_error_when_fix_is_enabled() {
        let mut state = DoctorState::new();
        let resolved_root = Path::new("/tmp/doctor-workspace");
        let mut collect_count = 0usize;

        let result = prepare_manifest_snapshot_with(
            resolved_root,
            true,
            &mut state,
            |_, _| {
                collect_count += 1;
                if collect_count == 1 {
                    return Ok(empty_manifest_snapshot());
                }
                Err(RunnerError::Cwd(std::io::Error::other(
                    "simulated recollect failure",
                )))
            },
            |_, _, _| {},
        );

        let err = match result {
            Err(err) => err,
            Ok(_) => panic!("recollect error should bubble up"),
        };
        assert!(matches!(err, RunnerError::Cwd(_)));
        assert_eq!(collect_count, 2);
    }

    #[test]
    fn prepare_manifest_snapshot_skips_recollect_error_path_when_fix_is_disabled() {
        let mut state = DoctorState::new();
        let resolved_root = Path::new("/tmp/doctor-workspace");
        let mut collect_count = 0usize;

        let manifest = prepare_manifest_snapshot_with(
            resolved_root,
            false,
            &mut state,
            |_, _| {
                collect_count += 1;
                if collect_count == 1 {
                    return Ok(empty_manifest_snapshot());
                }
                Ok(ManifestSnapshot {
                    manifest_paths: vec![PathBuf::from("/tmp/doctor-workspace/effigy.toml")],
                    parsed_catalogs: Vec::new(),
                    preferred_js_pm: None,
                    parse_ok_any: true,
                })
            },
            |_, _, _| {
                panic!("fixers should not run when fix is disabled");
            },
        )
        .expect("fix disabled branch should not recollect");

        assert_eq!(collect_count, 1);
        assert!(!manifest.parse_ok_any);
    }
}
