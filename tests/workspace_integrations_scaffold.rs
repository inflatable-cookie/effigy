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
    fn clear_step(&mut self, s: WorkspaceIntegrationStep) -> Result<(), ()> { self.events.push((s, "clear")); Ok(()) }
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
    assert_eq!(unwind_workspace_integrations(&mut f, &mut outcome, Instant::now() + Duration::from_secs(1)), Err("integration effects unsettled; re-run up"));
    let undone: Vec<_> = f.events.iter().filter(|(_, e)| *e == "undo").map(|(s, _)| *s).collect();
    assert_eq!(undone, vec![WorkspaceIntegrationStep::Secrets, WorkspaceIntegrationStep::Permissions, WorkspaceIntegrationStep::Readiness]);
    f.unknown = false;
    assert!(unwind_workspace_integrations(&mut f, &mut outcome, Instant::now() + Duration::from_secs(1)).is_ok());
}
#[test]
fn expired_deadline_starts_no_effect() {
    let mut f = fake(); f.deadline = Instant::now();
    assert!(matches!(run_workspace_integrations(&mut f, &mut WorkspaceIntegrationOutcome::default()), Err(IntegrationFailure::Deadline)));
    assert!(f.events.is_empty());
}

// These files and routes model idempotent external effects. They are not the
// production secret materializer, gateway, or listener implementation.
struct Durable {
    root: tempfile::TempDir,
    attempted: Vec<WorkspaceIntegrationStep>,
    env: Vec<u8>,
    json: Vec<u8>,
    routes: Vec<&'static str>,
}
impl Default for Durable {
    fn default() -> Self {
        Self { root: tempfile::Builder::new().prefix("workspace-integrations-").tempdir().unwrap(), attempted: vec![], env: vec![], json: vec![], routes: vec![] }
    }
}
struct RecoveryFake<'a> {
    state: &'a mut Durable,
    deadline: Instant,
    crash_after: Option<WorkspaceIntegrationStep>,
}
impl WorkspaceIntegrationAdapter for RecoveryFake<'_> {
    type Error = &'static str;
    fn deadline(&self) -> Instant { self.deadline }
    fn record_step(&mut self, step: WorkspaceIntegrationStep, _: bool) -> Result<(), Self::Error> {
        if !self.state.attempted.contains(&step) { self.state.attempted.push(step); }
        Ok(())
    }
    fn clear_step(&mut self, step: WorkspaceIntegrationStep) -> Result<(), Self::Error> {
        self.state.attempted.retain(|s| *s != step);
        Ok(())
    }
    fn reconcile(&mut self, step: WorkspaceIntegrationStep, deadline: Instant) -> Result<(), Self::Error> {
        if Instant::now() >= deadline { return Err("timeout"); }
        match step {
            WorkspaceIntegrationStep::Secrets => {
                self.state.env = b"TOKEN=fake\n".to_vec();
                self.state.json = b"{\"TOKEN\":\"fake\"}\n".to_vec();
                std::fs::write(self.state.root.path().join("runtime.env"), &self.state.env).unwrap();
                std::fs::write(self.state.root.path().join("runtime.json"), &self.state.json).unwrap();
            }
            WorkspaceIntegrationStep::Routes => self.state.routes = vec!["app:127.0.0.1:1234"],
            _ => {}
        }
        if self.crash_after == Some(step) { return Err("interrupted after effect"); }
        Ok(())
    }
    fn unwind(&mut self, step: WorkspaceIntegrationStep, _: Instant) -> Result<(), Self::Error> {
        match step {
            WorkspaceIntegrationStep::Secrets => { self.state.env.clear(); self.state.json.clear(); }
            WorkspaceIntegrationStep::Routes => self.state.routes.clear(),
            _ => {}
        }
        Ok(())
    }
}
#[test]
fn every_interrupted_effect_reconciles_to_identical_fake_runtime_and_routes() {
    let steps = [WorkspaceIntegrationStep::Readiness, WorkspaceIntegrationStep::Permissions, WorkspaceIntegrationStep::Secrets, WorkspaceIntegrationStep::Routes, WorkspaceIntegrationStep::HostIntegrations];
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut baseline = Durable::default();
    run_workspace_integrations(&mut RecoveryFake { state: &mut baseline, deadline, crash_after: None }, &mut WorkspaceIntegrationOutcome::default()).unwrap();
    for step in steps {
        let mut state = Durable::default();
        assert!(run_workspace_integrations(&mut RecoveryFake { state: &mut state, deadline, crash_after: Some(step) }, &mut WorkspaceIntegrationOutcome::default()).is_err());
        let mut recovered = WorkspaceIntegrationOutcome::from_attempted(state.attempted.clone());
        run_workspace_integrations(&mut RecoveryFake { state: &mut state, deadline, crash_after: None }, &mut recovered).unwrap();
        assert_eq!(std::fs::read(state.root.path().join("runtime.env")).unwrap(), std::fs::read(baseline.root.path().join("runtime.env")).unwrap(), "{step:?}");
        assert_eq!(std::fs::read(state.root.path().join("runtime.json")).unwrap(), std::fs::read(baseline.root.path().join("runtime.json")).unwrap(), "{step:?}");
        assert_eq!(state.routes, baseline.routes, "{step:?}");
    }
}

#[test]
fn expired_cleanup_budget_retains_attempt_for_retry() {
    let mut f = fake();
    f.fail = Some(WorkspaceIntegrationStep::Secrets);
    let mut outcome = WorkspaceIntegrationOutcome::default();
    assert!(run_workspace_integrations(&mut f, &mut outcome).is_err());
    assert!(unwind_workspace_integrations(&mut f, &mut outcome, Instant::now()).is_err());
    assert!(!f.events.iter().any(|(_, event)| *event == "undo"));
    assert!(unwind_workspace_integrations(&mut f, &mut outcome, Instant::now() + Duration::from_secs(1)).is_ok());
}

struct StalledRecord { deadline: Instant, effects: usize }
impl WorkspaceIntegrationAdapter for StalledRecord {
    type Error = ();
    fn deadline(&self) -> Instant { self.deadline }
    fn record_step(&mut self, _: WorkspaceIntegrationStep, _: bool) -> Result<(), ()> {
        while Instant::now() < self.deadline { std::thread::yield_now(); }
        Ok(())
    }
    fn clear_step(&mut self, _: WorkspaceIntegrationStep) -> Result<(), ()> { Ok(()) }
    fn reconcile(&mut self, _: WorkspaceIntegrationStep, _: Instant) -> Result<(), ()> { self.effects += 1; Ok(()) }
    fn unwind(&mut self, _: WorkspaceIntegrationStep, _: Instant) -> Result<(), ()> { Ok(()) }
}
#[test]
fn deadline_is_checked_after_record_before_effect() {
    let mut f = StalledRecord { deadline: Instant::now() + Duration::from_millis(5), effects: 0 };
    assert!(matches!(run_workspace_integrations(&mut f, &mut WorkspaceIntegrationOutcome::default()), Err(IntegrationFailure::Deadline)));
    assert_eq!(f.effects, 0);
}
