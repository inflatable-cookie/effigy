use std::path::{Path, PathBuf};

use crate::resolver::ResolvedTarget;

use super::super::super::super::RunnerError;
use super::super::super::{DoctorState, ManifestSnapshot};
use super::DoctorRunOutput;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkflowPhase {
    RootResolution,
    ManifestPreparation,
    Checks,
    FixFinalization,
    ManifestAvailabilityFindings,
    SummaryAndReport,
}

#[cfg(test)]
const WORKFLOW_PHASE_ORDER: [WorkflowPhase; 6] = [
    WorkflowPhase::RootResolution,
    WorkflowPhase::ManifestPreparation,
    WorkflowPhase::Checks,
    WorkflowPhase::FixFinalization,
    WorkflowPhase::ManifestAvailabilityFindings,
    WorkflowPhase::SummaryAndReport,
];

#[cfg(test)]
pub(super) fn workflow_phase_order() -> &'static [WorkflowPhase] {
    &WORKFLOW_PHASE_ORDER
}

pub(super) trait WorkflowPhaseHandler {
    fn resolve_root(
        &mut self,
        cwd: PathBuf,
        repo_override: Option<PathBuf>,
    ) -> Result<ResolvedTarget, RunnerError>;

    fn emit_root_resolution_finding(&mut self, resolved: &ResolvedTarget, state: &mut DoctorState);

    fn prepare_manifest(
        &mut self,
        resolved_root: &Path,
        fix: bool,
        state: &mut DoctorState,
    ) -> Result<ManifestSnapshot, RunnerError>;

    fn run_checks(
        &mut self,
        resolved_root: &Path,
        manifest: &ManifestSnapshot,
        state: &mut DoctorState,
    );

    fn finalize_fix_actions(&mut self, state: &mut DoctorState, fix: bool);

    fn add_manifest_availability_findings(
        &mut self,
        resolved_root: &Path,
        manifest: &ManifestSnapshot,
        state: &mut DoctorState,
    );

    fn summarize_and_report(
        &mut self,
        state: DoctorState,
        resolved: ResolvedTarget,
    ) -> DoctorRunOutput;
}

