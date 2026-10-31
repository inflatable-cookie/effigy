//! The deterministic ordering graph the session's waits are proven against:
//! one environment, `api`, and the two entries that depend on it. The clock
//! and the probes are fakes; nothing here starts a process or a runtime.

use std::time::{Duration, Instant};

use super::*;

/// The fake clock: every observation is measured from here.
struct Clock {
    base: Instant,
}

impl Clock {
    fn new() -> Self {
        Self {
            base: Instant::now(),
        }
    }

    fn at(&self, seconds: f64) -> Instant {
        self.base + Duration::from_secs_f64(seconds)
    }
}

/// `api` waits for the environment; `front` and `admin` wait for `api` to
/// serve. Every entry is its own tab.
fn four_entry_order() -> SessionOrder {
    SessionOrder::new(vec![
        OrderEntry {
            name: "api".to_owned(),
            waits_for: Vec::new(),
            waits_for_readiness: false,
            after_environment: true,
        },
        OrderEntry {
            name: "front".to_owned(),
            waits_for: vec!["api".to_owned()],
            waits_for_readiness: true,
            after_environment: false,
        },
        OrderEntry {
            name: "admin".to_owned(),
            waits_for: vec!["api".to_owned()],
            waits_for_readiness: true,
            after_environment: false,
        },
    ])
}

fn state_of<'a>(states: &'a [EntryState], entry: &str) -> &'a EntryState {
    states
        .iter()
        .find(|state| state.entry == entry)
        .unwrap_or_else(|| panic!("{entry} has a state"))
}

/// The graph, frame by frame: the environment reports its containers, `api`
/// starts and waits for the environment, `front`/`admin` wait for `api`, then
/// `api` waits on its own readiness probe with elapsed time, becomes ready,
/// and both dependents start.
#[test]
fn the_four_entry_graph_names_each_entry_s_own_wait_frame_by_frame() {
    let clock = Clock::new();
    let environment = "app";
    let mut waits = EntryWaits::new(four_entry_order(), Some(environment), clock.at(0.0));

    // Frame 1: the environment is coming up, three of four containers healthy.
    waits.environment_progress(3, 4);
    let api = waits.state("api", clock.at(1.0)).expect("api's state");
    assert_eq!(api.phase, EntryPhase::Waiting);
    assert_eq!(
        api.waiting_on,
        Some(WaitOn::Environment {
            healthy: Some(3),
            total: Some(4)
        })
    );
    assert_eq!(
        api.status_line(),
        "waiting for environment (containers 3/4 healthy) (1s)"
    );
    assert!(!waits.may_start("api"), "api waits for the environment");

    // Frame 2: `api` has been launched by the session and is still waiting for
    // the environment, so `front` and `admin` wait for `api` starting.
    waits.started("api", clock.at(2.0));
    waits.environment_progress(4, 4);
    waits.refresh(clock.at(2.0));
    let api = waits.state("api", clock.at(3.0)).expect("api's state");
    assert_eq!(api.phase, EntryPhase::Starting);
    assert_eq!(api.tab_label(), "api [starting]");
    let front = waits.state("front", clock.at(3.0)).expect("front's state");
    assert_eq!(front.phase, EntryPhase::Waiting);
    assert_eq!(
        front.waiting_on,
        Some(WaitOn::Entry {
            name: "api".to_owned(),
            phase: EntryPhase::Starting
        })
    );
    assert_eq!(front.tab_label(), "front [waiting: api starting]");
    assert_eq!(front.status_line(), "waiting for api (starting) (1s)");
    let admin = waits.state("admin", clock.at(3.0)).expect("admin's state");
    assert_eq!(admin.tab_label(), "admin [waiting: api starting]");
    // Each tab's label names its own entry, never another entry's label.
    assert!(front.tab_label().starts_with("front ["));
    assert!(admin.tab_label().starts_with("admin ["));
    assert!(!waits.may_start("front"), "front waits for api");
    assert!(!waits.may_start("admin"), "admin waits for api");

    // Frame 3: `api` now waits on its own readiness probe; the elapsed time is
    // the probe's, not the environment's.
    waits.probing("api", "GET / -> 200", clock.at(3.0));
    let api = waits.state("api", clock.at(15.0)).expect("api's state");
    assert_eq!(api.phase, EntryPhase::Starting);
    assert_eq!(
        api.waiting_on,
        Some(WaitOn::Probe {
            probe: "GET / -> 200".to_owned()
        })
    );
    assert_eq!(api.status_line(), "waiting for GET / -> 200 (12s)");
    assert_eq!(api.tab_label(), "api [starting: GET / -> 200 (12s)]");
    assert_eq!(
        waits
            .state("front", clock.at(15.0))
            .expect("front's state")
            .waiting_on,
        Some(WaitOn::Entry {
            name: "api".to_owned(),
            phase: EntryPhase::Starting
        }),
        "a dependent names api, not api's probe"
    );

    // Frame 4: `api` is ready; both dependents may start and say so.
    waits.ready("api", clock.at(20.0));
    let api = waits.state("api", clock.at(21.0)).expect("api's state");
    assert_eq!(api.phase, EntryPhase::Ready);
    assert_eq!(api.tab_label(), "api [ready]");
    assert_eq!(api.waiting_on, None);
    assert!(waits.may_start("front"), "front may start");
    assert!(
        waits.may_start("admin"),
        "this graph has no scheduler, so admin's own prerequisites decide"
    );
    waits.started("front", clock.at(21.0));
    waits.started("admin", clock.at(21.0));
    let states = waits.states(clock.at(22.0));
    assert_eq!(state_of(&states, "front").tab_label(), "front [starting]");
    assert_eq!(state_of(&states, "admin").tab_label(), "admin [starting]");
    assert_eq!(states.len(), 3);
}

