//! Standalone fake proof only; production lifecycle is not wired to this seam.
#[path = "../src/runner/container_command/workspace_integrations.rs"]
mod workspace_integrations;
use std::time::{Duration, Instant};
use workspace_integrations::*;

struct Fake { deadline: Instant, events: Vec<(WorkspaceIntegrationStep, &'static str)>, fail: Option<WorkspaceIntegrationStep>, unknown: bool }
impl WorkspaceIntegrationAdapter for Fake {
    type Error = ();
    fn deadline(&self) -> Instant { self.deadline }
    fn record_step(&mut self, s: WorkspaceIntegrationStep, done: bool) -> Result<(), ()> { self.events.push((s, if done { "done" } else { "record" })); Ok(()) }
    fn reconcile(&mut self, s: WorkspaceIntegrationStep, _: Instant) -> Result<(), ()> { self.events.push((s, "run")); if self.fail == Some(s) { Err(()) } else { Ok(()) } }
    fn unwind(&mut self, s: WorkspaceIntegrationStep, _: Instant) -> Result<(), ()> { self.events.push((s, "undo")); if self.unknown { Err(()) } else { Ok(()) } }
}
fn fake() -> Fake { Fake { deadline: Instant::now() + Duration::from_secs(60), events: vec![], fail: None, unknown: false } }
#[test]
fn failure_unwinds_attempted_steps_in_reverse_and_retains_unknown() {
    let mut f = fake(); f.fail = Some(WorkspaceIntegrationStep::Secrets);
    let mut outcome = WorkspaceIntegrationOutcome::default();
    assert!(run_workspace_integrations(&mut f, &mut outcome).is_err());
    f.unknown = true;
    assert_eq!(unwind_workspace_integrations(&mut f, &mut outcome), Err("integration effects unsettled; re-run up"));
    let undone: Vec<_> = f.events.iter().filter(|(_, e)| *e == "undo").map(|(s, _)| *s).collect();
    assert_eq!(undone, vec![WorkspaceIntegrationStep::Secrets, WorkspaceIntegrationStep::Permissions, WorkspaceIntegrationStep::Readiness]);
    f.unknown = false;
    assert!(unwind_workspace_integrations(&mut f, &mut outcome).is_ok());
}
#[test]
fn expired_deadline_starts_no_effect() {
    let mut f = fake(); f.deadline = Instant::now();
    assert!(matches!(run_workspace_integrations(&mut f, &mut WorkspaceIntegrationOutcome::default()), Err(IntegrationFailure::Deadline)));
    assert!(f.events.is_empty());
}