pub(super) fn run_workflow_phase_pipeline<H: WorkflowPhaseHandler>(
    cwd: PathBuf,
    repo_override: Option<PathBuf>,
    fix: bool,
    handler: &mut H,
) -> Result<DoctorRunOutput, RunnerError> {
    let mut state = DoctorState::new();
    let resolved = handler.resolve_root(cwd, repo_override)?;
    handler.emit_root_resolution_finding(&resolved, &mut state);

    let manifest = handler.prepare_manifest(&resolved.resolved_root, fix, &mut state)?;
    handler.run_checks(&resolved.resolved_root, &manifest, &mut state);
    handler.finalize_fix_actions(&mut state, fix);
    handler.add_manifest_availability_findings(&resolved.resolved_root, &manifest, &mut state);

    Ok(handler.summarize_and_report(state, resolved))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    use crate::resolver::ResolvedTarget;
    use crate::tasks::ResolutionMode;

    use super::*;

    struct MockWorkflowPhaseHandler {
        visited: RefCell<Vec<WorkflowPhase>>,
        fail_phase: Option<WorkflowPhase>,
        resolved_root: PathBuf,
        fixed_flags: RefCell<Vec<bool>>,
    }

    impl MockWorkflowPhaseHandler {
        fn new() -> Self {
            Self {
                visited: RefCell::new(Vec::new()),
                fail_phase: None,
                resolved_root: PathBuf::from("/tmp/doctor-workspace"),
                fixed_flags: RefCell::new(Vec::new()),
            }
        }

        fn with_failure(phase: WorkflowPhase) -> Self {
            let mut instance = Self::new();
            instance.fail_phase = Some(phase);
            instance
        }

        fn record(&self, phase: WorkflowPhase) {
            self.visited.borrow_mut().push(phase);
        }

        fn should_fail(&self, phase: WorkflowPhase) -> bool {
            self.fail_phase == Some(phase)
        }
    }

    impl WorkflowPhaseHandler for MockWorkflowPhaseHandler {
        fn resolve_root(
            &mut self,
            _cwd: PathBuf,
            _repo_override: Option<PathBuf>,
        ) -> Result<ResolvedTarget, RunnerError> {
            self.record(WorkflowPhase::RootResolution);
            if self.should_fail(WorkflowPhase::RootResolution) {
                return Err(RunnerError::task_invocation("simulated resolve failure"));
            }
            Ok(ResolvedTarget {
                resolved_root: self.resolved_root.clone(),
                resolution_mode: ResolutionMode::Explicit,
                evidence: Vec::new(),
                warnings: Vec::new(),
            })
        }

        fn emit_root_resolution_finding(
            &mut self,
            _resolved: &ResolvedTarget,
            _state: &mut DoctorState,
        ) {
        }

        fn prepare_manifest(
            &mut self,
            _resolved_root: &Path,
            fix: bool,
            _state: &mut DoctorState,
        ) -> Result<ManifestSnapshot, RunnerError> {
            self.record(WorkflowPhase::ManifestPreparation);
            self.fixed_flags.borrow_mut().push(fix);
            if self.should_fail(WorkflowPhase::ManifestPreparation) {
                return Err(RunnerError::task_invocation(
                    "simulated manifest prep failure",
                ));
            }
            Ok(ManifestSnapshot {
                manifest_paths: vec![PathBuf::from("/tmp/doctor-workspace/effigy.toml")],
                parsed_catalogs: Vec::new(),
                preferred_js_pm: None,
                parse_ok_any: true,
            })
        }

        fn run_checks(
            &mut self,
            _resolved_root: &Path,
            _manifest: &ManifestSnapshot,
            _state: &mut DoctorState,
        ) {
            self.record(WorkflowPhase::Checks);
        }

        fn finalize_fix_actions(&mut self, _state: &mut DoctorState, fix: bool) {
            self.record(WorkflowPhase::FixFinalization);
            self.fixed_flags.borrow_mut().push(fix);
        }

        fn add_manifest_availability_findings(
            &mut self,
            _resolved_root: &Path,
            _manifest: &ManifestSnapshot,
            _state: &mut DoctorState,
        ) {
            self.record(WorkflowPhase::ManifestAvailabilityFindings);
        }

        fn summarize_and_report(
            &mut self,
            state: DoctorState,
            resolved: ResolvedTarget,
        ) -> DoctorRunOutput {
            self.record(WorkflowPhase::SummaryAndReport);
            let summary = state.summarize();
            let error_count = summary.error;
            let report = state.into_report(summary, resolved.evidence, resolved.warnings);
            DoctorRunOutput {
                report,
                error_count,
            }
        }
    }

    #[test]
    fn workflow_phase_order_is_stable() {
        assert_eq!(
            workflow_phase_order(),
            &[
                WorkflowPhase::RootResolution,
                WorkflowPhase::ManifestPreparation,
                WorkflowPhase::Checks,
                WorkflowPhase::FixFinalization,
                WorkflowPhase::ManifestAvailabilityFindings,
                WorkflowPhase::SummaryAndReport,
            ]
        );
    }

    #[test]
    fn workflow_phase_pipeline_runs_handlers_in_declarative_order() {
        let mut handler = MockWorkflowPhaseHandler::new();
        let _ = run_workflow_phase_pipeline(
            PathBuf::from("/tmp/doctor-workspace"),
            None,
            true,
            &mut handler,
        )
        .expect("pipeline should succeed");

        assert_eq!(*handler.visited.borrow(), workflow_phase_order());
        assert_eq!(*handler.fixed_flags.borrow(), vec![true, true]);
    }

    #[test]
    fn workflow_phase_pipeline_propagates_phase_error_and_stops_subsequent_phases() {
        let mut handler =
            MockWorkflowPhaseHandler::with_failure(WorkflowPhase::ManifestPreparation);

        let result = run_workflow_phase_pipeline(
            PathBuf::from("/tmp/doctor-workspace"),
            None,
            false,
            &mut handler,
        );
        let err = match result {
            Err(err) => err,
            Ok(_) => panic!("pipeline should fail at manifest preparation phase"),
        };
        assert_task_invocation_contains(
            err,
            "simulated manifest prep failure",
            "manifest preparation error contract",
        );

        assert_eq!(
            *handler.visited.borrow(),
            vec![
                WorkflowPhase::RootResolution,
                WorkflowPhase::ManifestPreparation,
            ]
        );
    }

    fn assert_task_invocation_contains(err: RunnerError, expected: &str, context: &str) {
        match err {
            RunnerError::TaskInvocation(message) => {
                assert!(
                    message.contains(expected),
                    "{context}: expected `{expected}` in `{message}`"
                );
            }
            other => panic!("{context}: unexpected error: {other}"),
        }
    }
}