/// A definitive prerequisite failure is terminal and attributed: every
/// dependent is `not started` with the dependency's own report, and none may
/// start.
#[test]
fn a_failed_prerequisite_stops_every_dependent_with_attribution() {
    let clock = Clock::new();
    let mut waits = EntryWaits::new(four_entry_order(), None, clock.at(0.0));
    waits.environment_ready(clock.at(0.5));
    waits.started("api", clock.at(1.0));
    waits.probing("api", "GET / -> 200", clock.at(1.0));
    assert!(!waits.may_start("front"));

    waits.failed("api", "failed (status 500)", clock.at(4.0));
    for name in ["front", "admin"] {
        let state = waits.state(name, clock.at(5.0)).expect("dependent state");
        assert_eq!(state.phase, EntryPhase::NotStarted, "{name} did not start");
        assert_eq!(
            state.not_started_because,
            Some(Attribution::new("api", "failed (status 500)")),
            "{name} names the prerequisite and its report"
        );
        assert_eq!(state.status_line(), "not started — api failed (status 500)");
        assert!(
            state.tab_label().contains("not started: api failed"),
            "{}",
            state.tab_label()
        );
        assert!(!waits.may_start(name), "{name} may never start");
    }
    assert_eq!(waits.not_started(), vec!["front", "admin"]);
    // The failed entry itself keeps its own report, not a dependent's.
    let api = waits.state("api", clock.at(5.0)).expect("api's state");
    assert_eq!(api.phase, EntryPhase::Failed);
    assert_eq!(api.status_line(), "failed: failed (status 500)");
    assert_eq!(api.not_started_because, None);
}

/// A workload that only needs its predecessor to have *started* is not stopped
/// by that predecessor's later nonzero exit: it may still be inside its own
/// scheduled delay, and it starts when the delay elapses (effigy#458).
#[test]
fn a_predecessor_exiting_nonzero_does_not_stop_a_start_only_dependent() {
    let clock = Clock::new();
    let order = SessionOrder::workloads(&[
        WorkloadOrder {
            name: "api",
            after: &[],
        },
        WorkloadOrder {
            name: "front",
            after: &[],
        },
        WorkloadOrder {
            name: "admin",
            after: &[],
        },
    ]);
    let mut waits = EntryWaits::new(order, None, clock.at(0.0));
    waits.started("api", clock.at(0.5));
    // front is inside its own scheduled delay when api exits 7, with
    // `fail_on_non_zero = false` the session keeps going.
    waits.delaying("front", Duration::from_millis(1_500), clock.at(0.5));
    waits.exited("api", 7, clock.at(0.6));

    let front = waits.state("front", clock.at(0.7)).expect("front's state");
    assert_eq!(
        front.phase,
        EntryPhase::Waiting,
        "front still starts after its delay"
    );
    assert_eq!(front.not_started_because, None);
    assert_eq!(
        front.waiting_on,
        Some(WaitOn::Delay {
            remaining_ms: 1_500
        })
    );
    assert!(
        waits.not_started().is_empty(),
        "no dependent was blocked by api's exit"
    );

    waits.delay_elapsed("front", clock.at(2.1));
    assert!(waits.may_start("front"));
}

