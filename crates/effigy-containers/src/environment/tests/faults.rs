//! §12.1 fault injection at every acquisition and publication boundary of
//! the reconciler: the gate taken, the record committed, the backend
//! applying, the generation promoted, the state published and holders
//! attached. Every crossing gets EPERM, ENOSPC and SIGKILL; recovery is the
//! same command run again. Oracles: O3 (one unresolved run per owner),
//! O5 (no backend apply repeated for one generation), O6 (store integrity),
//! O7 (records parse as old or new), plus the scenario's own end state.
#![cfg(feature = "fault-injection")]

use super::*;
use effigy_kernel::fault_injection::with_hook;
use effigy_test_support::process::FixtureCommandExt;
use std::cell::Cell;
use std::os::unix::process::ExitStatusExt;

const CHILD: &str = "EFFIGY_TEST_ENVIRONMENT_FAULT";
const CHILD_HOME: &str = "EFFIGY_TEST_ENVIRONMENT_HOME";
const CHILD_SCENARIO: &str = "EFFIGY_TEST_ENVIRONMENT_SCENARIO";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Up,
    Down,
    Reset,
}

impl Scenario {
    const ALL: [Scenario; 3] = [Self::Up, Self::Down, Self::Reset];

    fn name(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Reset => "reset",
        }
    }

    fn parse(name: &str) -> Self {
        *Self::ALL
            .iter()
            .find(|scenario| scenario.name() == name)
            .expect("scenario")
    }

    /// The boundaries this scenario must cross for its sweep to count.
    fn required(self) -> &'static [&'static str] {
        match self {
            Self::Up => &[
                "GateAcquire",
                "BeginAdmitted",
                "ObjectPublishBefore",
                "EngineRecordBefore",
                "EngineRecordAfter",
                "IntendBefore",
                "IntendAfter",
                "EnvBackendBefore",
                "EnvBackendAfter",
                "AppliedBefore",
                "AppliedAfter",
                "ObjectRefBefore",
                "ObjectRefAfter",
                "AttachBefore",
                "AttachAfter",
                "SettleCloseBefore",
            ],
            Self::Down | Self::Reset => &[
                "GateAcquire",
                "BeginAdmitted",
                "IntendBefore",
                "EnvBackendBefore",
                "EnvBackendAfter",
                "AppliedAfter",
                "EnvStateBefore",
                "EnvStateAfter",
                "EngineRecordAfter",
                "SettleCloseBefore",
            ],
        }
    }
}

fn log(home: &Path) -> PathBuf {
    home.join("backend-calls")
}

/// The state each scenario starts from, built without any hook.
fn prepare(scenario: Scenario, home: &Path) {
    let root = root(home);
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let fake = Fake::with_log(Some(log(home)));
    match scenario {
        Scenario::Up => {}
        Scenario::Down | Scenario::Reset => {
            bring_up(
                &ctx,
                &env("stack"),
                &fake,
                &closure("one"),
                Retention::Persistent,
                &[],
            )
            .unwrap();
        }
    }
}

fn operate(scenario: Scenario, home: &Path) -> Result<(), EnvError> {
    let root = root(home);
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let fake = Fake::with_log(Some(log(home)));
    let stack = env("stack");
    match scenario {
        Scenario::Up => bring_up(
            &ctx,
            &stack,
            &fake,
            &closure("one"),
            Retention::Persistent,
            &[],
        )
        .map(|_| ()),
        Scenario::Down => take_down(&ctx, &stack, Operation::Down, &fake, None),
        Scenario::Reset => reset(&ctx, &stack, &selection(), PROJECT, || fake.stop()),
    }
}

fn applies(home: &Path) -> usize {
    std::fs::read_to_string(log(home))
        .unwrap_or_default()
        .lines()
        .filter(|line| *line == "apply")
        .count()
}

fn stops(home: &Path) -> usize {
    std::fs::read_to_string(log(home))
        .unwrap_or_default()
        .lines()
        .filter(|line| *line == "stop")
        .count()
}

/// Recovery is the same command again; then every oracle must hold.
fn recover_and_check(scenario: Scenario, home: &Path, label: &str) {
    operate(scenario, home).unwrap_or_else(|error| panic!("{label}: recovery failed: {error}"));
    let root = root(home);
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    assert_store_sound(&ctx);
    let stack = record(&ctx, &env("stack"));
    match scenario {
        Scenario::Up => {
            assert!(stack.active(), "{label}: not active after recovery");
            assert!(applies(home) >= 1, "{label}: the recovery never applied");
        }
        Scenario::Down | Scenario::Reset => {
            assert_eq!(stack.state, EnvState::Stopped, "{label}");
            assert!(applies(home) >= 1, "{label}: the stack was never applied");
            assert!(stops(home) >= 1, "{label}: never stopped");
            if scenario == Scenario::Reset {
                assert_eq!(stack.desired, None, "{label}: reset kept its render");
            }
        }
    }
}

#[test]
fn faults_reconciler_boundaries_up() {
    faults_reconciler_boundaries(Scenario::Up);
}
#[test]
fn faults_reconciler_boundaries_down() {
    faults_reconciler_boundaries(Scenario::Down);
}
#[test]
fn faults_reconciler_boundaries_reset() {
    faults_reconciler_boundaries(Scenario::Reset);
}

