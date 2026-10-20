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
use std::cell::Cell;

const CHILD: &str = "EFFIGY_TEST_ENVIRONMENT_FAULT";
const CHILD_HOME: &str = "EFFIGY_TEST_ENVIRONMENT_HOME";
const CHILD_SCENARIO: &str = "EFFIGY_TEST_ENVIRONMENT_SCENARIO";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Up,
    Down,
    Reset,
    Import,
}

impl Scenario {
    const ALL: [Scenario; 4] = [Self::Up, Self::Down, Self::Reset, Self::Import];

    fn name(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Reset => "reset",
            Self::Import => "import",
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
            Self::Import => &[
                "GateAcquire",
                "BeginAdmitted",
                "EngineRecordBefore",
                "EngineRecordAfter",
                "AttachBefore",
                "AttachAfter",
            ],
        }
    }
}

fn log(home: &Path) -> PathBuf {
    home.join("backend-calls")
}

fn markers(home: &Path) -> PathBuf {
    home.join("legacy-markers")
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
        Scenario::Import => {
            let dir = markers(home);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("stack.persistent"), b"").unwrap();
            std::fs::write(dir.join("stack.complete"), b"").unwrap();
            std::fs::write(dir.join("stack.partial.json"), b"{\"schema\":\"legacy\"}").unwrap();
        }
    }
}

fn workloads(home: &Path) -> Observed<Vec<Workload>> {
    verified(vec![Workload {
        id: "a".repeat(64),
        name: "acme-stack-app-1".into(),
        running: true,
        labels: BTreeMap::from([
            ("com.docker.compose.project".into(), PROJECT.into()),
            ("com.docker.compose.service".into(), "app".into()),
            (
                crate::runtime::scope::CHECKOUT_LABEL.into(),
                home.display().to_string(),
            ),
        ]),
        binds: vec![],
    }])
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
        Scenario::Import => {
            let legacy = LegacyMarkers::read(&markers(home), "stack");
            let ownership = Ownership {
                checkout: home.canonicalize().unwrap(),
                scope: None,
            };
            import(
                &ctx,
                ImportRequest {
                    env: &stack,
                    project: PROJECT,
                    markers: &legacy,
                    workloads: workloads(home),
                    ownership: &ownership,
                    primary_service: "app",
                    backend: &selection(),
                },
            )
            .map(|_| ())
        }
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
            assert_eq!(
                stack.last_applied.as_deref(),
                Some(closure("one").digest()),
                "{label}"
            );
            assert_eq!(stack.partial(), None, "{label}: partial after recovery");
            // O5: one generation is applied at most once.
            assert_eq!(applies(home), 1, "{label}: backend apply repeated");
        }
        Scenario::Down | Scenario::Reset => {
            assert_eq!(stack.state, EnvState::Stopped, "{label}");
            assert_eq!(applies(home), 1, "{label}: a stop path reapplied");
            assert!(stops(home) >= 1, "{label}: never stopped");
            if scenario == Scenario::Reset {
                assert_eq!(stack.desired, None, "{label}: reset kept its render");
            }
        }
        Scenario::Import => {
            let engine = stack.engine().expect("imported record");
            assert_eq!(engine.retention, Retention::Persistent, "{label}");
            assert!(engine.imported.is_some(), "{label}");
            assert!(stack.active(), "{label}: adopted stack not active");
            // M3, M7: no backend call and the legacy files are untouched.
            assert_eq!(
                applies(home) + stops(home),
                0,
                "{label}: import reached the backend"
            );
            assert!(markers(home).join("stack.persistent").is_file(), "{label}");
            assert_eq!(
                std::fs::read(markers(home).join("stack.partial.json")).unwrap(),
                b"{\"schema\":\"legacy\"}",
                "{label}"
            );
        }
    }
}

#[test]
fn faults_reconciler_boundaries() {
    if let Ok(target) = std::env::var(CHILD) {
        let target: usize = target.parse().expect("crossing");
        let home = PathBuf::from(std::env::var_os(CHILD_HOME).expect("home"));
        let scenario = Scenario::parse(&std::env::var(CHILD_SCENARIO).expect("scenario"));
        let mut count = 0;
        let _ = with_hook(
            move |_| {
                if count == target {
                    let _ = std::process::Command::new("/bin/kill")
                        .args(["-KILL", &std::process::id().to_string()])
                        .status();
                    panic!("SIGKILL did not terminate the child");
                }
                count += 1;
                Ok(())
            },
            || operate(scenario, &home),
        );
        panic!("fault #{target} not delivered");
    }
    for scenario in Scenario::ALL {
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
                .args([
                    "--exact",
                    "environment::tests::faults::faults_reconciler_boundaries",
                    "--nocapture",
                ])
                .env(CHILD, target.to_string())
                .env(CHILD_HOME, fresh.path())
                .env(CHILD_SCENARIO, scenario.name())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .expect("kill fixture");
            use std::os::unix::process::ExitStatusExt;
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
}

fn libc_eperm() -> i32 {
    1
}

fn libc_enospc() -> i32 {
    28
}
