//! Frame-by-frame TUI assertions over the deterministic ordering graph: one
//! environment, `api`, and the two entries that depend on it (effigy#458).
//!
//! The states come from `waits::EntryWaits`, so the frames show exactly what
//! the session derived; no process, terminal or runtime is involved.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use crate::core::InputMode;
use crate::multiprocess::render::{render_ui, RenderUiState};
use crate::multiprocess::state::{ProcessStartupState, SessionState};
use crate::waits::{EntryPhase, EntryState, EntryWaits, OrderEntry, SessionOrder};

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const TABS: [&str; 4] = ["lifecycle", "api", "front", "admin"];

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

/// Renders one frame with `active` selected and returns the whole screen.
fn render_frame(
    waits: &EntryWaits,
    active: &str,
    startup_states: &HashMap<String, ProcessStartupState>,
    now: Instant,
) -> String {
    let names = TABS
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let active_index = names
        .iter()
        .position(|name| name == active)
        .expect("the active tab exists");
    let entry_waits = waits
        .states(now)
        .into_iter()
        .map(|state| (state.entry.clone(), state))
        .collect::<HashMap<String, EntryState>>();
    let state = SessionState::new(PathBuf::from("/fixture"), names.to_vec(), 40, 120, 500);
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("test terminal");
    let active_entry_wait = entry_waits.get(active);
    terminal
        .draw(|frame| {
            render_ui(
                frame,
                RenderUiState {
                    process_names: &names,
                    active_index,
                    active_logs: &[],
                    scroll_offset: 0,
                    max_offset: 0,
                    render_scroll_offset: 0,
                    scrollbar_total: 0,
                    follow: true,
                    active_process: active,
                    active_startup_state: startup_states
                        .get(active)
                        .copied()
                        .unwrap_or(ProcessStartupState::Running),
                    active_vt: false,
                    input_line: "",
                    input_mode: InputMode::Command,
                    shell_capture_mode: false,
                    exit_states: &state.exit_states,
                    startup_states,
                    entry_waits: &entry_waits,
                    active_entry_wait,
                    show_help: false,
                    show_options: false,
                    options_index: 0,
                    footer_message: None,
                    active_output_seen: false,
                    spinner_tick: 3,
                    active_elapsed: Duration::from_secs(1),
                    active_restart_count: 0,
                    shell_cursor: None,
                },
            )
        })
        .expect("the frame renders");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn assert_no_spinner_or_generic_wait(frame: &str) {
    for glyph in SPINNER_FRAMES {
        assert!(
            !frame.contains(glyph),
            "a waiting tab shows no spinner:\n{frame}"
        );
    }
    assert!(!frame.contains("waiting for startup delay"), "{frame}");
}

fn assert_tab_labels(frame: &str, expected: &[&str]) {
    for label in expected {
        assert!(frame.contains(label), "frame shows `{label}`:\n{frame}");
    }
}

/// The environment reports three of four containers healthy; `api` waits for
/// it, and both dependents wait for `api`. Every tab says its own thing.
#[test]
fn waiting_tabs_name_their_own_dependency_and_phase() {
    let now = Instant::now();
    let mut waits = EntryWaits::new(four_entry_order(), Some("app"), now);
    waits.environment_progress(3, 4);
    waits.refresh(now);
    let startup = HashMap::from([
        ("api".to_owned(), ProcessStartupState::Waiting),
        ("front".to_owned(), ProcessStartupState::Waiting),
        ("admin".to_owned(), ProcessStartupState::Waiting),
    ]);

    let frame = render_frame(&waits, "api", &startup, now + Duration::from_secs(2));
    assert_tab_labels(
        &frame,
        &[
            "api [waiting: environment]",
            "front [waiting: api waiting]",
            "admin [waiting: api waiting]",
        ],
    );
    assert!(
        frame.contains("waiting for environment (containers 3/4 healthy)"),
        "{frame}"
    );
    assert_no_spinner_or_generic_wait(&frame);

    // `front`'s tab names `front` and what `front` waits for, never api's
    // environment wait.
    let frame = render_frame(&waits, "front", &startup, now + Duration::from_secs(2));
    assert!(frame.contains("front [waiting: api waiting]"), "{frame}");
    assert!(frame.contains("waiting for api (waiting)"), "{frame}");
    assert!(!frame.contains("waiting for environment"), "{frame}");
    assert_no_spinner_or_generic_wait(&frame);
}

/// `api` starts, then waits on its readiness probe with elapsed seconds; the
/// dependents still name `api` starting. Then `api` is ready, and both
/// dependents start.
#[test]
fn each_transition_updates_only_its_own_tab() {
    let now = Instant::now();
    let mut waits = EntryWaits::new(four_entry_order(), Some("app"), now);
    waits.environment_ready(now);
    waits.started("api", now);
    let startup = HashMap::from([
        ("api".to_owned(), ProcessStartupState::Starting),
        ("front".to_owned(), ProcessStartupState::Waiting),
        ("admin".to_owned(), ProcessStartupState::Waiting),
    ]);

    let frame = render_frame(&waits, "front", &startup, now + Duration::from_secs(1));
    assert_tab_labels(&frame, &["api [starting]", "front [waiting: api starting]"]);
    assert!(frame.contains("waiting for api (starting)"), "{frame}");
    assert_no_spinner_or_generic_wait(&frame);

    // api's own readiness probe, with the elapsed wait.
    waits.probing("api", "GET / -> 200", now);
    let frame = render_frame(&waits, "api", &startup, now + Duration::from_secs(12));
    assert!(
        frame.contains("api [starting: GET / -> 200 (12s)]"),
        "{frame}"
    );
    assert!(frame.contains("waiting for GET / -> 200 (12s)"), "{frame}");
    assert_no_spinner_or_generic_wait(&frame);
    // The dependents did not borrow api's probe.
    let frame = render_frame(&waits, "admin", &startup, now + Duration::from_secs(12));
    assert!(frame.contains("admin [waiting: api starting]"), "{frame}");
    assert!(
        frame.contains("waiting for api (starting) (12s)"),
        "admin's pane names api, not api's probe: {frame}"
    );
    assert!(!frame.contains("waiting for GET / -> 200"), "{frame}");

    // api is ready: both dependents start.
    waits.ready("api", now + Duration::from_secs(20));
    waits.started("front", now + Duration::from_secs(20));
    waits.started("admin", now + Duration::from_secs(20));
    let running = HashMap::from([
        ("api".to_owned(), ProcessStartupState::Running),
        ("front".to_owned(), ProcessStartupState::Starting),
        ("admin".to_owned(), ProcessStartupState::Starting),
    ]);
    let frame = render_frame(&waits, "front", &running, now + Duration::from_secs(21));
    assert_tab_labels(
        &frame,
        &["api [ready]", "front [starting]", "admin [starting]"],
    );
}

/// A failed prerequisite leaves every dependent terminal and attributed, in
/// the tab and in the pane.
#[test]
fn a_failed_prerequisite_shows_not_started_with_attribution() {
    let now = Instant::now();
    let mut waits = EntryWaits::new(four_entry_order(), Some("app"), now);
    waits.environment_ready(now);
    waits.started("api", now);
    waits.probing("api", "GET / -> 200", now);
    waits.failed("api", "failed (status 500)", now + Duration::from_secs(4));
    let startup = HashMap::from([
        ("api".to_owned(), ProcessStartupState::Failed),
        ("front".to_owned(), ProcessStartupState::Waiting),
        ("admin".to_owned(), ProcessStartupState::Waiting),
    ]);
    let frame = render_frame(&waits, "front", &startup, now + Duration::from_secs(5));
    assert_tab_labels(
        &frame,
        &[
            "api [failed]",
            "front [not started: api failed]",
            "admin [not started: api failed]",
        ],
    );
    assert!(
        frame.contains("not started — api failed (status 500)"),
        "{frame}"
    );
    assert_no_spinner_or_generic_wait(&frame);
    assert!(
        waits
            .states(now + Duration::from_secs(5))
            .iter()
            .filter(|state| state.entry != "api")
            .all(|state| {
                state.phase == EntryPhase::NotStarted && !waits.may_start(&state.entry)
            }),
        "no dependent may start"
    );
}

/// The accepted legacy delay shows what remains of it, not another entry's
/// label.
#[test]
fn a_legacy_delay_tab_names_the_delay() {
    let now = Instant::now();
    let order = SessionOrder::new(vec![OrderEntry {
        name: "jobs".to_owned(),
        waits_for: Vec::new(),
        waits_for_readiness: false,
        after_environment: false,
    }]);
    let mut waits = EntryWaits::new(order, None, now);
    waits.delaying("jobs", Duration::from_millis(1_200), now);
    let names = ["lifecycle".to_owned(), "jobs".to_owned()];
    let entry_waits = waits
        .states(now)
        .into_iter()
        .map(|state| (state.entry.clone(), state))
        .collect::<HashMap<String, EntryState>>();
    let startup = HashMap::from([("jobs".to_owned(), ProcessStartupState::Waiting)]);
    let state = SessionState::new(PathBuf::from("/fixture"), names.to_vec(), 40, 120, 500);
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("test terminal");
    terminal
        .draw(|frame| {
            render_ui(
                frame,
                RenderUiState {
                    process_names: &names,
                    active_index: 1,
                    active_logs: &[],
                    scroll_offset: 0,
                    max_offset: 0,
                    render_scroll_offset: 0,
                    scrollbar_total: 0,
                    follow: true,
                    active_process: "jobs",
                    active_startup_state: ProcessStartupState::Waiting,
                    active_vt: false,
                    input_line: "",
                    input_mode: InputMode::Command,
                    shell_capture_mode: false,
                    exit_states: &state.exit_states,
                    startup_states: &startup,
                    entry_waits: &entry_waits,
                    active_entry_wait: entry_waits.get("jobs"),
                    show_help: false,
                    show_options: false,
                    options_index: 0,
                    footer_message: None,
                    active_output_seen: false,
                    spinner_tick: 0,
                    active_elapsed: Duration::from_secs(1),
                    active_restart_count: 0,
                    shell_cursor: None,
                },
            )
        })
        .expect("the frame renders");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    assert!(text.contains("jobs [waiting: 2s delay]"), "{text}");
    assert!(text.contains("scheduled delay: 2s remaining"), "{text}");
    assert_no_spinner_or_generic_wait(&text);
}

/// The production host mapping: `SessionOrder::hosts` over the declared
/// `depends_on`, and the declared readiness request as the probe label. The
/// frames name `api`'s phase to each dependent and `api`'s own request to its
/// tab (effigy#458).
#[test]
fn host_depends_on_frames_name_the_reader_and_its_own_probe() {
    use crate::waits::{listener_probe_label, HostOrder, SessionOrder};

    let now = Instant::now();
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
            depends_on: &["api".to_owned()],
        },
    ]);
    let mut waits = EntryWaits::new(order, None, now);
    waits.started("api", now);
    waits.probing("api", listener_probe_label("/", 200), now);
    let startup = HashMap::from([
        ("api".to_owned(), ProcessStartupState::Starting),
        ("front".to_owned(), ProcessStartupState::Waiting),
        ("admin".to_owned(), ProcessStartupState::Waiting),
    ]);

    let frame = render_frame(&waits, "front", &startup, now + Duration::from_secs(12));
    assert_tab_labels(
        &frame,
        &[
            "api [starting: GET / -> 200 (12s)]",
            "front [waiting: api starting]",
            "admin [waiting: api starting]",
        ],
    );
    assert!(frame.contains("waiting for api (starting)"), "{frame}");
    assert_no_spinner_or_generic_wait(&frame);

    let frame = render_frame(&waits, "api", &startup, now + Duration::from_secs(12));
    assert!(frame.contains("waiting for GET / -> 200 (12s)"), "{frame}");

    // api reached readiness: the dependents no longer wait on it.
    waits.ready("api", now + Duration::from_secs(20));
    assert!(waits.may_start("front"), "front starts once api is ready");
    assert!(waits.may_start("admin"));
}