fn faults_reconciler_boundaries(scenario: Scenario) {
    if let Ok(target) = std::env::var(CHILD) {
        let target: usize = target.parse().expect("crossing");
        let home = PathBuf::from(std::env::var_os(CHILD_HOME).expect("home"));
        assert_eq!(
            scenario,
            Scenario::parse(&std::env::var(CHILD_SCENARIO).expect("scenario"))
        );
        let mut count = 0;
        let _ = with_hook(
            move |_| {
                if count == target {
                    nix::sys::signal::kill(
                        nix::unistd::Pid::this(),
                        nix::sys::signal::Signal::SIGKILL,
                    )
                    .expect("kill only this fault child");
                    panic!("SIGKILL did not terminate the child");
                }
                count += 1;
                Ok(())
            },
            || operate(scenario, &home),
        );
        panic!("fault #{target} not delivered");
    }
    let traced = home();
    prepare(scenario, traced.path());
    let points = Rc::new(RefCell::new(Vec::new()));
    let trace = points.clone();
    with_hook(
        move |point| {
            trace.borrow_mut().push(point);
            Ok(())
        },
        || operate(scenario, traced.path()),
    )
    .unwrap_or_else(|error| panic!("{scenario:?} trace: {error}"));
    let points = points.borrow().clone();
    for required in scenario.required() {
        assert!(
            points.contains(required),
            "{scenario:?} never crosses {required}"
        );
    }
    for (target, point) in points.iter().enumerate() {
        for errno in [libc_eperm(), libc_enospc()] {
            let fresh = home();
            prepare(scenario, fresh.path());
            let fired = Rc::new(Cell::new(false));
            let delivered = fired.clone();
            let mut count = 0;
            let _ = with_hook(
                move |_| {
                    let here = count;
                    count += 1;
                    if here == target {
                        delivered.set(true);
                        Err(std::io::Error::from_raw_os_error(errno))
                    } else {
                        Ok(())
                    }
                },
                || operate(scenario, fresh.path()),
            );
            assert!(fired.get(), "{scenario:?}: undelivered {point} #{target}");
            recover_and_check(
                scenario,
                fresh.path(),
                &format!(
                    "{scenario:?} errno {errno} at {point} #{target} after {:?}",
                    &points[target.saturating_sub(4)..target]
                ),
            );
        }
        let fresh = home();
        prepare(scenario, fresh.path());
        let status = std::process::Command::new(std::env::current_exe().expect("test exe"))
            .arg("--exact")
            .arg(format!(
                "environment::tests::faults::faults_reconciler_boundaries_{}",
                scenario.name()
            ))
            .arg("--nocapture")
            .current_dir(fresh.path())
            .env(CHILD, target.to_string())
            .env(CHILD_HOME, fresh.path())
            .env(CHILD_SCENARIO, scenario.name())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn_owned()
            .expect("owned fault child")
            .wait()
            .expect("kill fixture");
        assert_eq!(
            status.signal(),
            Some(9),
            "{scenario:?} kill at {point} #{target}"
        );
        recover_and_check(
            scenario,
            fresh.path(),
            &format!("{scenario:?} SIGKILL at {point} #{target}"),
        );
    }
}

fn libc_eperm() -> i32 {
    1
}

fn libc_enospc() -> i32 {
    28
}

/// effigy#447: admission may wait up to ten minutes for `Gate(Env)`, so the
/// window that settles an interrupted predecessor must start once the gate is
/// held. The hook runs with the gate held and before that settlement, so its
/// sleep stands in for a slow acquisition: the predecessor still settles on a
/// whole budget and the transition is admitted. On a window made before the
/// gate it refuses `TimedOut { phase: "settle" }`.
#[test]
fn a_slow_gate_acquisition_does_not_spend_the_predecessor_settlement() {
    let home = home();
    let root = root(home.path());
    // The settlement budget the slow acquisition spends entirely.
    let ctx = EnvContext::with_cleanup_budget(
        &root,
        CancelToken::default(),
        std::time::Duration::from_millis(700),
    )
    .unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    bring_up(&ctx, &stack, &fake, &closure("one"), Retention::Held, &[]).unwrap();
    // A transition whose process died before it settled: its gate is free and
    // its row is open, so the next admission settles it (K5).
    let abandoned = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).unwrap();
    let predecessor = abandoned.id();
    drop(abandoned);
    let slept = Rc::new(Cell::new(false));
    let slept_in = slept.clone();
    let admitted = with_hook(
        move |point| {
            if point == "GateAcquire" && !slept_in.replace(true) {
                // Longer than the settlement budget this admission was given.
                std::thread::sleep(std::time::Duration::from_millis(900));
            }
            Ok(())
        },
        || EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast),
    );
    assert!(slept.get(), "the slow acquisition was never exercised");
    let run = match admitted {
        Ok(run) => run,
        Err(error) => panic!("admission with predecessor {predecessor}: {error}"),
    };
    run.finish(true).unwrap();
    // No run of the owner is left open: the predecessor settled.
    assert_store_sound(&ctx);
}
