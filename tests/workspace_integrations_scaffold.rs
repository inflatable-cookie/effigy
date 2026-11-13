//! Standalone fake proof only; production lifecycle is not wired to this seam.
#[path = "../src/runner/container_command/workspace_integrations.rs"]
mod workspace_integrations;
use std::time::{Duration, Instant};
use workspace_integrations::*;

/// Every budget proof below advances this clock explicitly, so expiry is
/// decided by the step that spends it and never by host scheduling.
fn clock() -> Instant {
    Instant::now()
}

struct Fake {
    deadline: Instant,
    events: Vec<(WorkspaceIntegrationStep, &'static str)>,
    fail: Option<WorkspaceIntegrationStep>,
    unknown: bool,
}
impl WorkspaceIntegrationAdapter for Fake {
    type Error = ();
    fn integration_deadline(&self) -> Instant {
        self.deadline
    }
    fn record_step(&mut self, s: WorkspaceIntegrationStep, done: bool) -> Result<(), ()> {
        self.events.push((s, if done { "done" } else { "record" }));
        Ok(())
    }
    fn clear_step(&mut self, s: WorkspaceIntegrationStep) -> Result<(), ()> {
        self.events.push((s, "clear"));
        Ok(())
    }
    fn reconcile(&mut self, s: WorkspaceIntegrationStep, _: Instant) -> Result<(), ()> {
        self.events.push((s, "run"));
        if self.fail == Some(s) {
            Err(())
        } else {
            Ok(())
        }
    }
    fn unwind(&mut self, s: WorkspaceIntegrationStep, _: Instant) -> Result<(), ()> {
        self.events.push((s, "undo"));
        if self.unknown {
            Err(())
        } else {
            Ok(())
        }
    }
}
fn fake() -> Fake {
    Fake {
        deadline: Instant::now() + Duration::from_secs(60),
        events: vec![],
        fail: None,
        unknown: false,
    }
}
#[test]
fn failure_unwinds_attempted_steps_in_reverse_and_retains_unknown() {
    let mut f = fake();
    f.fail = Some(WorkspaceIntegrationStep::Secrets);
    let mut outcome = WorkspaceIntegrationOutcome::default();
    assert!(run_workspace_integrations(&mut f, &mut outcome).is_err());
    f.unknown = true;
    assert_eq!(
        unwind_workspace_integrations(
            &mut f,
            &mut outcome,
            Instant::now() + Duration::from_secs(1)
        ),
        Err("integration effects unsettled; re-run up")
    );
    let undone: Vec<_> = f
        .events
        .iter()
        .filter(|(_, e)| *e == "undo")
        .map(|(s, _)| *s)
        .collect();
    assert_eq!(
        undone,
        vec![
            WorkspaceIntegrationStep::Secrets,
            WorkspaceIntegrationStep::Permissions,
            WorkspaceIntegrationStep::Readiness
        ]
    );
    f.unknown = false;
    assert!(unwind_workspace_integrations(
        &mut f,
        &mut outcome,
        Instant::now() + Duration::from_secs(1)
    )
    .is_ok());
}
#[test]
fn expired_deadline_starts_no_effect() {
    let mut f = fake();
    f.deadline = Instant::now();
    assert!(matches!(
        run_workspace_integrations(&mut f, &mut WorkspaceIntegrationOutcome::default()),
        Err(IntegrationFailure::IntegrationBudgetExhausted)
    ));
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
        Self {
            root: tempfile::Builder::new()
                .prefix("workspace-integrations-")
                .tempdir()
                .unwrap(),
            attempted: vec![],
            env: vec![],
            json: vec![],
            routes: vec![],
        }
    }
}
struct RecoveryFake<'a> {
    state: &'a mut Durable,
    deadline: Instant,
    crash_after: Option<WorkspaceIntegrationStep>,
}
impl WorkspaceIntegrationAdapter for RecoveryFake<'_> {
    type Error = &'static str;
    fn integration_deadline(&self) -> Instant {
        self.deadline
    }
    fn record_step(&mut self, step: WorkspaceIntegrationStep, _: bool) -> Result<(), Self::Error> {
        if !self.state.attempted.contains(&step) {
            self.state.attempted.push(step);
        }
        Ok(())
    }
    fn clear_step(&mut self, step: WorkspaceIntegrationStep) -> Result<(), Self::Error> {
        self.state.attempted.retain(|s| *s != step);
        Ok(())
    }
    fn reconcile(
        &mut self,
        step: WorkspaceIntegrationStep,
        deadline: Instant,
    ) -> Result<(), Self::Error> {
        if Instant::now() >= deadline {
            return Err("timeout");
        }
        match step {
            WorkspaceIntegrationStep::Secrets => {
                self.state.env = b"TOKEN=fake\n".to_vec();
                self.state.json = b"{\"TOKEN\":\"fake\"}\n".to_vec();
                std::fs::write(self.state.root.path().join("runtime.env"), &self.state.env)
                    .unwrap();
                std::fs::write(
                    self.state.root.path().join("runtime.json"),
                    &self.state.json,
                )
                .unwrap();
            }
            WorkspaceIntegrationStep::Routes => self.state.routes = vec!["app:127.0.0.1:1234"],
            _ => {}
        }
        if self.crash_after == Some(step) {
            return Err("interrupted after effect");
        }
        Ok(())
    }
    fn unwind(&mut self, step: WorkspaceIntegrationStep, _: Instant) -> Result<(), Self::Error> {
        match step {
            WorkspaceIntegrationStep::Secrets => {
                self.state.env.clear();
                self.state.json.clear();
                for name in ["runtime.env", "runtime.json"] {
                    let path = self.state.root.path().join(name);
                    if path.exists() {
                        std::fs::remove_file(path).unwrap();
                    }
                }
            }
            WorkspaceIntegrationStep::Routes => self.state.routes.clear(),
            _ => {}
        }
        Ok(())
    }
}
#[test]
fn every_interrupted_effect_reconciles_to_identical_fake_runtime_and_routes() {
    let steps = [
        WorkspaceIntegrationStep::Readiness,
        WorkspaceIntegrationStep::Permissions,
        WorkspaceIntegrationStep::Secrets,
        WorkspaceIntegrationStep::Routes,
        WorkspaceIntegrationStep::HostIntegrations,
    ];
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut baseline = Durable::default();
    run_workspace_integrations(
        &mut RecoveryFake {
            state: &mut baseline,
            deadline,
            crash_after: None,
        },
        &mut WorkspaceIntegrationOutcome::default(),
    )
    .unwrap();
    for step in steps {
        let mut state = Durable::default();
        assert!(run_workspace_integrations(
            &mut RecoveryFake {
                state: &mut state,
                deadline,
                crash_after: Some(step)
            },
            &mut WorkspaceIntegrationOutcome::default()
        )
        .is_err());
        let mut recovered =
            WorkspaceIntegrationOutcome::from_attempted(state.attempted.clone()).unwrap();
        run_workspace_integrations(
            &mut RecoveryFake {
                state: &mut state,
                deadline,
                crash_after: None,
            },
            &mut recovered,
        )
        .unwrap();
        assert_eq!(
            std::fs::read(state.root.path().join("runtime.env")).unwrap(),
            std::fs::read(baseline.root.path().join("runtime.env")).unwrap(),
            "{step:?}"
        );
        assert_eq!(
            std::fs::read(state.root.path().join("runtime.json")).unwrap(),
            std::fs::read(baseline.root.path().join("runtime.json")).unwrap(),
            "{step:?}"
        );
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
    assert!(unwind_workspace_integrations(
        &mut f,
        &mut outcome,
        Instant::now() + Duration::from_secs(1)
    )
    .is_ok());
}

struct StalledRecord {
    clock: Instant,
    deadline: Instant,
    effects: usize,
}
impl WorkspaceIntegrationAdapter for StalledRecord {
    type Error = ();
    fn integration_deadline(&self) -> Instant {
        self.deadline
    }
    fn now(&self) -> Instant {
        self.clock
    }
    fn record_step(&mut self, _: WorkspaceIntegrationStep, _: bool) -> Result<(), ()> {
        // This adapter spends the whole remaining budget recording the attempt.
        self.clock = self.deadline;
        Ok(())
    }
    fn clear_step(&mut self, _: WorkspaceIntegrationStep) -> Result<(), ()> {
        Ok(())
    }
    fn reconcile(&mut self, _: WorkspaceIntegrationStep, _: Instant) -> Result<(), ()> {
        self.effects += 1;
        Ok(())
    }
    fn unwind(&mut self, _: WorkspaceIntegrationStep, _: Instant) -> Result<(), ()> {
        Ok(())
    }
}
#[test]
fn deadline_is_checked_after_record_before_effect() {
    let base = clock();
    let mut f = StalledRecord {
        clock: base,
        deadline: base + Duration::from_secs(1),
        effects: 0,
    };
    assert!(matches!(
        run_workspace_integrations(&mut f, &mut WorkspaceIntegrationOutcome::default()),
        Err(IntegrationFailure::IntegrationBudgetExhausted)
    ));
    assert_eq!(f.effects, 0);
}

#[test]
fn partial_fake_effect_with_expired_cleanup_retains_custody_until_absent() {
    let mut state = Durable::default();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut outcome = WorkspaceIntegrationOutcome::default();
    let mut adapter = RecoveryFake {
        state: &mut state,
        deadline,
        crash_after: Some(WorkspaceIntegrationStep::Secrets),
    };
    assert!(run_workspace_integrations(&mut adapter, &mut outcome).is_err());
    assert!(!adapter.state.env.is_empty());
    assert_eq!(
        unwind_workspace_integrations(&mut adapter, &mut outcome, Instant::now()),
        Err("integration effects unsettled; re-run up")
    );
    assert!(!adapter.state.env.is_empty());
    assert!(unwind_workspace_integrations(
        &mut adapter,
        &mut outcome,
        Instant::now() + Duration::from_secs(1)
    )
    .is_ok());
    assert!(adapter.state.env.is_empty());
    assert!(!adapter.state.root.path().join("runtime.env").exists());
    assert!(!adapter.state.root.path().join("runtime.json").exists());
}

#[test]
fn recovered_partial_cleanup_preserves_reverse_order_after_rerun() {
    let mut f = fake();
    let mut outcome =
        WorkspaceIntegrationOutcome::from_attempted(vec![WorkspaceIntegrationStep::Secrets])
            .unwrap();
    f.fail = Some(WorkspaceIntegrationStep::Routes);
    assert!(run_workspace_integrations(&mut f, &mut outcome).is_err());
    assert!(unwind_workspace_integrations(
        &mut f,
        &mut outcome,
        Instant::now() + Duration::from_secs(1)
    )
    .is_ok());
    let undone: Vec<_> = f
        .events
        .iter()
        .filter(|(_, e)| *e == "undo")
        .map(|(s, _)| *s)
        .collect();
    assert_eq!(
        undone,
        vec![
            WorkspaceIntegrationStep::Routes,
            WorkspaceIntegrationStep::Secrets,
            WorkspaceIntegrationStep::Permissions,
            WorkspaceIntegrationStep::Readiness
        ]
    );
}

struct SlowEffect {
    clock: Instant,
    deadline: Instant,
    overshoot: bool,
    reconciled: Vec<WorkspaceIntegrationStep>,
}
impl WorkspaceIntegrationAdapter for SlowEffect {
    type Error = &'static str;
    fn integration_deadline(&self) -> Instant {
        self.deadline
    }
    fn now(&self) -> Instant {
        self.clock
    }
    fn record_step(&mut self, _: WorkspaceIntegrationStep, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }
    fn clear_step(&mut self, _: WorkspaceIntegrationStep) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reconcile(&mut self, step: WorkspaceIntegrationStep, _: Instant) -> Result<(), Self::Error> {
        self.reconciled.push(step);
        if step == WorkspaceIntegrationStep::HostIntegrations {
            // The final effect is the slow one: inside the budget or past it.
            self.clock = if self.overshoot {
                self.deadline
            } else {
                self.clock + Duration::from_millis(1)
            };
        }
        Ok(())
    }
    fn unwind(&mut self, _: WorkspaceIntegrationStep, _: Instant) -> Result<(), Self::Error> {
        Ok(())
    }
}
#[test]
fn slow_effect_succeeds_within_integration_budget() {
    let base = clock();
    let mut adapter = SlowEffect {
        clock: base,
        deadline: base + Duration::from_secs(1),
        overshoot: false,
        reconciled: Vec::new(),
    };
    assert!(
        run_workspace_integrations(&mut adapter, &mut WorkspaceIntegrationOutcome::default())
            .is_ok()
    );
    // Every ordered step actually reconciled; Ok alone would not prove that.
    assert_eq!(adapter.reconciled, STEPS);
}
#[test]
fn final_effect_overshoot_refuses_before_handoff() {
    let base = clock();
    let mut adapter = SlowEffect {
        clock: base,
        deadline: base + Duration::from_secs(1),
        overshoot: true,
        reconciled: Vec::new(),
    };
    let mut outcome = WorkspaceIntegrationOutcome::default();
    let failure = run_workspace_integrations(&mut adapter, &mut outcome).unwrap_err();
    assert_eq!(failure.to_string(), "integration budget exhausted");
    let cleanup_deadline = adapter.deadline + Duration::from_secs(1);
    assert!(unwind_workspace_integrations(&mut adapter, &mut outcome, cleanup_deadline).is_ok());
}

struct MidBudget {
    clock: Instant,
    deadline: Instant,
    ran: Vec<WorkspaceIntegrationStep>,
    undone: Vec<WorkspaceIntegrationStep>,
}
impl WorkspaceIntegrationAdapter for MidBudget {
    type Error = &'static str;
    fn integration_deadline(&self) -> Instant {
        self.deadline
    }
    fn now(&self) -> Instant {
        self.clock
    }
    fn record_step(
        &mut self,
        step: WorkspaceIntegrationStep,
        completed: bool,
    ) -> Result<(), Self::Error> {
        if step == WorkspaceIntegrationStep::Permissions && !completed {
            // Recording this attempt is what spends the remaining budget.
            self.clock = self.deadline;
        }
        Ok(())
    }
    fn clear_step(&mut self, _: WorkspaceIntegrationStep) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reconcile(&mut self, step: WorkspaceIntegrationStep, _: Instant) -> Result<(), Self::Error> {
        self.ran.push(step);
        Ok(())
    }
    fn unwind(&mut self, step: WorkspaceIntegrationStep, _: Instant) -> Result<(), Self::Error> {
        self.undone.push(step);
        Ok(())
    }
}
#[test]
fn budget_expiry_before_later_step_refuses_and_unwinds_attempts() {
    let base = clock();
    let mut adapter = MidBudget {
        clock: base,
        deadline: base + Duration::from_secs(1),
        ran: vec![],
        undone: vec![],
    };
    let mut outcome = WorkspaceIntegrationOutcome::default();
    assert!(matches!(
        run_workspace_integrations(&mut adapter, &mut outcome),
        Err(IntegrationFailure::IntegrationBudgetExhausted)
    ));
    assert_eq!(adapter.ran, vec![WorkspaceIntegrationStep::Readiness]);
    let cleanup_deadline = adapter.deadline + Duration::from_secs(1);
    assert!(unwind_workspace_integrations(&mut adapter, &mut outcome, cleanup_deadline).is_ok());
    assert_eq!(
        adapter.undone,
        vec![
            WorkspaceIntegrationStep::Permissions,
            WorkspaceIntegrationStep::Readiness
        ]
    );
}

#[test]
fn malformed_recovery_order_is_refused() {
    assert!(WorkspaceIntegrationOutcome::from_attempted(vec![
        WorkspaceIntegrationStep::Routes,
        WorkspaceIntegrationStep::Secrets
    ])
    .is_err());
    assert!(WorkspaceIntegrationOutcome::from_attempted(vec![
        WorkspaceIntegrationStep::Secrets,
        WorkspaceIntegrationStep::Secrets
    ])
    .is_err());
}

struct SlowUnwind {
    clock: Instant,
    deadline: Instant,
    clears: usize,
}
impl WorkspaceIntegrationAdapter for SlowUnwind {
    type Error = ();
    fn integration_deadline(&self) -> Instant {
        self.deadline
    }
    fn now(&self) -> Instant {
        self.clock
    }
    fn record_step(&mut self, _: WorkspaceIntegrationStep, _: bool) -> Result<(), ()> {
        Ok(())
    }
    fn clear_step(&mut self, _: WorkspaceIntegrationStep) -> Result<(), ()> {
        self.clears += 1;
        Ok(())
    }
    fn reconcile(&mut self, _: WorkspaceIntegrationStep, _: Instant) -> Result<(), ()> {
        Ok(())
    }
    fn unwind(&mut self, _: WorkspaceIntegrationStep, deadline: Instant) -> Result<(), ()> {
        // The withdrawal itself outlives the cleanup budget it was given.
        self.clock = deadline;
        Ok(())
    }
}
#[test]
fn cleanup_overshoot_does_not_clear_attempt() {
    let base = clock();
    let mut adapter = SlowUnwind {
        clock: base,
        deadline: base + Duration::from_secs(1),
        clears: 0,
    };
    let mut outcome =
        WorkspaceIntegrationOutcome::from_attempted(vec![WorkspaceIntegrationStep::Secrets])
            .unwrap();
    assert!(unwind_workspace_integrations(
        &mut adapter,
        &mut outcome,
        base + Duration::from_secs(1)
    )
    .is_err());
    assert_eq!(adapter.clears, 0);
}

struct LateClear {
    clock: Instant,
    integration_deadline: Instant,
    cleanup_deadline: Instant,
    journal_has_attempt: bool,
    effect_present: bool,
    delay_clear: bool,
}
impl WorkspaceIntegrationAdapter for LateClear {
    type Error = ();
    fn integration_deadline(&self) -> Instant {
        self.integration_deadline
    }
    fn now(&self) -> Instant {
        self.clock
    }
    fn record_step(&mut self, _: WorkspaceIntegrationStep, _: bool) -> Result<(), ()> {
        self.journal_has_attempt = true;
        Ok(())
    }
    fn clear_step(&mut self, _: WorkspaceIntegrationStep) -> Result<(), ()> {
        // The clear completes, but only after its cleanup budget is gone.
        if self.delay_clear {
            self.clock = self.cleanup_deadline;
        }
        self.journal_has_attempt = false;
        Ok(())
    }
    fn reconcile(&mut self, step: WorkspaceIntegrationStep, _: Instant) -> Result<(), ()> {
        if step == WorkspaceIntegrationStep::Secrets {
            self.effect_present = true;
        }
        Ok(())
    }
    fn unwind(&mut self, _: WorkspaceIntegrationStep, _: Instant) -> Result<(), ()> {
        self.effect_present = false;
        Ok(())
    }
}
#[test]
fn late_completed_clear_refuses_then_reconciles_safely() {
    // The clear itself spends the cleanup budget; nothing here waits on a clock.
    let base = clock();
    let cleanup_deadline = base + Duration::from_secs(2);
    let mut adapter = LateClear {
        clock: base,
        integration_deadline: base + Duration::from_secs(60),
        cleanup_deadline,
        journal_has_attempt: true,
        effect_present: true,
        delay_clear: true,
    };
    let mut outcome =
        WorkspaceIntegrationOutcome::from_attempted(vec![WorkspaceIntegrationStep::Secrets])
            .unwrap();
    assert_eq!(
        unwind_workspace_integrations(&mut adapter, &mut outcome, cleanup_deadline),
        Err("integration effects unsettled; re-run up")
    );
    assert!(!adapter.effect_present);
    assert!(!adapter.journal_has_attempt); // Verified absence was durably cleared, but late.
    adapter.delay_clear = false;
    assert!(unwind_workspace_integrations(
        &mut adapter,
        &mut outcome,
        cleanup_deadline + Duration::from_secs(1)
    )
    .is_ok());
    // Simulate reloading the now-empty fake journal and rerunning integration.
    let mut recovered = WorkspaceIntegrationOutcome::from_attempted(vec![]).unwrap();
    assert!(run_workspace_integrations(&mut adapter, &mut recovered).is_ok());
    assert!(adapter.effect_present);
    assert!(adapter.journal_has_attempt);
}