/// A dependent that needs the readiness of an entry that exited before it was
/// ready is terminal, and so is everything downstream of it (effigy#458).
#[test]
fn an_exit_before_readiness_stops_readiness_dependents_transitively() {
    let clock = Clock::new();
    let order = SessionOrder::hosts(&[
        HostOrder {
            name: "api",
            depends_on: &[],
        },
        HostOrder {
            name: "front",
            depends_on: &["api".to_owned()],
        },
        HostOrder {
            name: "admin",
            depends_on: &["front".to_owned()],
        },
    ]);
    let mut waits = EntryWaits::new(order, None, clock.at(0.0));
    waits.started("api", clock.at(0.5));
    waits.exited("api", 7, clock.at(1.0));
    for name in ["front", "admin"] {
        let state = waits.state(name, clock.at(2.0)).expect("state");
        assert_eq!(state.phase, EntryPhase::NotStarted, "{name}");
        assert_eq!(
            state
                .not_started_because
                .as_ref()
                .map(|cause| cause.name.as_str()),
            Some("api"),
            "{name} attributes the failing prerequisite"
        );
    }
}

/// A dependency that exits non-zero before its dependents started is the same
/// definitive failure; an entry that only needs its predecessor to have
/// started is never un-started by that predecessor's later exit.
#[test]
fn an_exit_before_readiness_is_a_prerequisite_failure() {
    let clock = Clock::new();
    let mut waits = EntryWaits::new(four_entry_order(), None, clock.at(0.0));
    waits.environment_ready(clock.at(0.0));
    waits.started("api", clock.at(1.0));
    waits.exited("api", 500, clock.at(2.0));
    assert_eq!(
        waits
            .state("front", clock.at(3.0))
            .expect("front's state")
            .not_started_because,
        Some(Attribution::new("api", "exited with 500"))
    );

    // The workload graph only requires the predecessor's start.
    let workloads = SessionOrder::workloads(&[
        WorkloadOrder {
            name: "api",
            after: &[],
        },
        WorkloadOrder {
            name: "front",
            after: &[],
        },
    ]);
    let mut waits = EntryWaits::new(workloads, None, clock.at(0.0));
    waits.started("api", clock.at(1.0));
    waits.started("front", clock.at(1.0));
    waits.exited("api", 1, clock.at(2.0));
    let front = waits.state("front", clock.at(3.0)).expect("front's state");
    assert_eq!(front.phase, EntryPhase::Starting, "front already started");
    assert_eq!(front.not_started_because, None);
}

/// The accepted legacy `start_after_ms` names itself and what remains: it is
/// never rendered as another entry's label.
#[test]
fn a_legacy_delay_names_itself_not_another_entry() {
    let clock = Clock::new();
    let order = SessionOrder::workloads(&[
        WorkloadOrder {
            name: "api",
            after: &[],
        },
        WorkloadOrder {
            name: "jobs",
            after: &[],
        },
    ]);
    let mut waits = EntryWaits::new(order, None, clock.at(0.0));
    waits.started("api", clock.at(0.0));
    waits.delaying("jobs", Duration::from_millis(1_200), clock.at(0.0));
    let jobs = waits.state("jobs", clock.at(0.0)).expect("jobs' state");
    assert_eq!(jobs.phase, EntryPhase::Waiting);
    assert_eq!(
        jobs.waiting_on,
        Some(WaitOn::Delay {
            remaining_ms: 1_200
        })
    );
    assert_eq!(jobs.status_line(), "scheduled delay: 2s remaining");
    assert_eq!(jobs.tab_label(), "jobs [waiting: 2s delay]");
    assert!(!jobs.status_line().contains("api"));
    assert!(!waits.may_start("jobs"), "the delay still holds it");

    waits.delay_elapsed("jobs", clock.at(1.2));
    assert!(waits.may_start("jobs"), "the delay elapsed");
    assert_eq!(
        waits
            .state("jobs", clock.at(1.3))
            .expect("jobs' state")
            .waiting_on,
        None
    );
}

/// A host that declares no dependency of its own still has a real wait: the
/// plan launches entries one at a time, so it names that hold — a distinct
/// kind, never an invented dependency (effigy#458).
#[test]
fn an_independent_host_names_the_scheduler_hold() {
    use super::HostOrder;

    let clock = Clock::new();
    let order = SessionOrder::hosts(&[
        HostOrder {
            name: "api",
            depends_on: &[],
        },
        HostOrder {
            name: "jobs",
            depends_on: &[],
        },
    ]);
    let mut waits = EntryWaits::new(order, None, clock.at(0.0));
    waits.started("api", clock.at(1.0));
    waits.probing("api", "GET / -> 200", clock.at(1.0));
    waits.refresh(clock.at(1.0));

    let jobs = waits.state("jobs", clock.at(2.0)).expect("jobs' state");
    assert_eq!(jobs.phase, EntryPhase::Waiting);
    assert_eq!(
        jobs.waiting_on,
        Some(WaitOn::Launch {
            name: "api".to_owned(),
            phase: EntryPhase::Starting
        })
    );
    assert_eq!(
        jobs.waiting_on.as_ref().and_then(WaitOn::dependency),
        None,
        "the scheduler hold is not a dependency"
    );
    assert_eq!(
        jobs.status_line(),
        "waiting for api's launch (starting) (1s)"
    );
    assert_eq!(jobs.tab_label(), "jobs [waiting: api starting]");
    assert!(!waits.may_start("jobs"));

    // The plan moved past api: jobs launches without any dependency edge.
    waits.ready("api", clock.at(3.0));
    assert!(waits.may_start("jobs"));
    assert_eq!(
        waits
            .state("jobs", clock.at(4.0))
            .expect("jobs' state")
            .waiting_on,
        None
    );
}

/// The host graph maps `depends_on` to a readiness wait; the workload graph
/// maps the resolved start order to a start wait, and `after = ["environment"]`
/// to the environment phase.
#[test]
fn host_depends_on_and_workload_order_map_to_their_own_waits() {
    let hosts = SessionOrder::hosts(&[
        HostOrder {
            name: "api",
            depends_on: &[],
        },
        HostOrder {
            name: "front",
            depends_on: &["api".to_owned()],
        },
    ]);
    let front = hosts.get("front").expect("front's order");
    assert_eq!(front.waits_for, vec!["api".to_owned()]);
    assert!(front.waits_for_readiness, "host readers wait for readiness");
    assert!(
        front.after_environment,
        "a bring-up starts host processes after the environment reports ready"
    );
    assert!(
        !hosts.get("api").expect("api's order").waits_for_readiness
            || hosts.get("api").expect("api's order").waits_for.is_empty()
    );
    assert!(
        hosts.get("api").expect("api's order").waits_for.is_empty(),
        "api declares no dependency"
    );

    let declared = vec!["environment".to_owned()];
    let workloads = SessionOrder::workloads(&[
        WorkloadOrder {
            name: "api",
            after: &declared,
        },
        WorkloadOrder {
            name: "front",
            after: &declared,
        },
    ]);
    let front = workloads.get("front").expect("front's order");
    assert_eq!(front.waits_for, vec!["api".to_owned()]);
    assert!(
        !front.waits_for_readiness,
        "a workload starts, then the next"
    );
    assert!(front.after_environment);
    assert!(workloads
        .get("api")
        .expect("api's order")
        .waits_for
        .is_empty());
}

/// The environment's own failure is attributed the same way, and the JSON
/// record carries the machine-readable identity and state.
#[test]
fn the_published_record_carries_waiting_on_and_not_started_because() {
    let clock = Clock::new();
    let mut waits = EntryWaits::new(four_entry_order(), Some("app"), clock.at(0.0));
    waits.environment_failed("`up` exited 1 after 4s", clock.at(4.0));
    let api = waits.state("api", clock.at(5.0)).expect("api's state");
    assert_eq!(
        api.not_started_because,
        Some(Attribution::new("environment", "`up` exited 1 after 4s"))
    );
    let record = serde_json::to_value(&api).expect("the record encodes");
    assert_eq!(record["entry"], "api");
    assert_eq!(record["phase"], "not_started");
    assert_eq!(
        record["not_started_because"]["name"], "environment",
        "{record}"
    );

    let mut waits = EntryWaits::new(four_entry_order(), Some("app"), clock.at(0.0));
    waits.environment_progress(2, 4);
    let api = waits.state("api", clock.at(1.0)).expect("api's state");
    let record = serde_json::to_value(&api).expect("the record encodes");
    assert_eq!(record["phase"], "waiting");
    assert_eq!(record["waiting_on"]["kind"], "environment");
    assert_eq!(record["waiting_on"]["healthy"], 2);
    assert_eq!(record["waiting_on"]["total"], 4);
    assert_eq!(record["elapsed_ms"], 1_000);

    let mut waits = EntryWaits::new(four_entry_order(), None, clock.at(0.0));
    waits.environment_ready(clock.at(0.0));
    waits.started("api", clock.at(1.0));
    let front = waits.state("front", clock.at(2.0)).expect("front's state");
    let record = serde_json::to_value(&front).expect("the record encodes");
    assert_eq!(record["waiting_on"]["kind"], "entry");
    assert_eq!(record["waiting_on"]["name"], "api");
    assert_eq!(record["waiting_on"]["phase"], "starting");
    // A record round-trips: every front end reads the same shape.
    let decoded: EntryState = serde_json::from_value(record).expect("the record decodes");
    assert_eq!(decoded, front);
}

/// A row of frames never shows one entry's label on another entry's tab, and a
/// waiting tab never falls back to a generic wait.
#[test]
fn every_frame_keeps_each_entry_s_label_its_own() {
    let clock = Clock::new();
    let mut waits = EntryWaits::new(four_entry_order(), Some("app"), clock.at(0.0));
    let frames = [
        clock.at(0.5),
        clock.at(1.0),
        clock.at(2.0),
        clock.at(9.0),
        clock.at(30.0),
    ];
    for (index, frame) in frames.iter().enumerate() {
        if index == 1 {
            waits.started("api", *frame);
        }
        if index == 2 {
            waits.environment_progress(4, 4);
            waits.environment_ready(*frame);
        }
        if index == 3 {
            waits.probing("api", "GET / -> 200", *frame);
        }
        if index == 4 {
            waits.ready("api", *frame);
        }
        waits.refresh(*frame);
        let states = waits.states(*frame);
        for state in &states {
            let label = state.tab_label();
            assert!(
                label.starts_with(&format!("{} [", state.entry)),
                "frame {index}: {label}"
            );
            if state.phase == EntryPhase::Waiting {
                match state.waiting_on.as_ref() {
                    Some(wait) => assert!(
                        !matches!(wait, WaitOn::Entry { name, .. } if name == &state.entry),
                        "frame {index}: {} waits for itself",
                        state.entry
                    ),
                    None => assert!(
                        waits.may_start(&state.entry),
                        "frame {index}: {} waits for something it cannot name",
                        state.entry
                    ),
                }
            }
        }
    }
}

/// A writer publishes a state when its meaning changes, and refreshes only a
/// live wait: a started process or a settled record that merely ages is never
/// rewritten, so one owner-loop tick is not one ledger write (effigy#458).
#[test]
fn only_a_changed_meaning_or_a_live_wait_is_written() {
    use std::time::Duration;

    let now = base_clock();
    let started = EntryState {
        entry: "api".to_owned(),
        phase: EntryPhase::Starting,
        waiting_on: None,
        not_started_because: None,
        detail: None,
        elapsed_ms: 0,
    };
    assert!(should_publish(None, None, &started, now), "the first state");
    let aged = EntryState {
        elapsed_ms: 60_000,
        ..started.clone()
    };
    assert!(
        !should_publish(
            Some(&started),
            Some(now),
            &aged,
            now + Duration::from_secs(60)
        ),
        "a started process only ages"
    );

    let waiting = EntryState {
        phase: EntryPhase::Waiting,
        waiting_on: Some(WaitOn::Entry {
            name: "api".to_owned(),
            phase: EntryPhase::Starting,
        }),
        ..started.clone()
    };
    assert!(
        should_publish(Some(&started), Some(now), &waiting, now),
        "a transition"
    );
    assert!(
        !should_publish(
            Some(&waiting),
            Some(now),
            &waiting,
            now + Duration::from_millis(100)
        ),
        "a live wait keeps its refresh interval"
    );
    assert!(
        should_publish(Some(&waiting), Some(now), &waiting, now + WAIT_REFRESH),
        "a live wait refreshes its elapsed time"
    );
    let ready_dependency = EntryState {
        waiting_on: Some(WaitOn::Entry {
            name: "api".to_owned(),
            phase: EntryPhase::Ready,
        }),
        ..waiting.clone()
    };
    assert!(
        should_publish(
            Some(&waiting),
            Some(now),
            &ready_dependency,
            now + Duration::from_millis(1)
        ),
        "the dependency's phase is a change of meaning"
    );

    let not_started = EntryState {
        phase: EntryPhase::NotStarted,
        waiting_on: None,
        not_started_because: Some(Attribution::new("api", "failed (status 500)")),
        ..started.clone()
    };
    assert!(should_publish(Some(&waiting), Some(now), &not_started, now));
    assert!(
        !should_publish(
            Some(&not_started),
            Some(now),
            &not_started,
            now + Duration::from_secs(60)
        ),
        "a terminal state is written once"
    );
}

fn base_clock() -> Instant {
    Instant::now()
}
