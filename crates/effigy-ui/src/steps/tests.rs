//! Step-progress behaviour: frames, endings, streaming windows and terminal
//! hand-over, driven through the real sink with fake terminals.

use super::*;
use std::path::Path;

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Buffer {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).expect("utf8")
    }
}

impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn sink(mode: StepMode) -> (Arc<Sink>, Buffer) {
    let buffer = Buffer::default();
    (Sink::new(mode, false, buffer.clone()), buffer)
}

/// A spinner-mode sink whose bars draw nowhere, so the test reads only the
/// finish lines the operator would see between redraws.
fn silent_spinner_sink() -> (Arc<Sink>, Buffer) {
    let buffer = Buffer::default();
    let sink = Sink::new(StepMode::Spinner, false, buffer.clone());
    let _ = sink
        .multi
        .set(MultiProgress::with_draw_target(ProgressDrawTarget::hidden()));
    (sink, buffer)
}

#[test]
fn concise_frames_keep_counts_blockers_and_verbose_evidence() {
    let source = ProgressSource::new(
        ProgressFrame::default()
            .label("Waiting for web")
            .waiting_for("environment `web` to be free"),
    );
    assert_eq!(
        source.frame_at(Duration::from_millis(1999)),
        "Waiting for web"
    );
    assert_eq!(
        source.frame_at(Duration::from_secs(2)),
        "Waiting for web · 2s"
    );
    source.update(|_| {
        ProgressFrame::new("reconciling the backend")
            .count(Some((3, 7)))
            .label("Waiting for web")
            .detail_event("engine colima-nerdctl, context `effigy`, project `inventors`")
    });
    assert_eq!(
        source.frame_at(Duration::from_secs(21)),
        "Waiting for web (3/7) · 21s"
    );
    assert!(source
        .detailed_frame_at(Duration::from_secs(21))
        .contains("engine colima-nerdctl"));
    source.blocker("run abc holds the gate", "held by `effigy dev` (pid 9012)");
    assert_eq!(
        source.frame_at(Duration::from_secs(3)),
        "Waiting for web (3/7) · held by `effigy dev` (pid 9012) · 3s"
    );
    source.update(|_| {
        ProgressFrame::default()
            .waiting_for("web health check")
            .label("Waiting for web")
            .detail_event("last check: HTTP 404")
            .event_label("last check: HTTP 404")
    });
    assert_eq!(
        source.frame_at(Duration::from_secs(12)),
        "Waiting for web · last check: HTTP 404 · 12s"
    );
    source.update(|_| ProgressFrame::default().label("Waiting for web"));
    assert_eq!(source.frame_at(Duration::ZERO), "Waiting for web");
}

#[test]
fn phase_replacement_is_atomic_for_both_renderers() {
    fn phase(n: u64) -> ProgressFrame {
        ProgressFrame::new(format!("phase {n}"))
            .waiting_for(format!("probe {n}"))
            .label(format!("short {n}"))
            .count(Some((n, 2)))
            .detail_event(format!("evidence {n}"))
            .event_label(format!("blocker {n}"))
    }
    let source = ProgressSource::new(phase(0));
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for n in 0..2000 {
                source.update(|_| phase(n % 2));
            }
        });
        for _ in 0..2000 {
            let short = source.frame_at(Duration::ZERO);
            assert!(
                (0..2).any(|n| short == format!("short {n} ({n}/2) · blocker {n}")),
                "{short}"
            );
            let detail = source.detailed_frame_at(Duration::ZERO);
            assert!((0..2).any(|n| detail == format!("phase {n} ({n}/2); waiting for probe {n}; last: evidence {n}; 0s elapsed")), "{detail}");
        }
    });
    source.update(|_| ProgressFrame::default().waiting_for("next probe"));
    assert_eq!(source.frame_at(Duration::ZERO), "Waiting for next probe");
    assert_eq!(
        source.detailed_frame_at(Duration::ZERO),
        "waiting for next probe; 0s elapsed"
    );
}

#[test]
fn live_and_detailed_frames_preserve_step_state_until_finish() {
    let source = ProgressSource::waiting_for("web health probe on port 8080");
    let (sink, _) = sink(StepMode::Silent);
    let step = Step::new_live(sink, None, source.clone());
    assert!(step.inner.render_live(&source, Duration::from_secs(2)));
    assert_eq!(
        step.label(),
        "Waiting for web health probe on port 8080 · 2s"
    );
    source.update(|_| {
        ProgressFrame::new("checking web")
            .count(Some((2, 3)))
            .event(Some("HTTP 404".into()))
    });
    assert!(step.inner.render_live(&source, Duration::from_secs(5)));
    assert_eq!(step.label(), "checking web (2/3) · HTTP 404 · 5s");
    source.update(|_| {
        ProgressFrame::new("checking database")
            .count(Some((3, 3)))
            .event(Some("connection refused".into()))
    });
    assert!(step.inner.render_live(&source, Duration::from_secs(7)));
    assert_eq!(
        step.label(),
        "checking database (3/3) · connection refused · 7s"
    );
    step.finish_clear();
    assert!(!step.inner.render_live(&source, Duration::from_secs(8)));
    assert!(step.label().ends_with("· 7s"));
    let detailed = ProgressSource::waiting_for("the environment gate for `stack`");
    assert_eq!(
        detailed.detailed_frame_at(Duration::from_secs(1)),
        "waiting for the environment gate for `stack`; 1s elapsed"
    );
    detailed.update(|_| ProgressFrame::new("reconciling the backend").count(Some((1, 3))));
    detailed.event("run agq holds it (pid 9012, task `dev`)");
    assert_eq!(
        detailed.detailed_frame_at(Duration::from_secs(3)),
        "reconciling the backend (1/3); waiting for the environment gate for `stack`; last: run agq holds it (pid 9012, task `dev`); 3s elapsed"
    );
}

#[test]
fn mode_resolution_keeps_json_and_pipes_silent() {
    // --json is silent wherever it runs.
    assert_eq!(StepMode::resolve(true, true, true, false), StepMode::Silent);
    assert_eq!(
        StepMode::resolve(true, false, true, false),
        StepMode::Silent
    );
    // A terminal draws spinners; CI and pipes draw nothing without verbose.
    assert_eq!(
        StepMode::resolve(false, false, true, false),
        StepMode::Spinner
    );
    assert_eq!(
        StepMode::resolve(false, false, true, true),
        StepMode::Silent
    );
    assert_eq!(
        StepMode::resolve(false, false, false, false),
        StepMode::Silent
    );
    // A pipe with verbose output gets plain step lines.
    assert_eq!(
        StepMode::resolve(false, true, false, false),
        StepMode::Lines
    );
    assert_eq!(StepMode::resolve(false, true, false, true), StepMode::Lines);
}

#[test]
fn silent_mode_draws_nothing() {
    let (sink, buffer) = sink(StepMode::Silent);
    let step = Step::new(sink, None, "starting web");
    step.set_label("waiting for web to report healthy");
    step.succeed("web is healthy");
    step.fail("web is unhealthy");
    assert_eq!(buffer.text(), "");
}

#[test]
fn line_mode_writes_one_line_per_finished_step() {
    let (line_sink, buffer) = sink(StepMode::Lines);
    let ok = Step::new(line_sink.clone(), None, "starting web");
    ok.set_label("waiting for web to report healthy");
    ok.succeed("web is healthy");
    let failed = Step::new(line_sink, None, "starting db");
    failed.fail("db refused connections on port 5432");
    assert_eq!(
        buffer.text(),
        "✓ web is healthy\n✗ db refused connections on port 5432\n"
    );

    // A running or explicitly cleared step has no durable line.
    let (running_sink, buffer) = sink(StepMode::Lines);
    let running = Step::new(running_sink.clone(), None, "starting web");
    running.set_label("waiting for web to report healthy (attempt 4)");
    assert_eq!(buffer.text(), "");
    let cleared = Step::new(running_sink, None, "starting gateway");
    cleared.finish_clear();
    assert_eq!(buffer.text(), "");

    // The first terminal outcome wins; later finish calls do not append.
    let (idempotent_sink, buffer) = sink(StepMode::Lines);
    let step = Step::new(idempotent_sink, None, "starting web");
    step.succeed("web is healthy");
    step.succeed("web is healthy again");
    step.fail("and now it failed");
    step.finish_clear();
    assert_eq!(buffer.text(), "✓ web is healthy\n");

    // An empty summary falls back to the latest user-facing label.
    let (fallback_sink, buffer) = sink(StepMode::Lines);
    let step = Step::new(fallback_sink, None, "starting web");
    step.set_label("web is healthy");
    step.succeed("");
    assert_eq!(buffer.text(), "✓ web is healthy\n");
}

#[test]
fn nested_steps_finish_with_their_parent() {
    let (sink, buffer) = sink(StepMode::Lines);
    let parent = Step::new(sink, None, "starting web");
    let _child = parent.live_step(ProgressSource::waiting_for(
        "web to report healthy on :8080",
    ));
    parent.succeed("web is healthy");
    assert_eq!(buffer.text(), "✓ web is healthy\n");
}

#[test]
fn concurrent_steps_never_interleave_a_line() {
    let (sink, buffer) = sink(StepMode::Lines);
    let mut handles = Vec::new();
    for index in 0..8 {
        let sink = Arc::clone(&sink);
        handles.push(std::thread::spawn(move || {
            let step = Step::new(sink, None, "starting web");
            step.succeed(&format!("service {index} {}", "ready ".repeat(40)));
        }));
    }
    for handle in handles {
        handle.join().expect("thread");
    }
    let text = buffer.text();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 8, "one line per step: {text}");
    for line in lines {
        assert!(line.starts_with("✓ service "), "whole line: {line}");
        assert_eq!(line.matches("ready").count(), 40);
    }
}

#[test]
fn producers_show_headers_and_timed_endings_without_spinners() {
    let (sink, buffer) = silent_spinner_sink();
    let source = ProgressSource::new(
        ProgressFrame::default()
            .label("Building Linux effigy")
            .waiting_for("the Linux build to exit"),
    );
    let step = Step::new_streaming(sink.clone(), source);
    assert!(sink.is_streaming());
    assert!(step.inner.bar.is_none());
    let child = Step::new(sink.clone(), None, "probing the builder");
    assert!(child.inner.bar.is_none());
    sink.write_line("Compiling effigy\n");
    step.succeed("Built Linux effigy");
    step.fail("must not print twice");
    assert!(!sink.is_streaming());
    let text = buffer.text();
    assert!(
        text.starts_with("Building Linux effigy…\nCompiling effigy\n✓ Built Linux effigy ("),
        "{text}"
    );
    assert_eq!(text.lines().count(), 3);
}

#[test]
fn nested_producers_restore_spinners_only_after_the_last_ends() {
    let (sink, _) = silent_spinner_sink();
    let first = Step::new_streaming(sink.clone(), ProgressSource::waiting_for("build"));
    let second = Step::new_streaming(sink.clone(), ProgressSource::waiting_for("install"));
    second.fail("Install failed");
    assert!(sink.is_streaming());
    drop(first);
    assert!(!sink.is_streaming());
}

#[test]
fn pane_mode_writes_one_plain_start_and_finish() {
    let (sink, buffer) = sink(StepMode::Pane);
    let step = Step::new(sink, None, "running web");
    step.succeed("web finished");
    let text = buffer.text();
    assert_eq!(text, "Started: running web\n✓ web finished\n");
}

#[test]
fn the_pane_fact_survives_the_names_a_child_can_carry() {
    // A captured child inherits the written marker, and an invocation
    // already carrying the first spelling is still a pane.
    for marked in [MANAGED_PANE_ENV, LEGACY_MANAGED_PANE_ENV] {
        let value = pane_marker([MANAGED_PANE_ENV, LEGACY_MANAGED_PANE_ENV], |name| {
            (name == marked).then(|| "1".into())
        });
        assert!(value, "{marked}");
    }
    // Anything but `1` is not the fact, and no marker at all is a real
    // terminal.
    for unmarked in [None, Some("0"), Some("true"), Some("")] {
        let value = pane_marker([MANAGED_PANE_ENV, LEGACY_MANAGED_PANE_ENV], |_| {
            unmarked.map(Into::into)
        });
        assert!(!value, "{unmarked:?}");
    }
}

#[test]
fn spinner_mode_ends_each_step_with_a_line() {
    let (sink, buffer) = silent_spinner_sink();
    let step = Step::new(sink.clone(), None, "fetching bundle decodelabs");
    step.set_label("fetching bundle decodelabs (2 of 3)");
    step.succeed("fetched bundle decodelabs");
    let failed = Step::new(sink, None, "starting web");
    failed.fail("web never answered on port 8080");
    assert_eq!(
        buffer.text(),
        "✓ fetched bundle decodelabs\n✗ web never answered on port 8080\n"
    );
}

#[test]
fn a_plain_line_lands_between_finishes_and_json_writes_none() {
    let (sink, buffer) = silent_spinner_sink();
    let step = Step::new(sink.clone(), None, "fetching bundle decodelabs main");
    print_line_to(
        &sink,
        false,
        "[bundle] kept bundle `decodelabs` at 1a2b3c4d",
    );
    step.succeed("fetched bundle `decodelabs`");
    print_line_to(&sink, true, "[bundle] this warning belongs in the envelope");
    assert_eq!(
        buffer.text(),
        "[bundle] kept bundle `decodelabs` at 1a2b3c4d\n✓ fetched bundle `decodelabs`\n"
    );
}

#[test]
fn a_suspended_terminal_silences_steps_and_restores_them() {
    let (sink, buffer) = silent_spinner_sink();
    let guard = suspend_terminal_with(&sink);
    let step = Step::new(sink.clone(), None, "starting web");
    step.succeed("web is healthy");
    drop(guard);
    let after = Step::new(sink, None, "starting web");
    after.succeed("web is healthy");
    assert_eq!(buffer.text(), "✓ web is healthy\n");
}

/// A fake terminal records each draw; manual ticks stand in for the clock.
#[derive(Debug, Clone, Default)]
struct FrameTerminal(Arc<Mutex<(String, Vec<String>)>>);

impl indicatif::TermLike for FrameTerminal {
    fn width(&self) -> u16 {
        120
    }
    fn move_cursor_up(&self, _: usize) -> std::io::Result<()> {
        Ok(())
    }
    fn move_cursor_down(&self, _: usize) -> std::io::Result<()> {
        Ok(())
    }
    fn move_cursor_right(&self, _: usize) -> std::io::Result<()> {
        Ok(())
    }
    fn move_cursor_left(&self, _: usize) -> std::io::Result<()> {
        Ok(())
    }
    fn write_line(&self, text: &str) -> std::io::Result<()> {
        self.write_str(text)
    }
    fn write_str(&self, text: &str) -> std::io::Result<()> {
        self.0.lock().unwrap().0.push_str(text);
        Ok(())
    }
    fn clear_line(&self) -> std::io::Result<()> {
        Ok(())
    }
    fn flush(&self) -> std::io::Result<()> {
        let mut state = self.0.lock().unwrap();
        let frame = std::mem::take(&mut state.0);
        state.1.push(frame);
        Ok(())
    }
}

#[test]
fn ctrl_c_retires_running_progress_before_shutdown_frames() {
    let (sink, _) = sink(StepMode::Spinner);
    let terminal = FrameTerminal::default();
    sink.multi()
        .set_draw_target(ProgressDrawTarget::term_like(Box::new(terminal.clone())));
    let running = Step::new(sink.clone(), None, "running dev");
    let bar = running.inner.bar.as_ref().unwrap();
    bar.disable_steady_tick();
    for _ in 0..3 {
        bar.tick();
        bar.force_draw();
    }
    assert!(terminal
        .0
        .lock()
        .unwrap()
        .1
        .iter()
        .any(|frame| frame.contains("running dev")));

    // Ctrl+C transfers ownership while the TUI is on screen.
    let guard = suspend_terminal_with(&sink);
    sink.multi().set_draw_target(ProgressDrawTarget::hidden());
    guard.retire_running();
    drop(guard);
    sink.multi()
        .set_draw_target(ProgressDrawTarget::term_like(Box::new(terminal.clone())));
    terminal.0.lock().unwrap().1.clear();
    let stopping = Step::new(sink.clone(), None, "stopping dev");
    let shutdown_bar = stopping.inner.bar.as_ref().unwrap();
    shutdown_bar.disable_steady_tick();
    for _ in 0..3 {
        bar.tick();
        shutdown_bar.tick();
        shutdown_bar.force_draw();
    }
    stopping.set_label("stopped dev");
    shutdown_bar.force_draw();
    stopping.succeed("stopped dev");
    running.set_label("running dev");
    bar.tick();
    let state = terminal.0.lock().unwrap();
    assert!(state.1.iter().any(|frame| frame.contains("stopping dev")));
    assert!(state.1.iter().any(|frame| frame.contains("stopped dev")));
    assert!(state.1.iter().all(|frame| !frame.contains("running dev")));
    assert!(state
        .1
        .iter()
        .all(|frame| frame.matches("stopping dev").count() <= 1));
    assert!(running.inner.finished.load(Ordering::SeqCst));
}

#[test]
fn capturing_records_the_finished_lines_and_draws_nothing() {
    let (value, lines) = capture_lines(|| {
        let step = live_step(ProgressSource::waiting_for("the gateway to answer"));
        step.succeed("gateway is running");
        let failed = step.live_step(ProgressSource::waiting_for("DNS to bind port 53"));
        failed.fail("DNS refused to bind port 53");
        let cleared = step.live_step(ProgressSource::waiting_for("the proxy to bind"));
        cleared.finish_clear();
        let unfinished = step.live_step(ProgressSource::waiting_for("the broker to start"));
        drop(unfinished);
        7
    });
    assert_eq!(value, 7);
    assert_eq!(
        lines,
        vec![
            "✓ gateway is running".to_owned(),
            "✗ DNS refused to bind port 53".to_owned(),
        ]
    );
    assert_eq!(mode(), StepMode::Silent);
}

#[test]
fn capturing_is_per_thread_and_ends_with_the_closure() {
    let (_, first) = capture_lines(|| {
        live_step(ProgressSource::waiting_for("web to start")).succeed("web is healthy")
    });
    assert_eq!(first, vec!["✓ web is healthy".to_owned()]);
    let (_, second) = capture_lines(|| {
        live_step(ProgressSource::waiting_for("web to start")).fail("web is unhealthy")
    });
    assert_eq!(second, vec!["✗ web is unhealthy".to_owned()]);
}

#[test]
fn a_live_window_bounds_its_lines_and_collapses_into_the_finish_line() {
    let (sink, buffer, terminal) = window_sink();
    let log = Path::new("/repo/.effigy/runtime/logs/web.compose.log");
    let (step, bar) = terminal_step(&sink);
    let live = LiveOutput::start(step.clone(), Some(log), 60);
    for index in 0..20 {
        live.observe(&format!("level=info creating layer {index}"));
    }
    live.observe(&format!("#42 {}", "extracting ".repeat(12)));
    bar.force_draw();
    let frame = last_frame(&terminal);
    assert_eq!(
        frame.matches("creating layer").count(),
        WINDOW_LINES - 1,
        "the window keeps the newest lines only: {frame}"
    );
    assert!(frame.contains("creating layer 19"), "{frame}");
    assert!(!frame.contains("creating layer 12"), "{frame}");
    assert!(
        frame.contains('…')
            && !frame.contains("extracting extracting extracting extracting extracting"),
        "a line wider than the window is cut: {frame}"
    );
    assert!(buffer.text().is_empty(), "nothing durable before the end");

    live.succeed("The stack is up");
    let text = buffer.text();
    assert!(
        text.starts_with(&format!("output log (last 64 KiB): {}\n", log.display())),
        "{text}"
    );
    let ending = text.lines().last().expect("the stage's own line");
    assert!(
        ending.starts_with("✓ The stack is up (") && ending.ends_with(')'),
        "the window collapses into the timed stage line: {text}"
    );
    terminal.0.lock().unwrap().1.clear();
    bar.force_draw();
    assert!(
        !last_frame(&terminal).contains("creating layer"),
        "a finished stage rewrites no window"
    );
}

#[test]
fn nerdctl_capability_warnings_are_counted_and_never_echoed() {
    let (sink, buffer, terminal) = window_sink();
    let log = Path::new("/repo/.effigy/runtime/logs/web.compose.log");
    let (step, bar) = terminal_step(&sink);
    let live = LiveOutput::start(step, Some(log), 100);
    live.observe(r#"level=warning msg="Ignoring: volume web_node_modules [Labels]""#);
    live.observe(r#"level=warning msg="Ignoring: volume web_cache [Driver Labels]""#);
    live.observe(r#"level=warning msg="Ignoring: service web depends_on: [db] [Required]""#);
    live.observe(r#"level=warning msg="a warning an operator needs""#);
    live.observe(r#"level=warning msg="Ignoring: service app healthcheck [Required]""#);
    bar.force_draw();
    let frame = last_frame(&terminal);
    assert!(
        !frame.contains("web_node_modules")
            && !frame.contains("web_cache")
            && !frame.contains("depends_on"),
        "known capability warnings are counted, not echoed: {frame}"
    );
    assert!(
        frame.contains("a warning an operator needs") && frame.contains("service app healthcheck"),
        "any other warning keeps its place in the window: {frame}"
    );

    live.succeed("The stack is up");
    let text = buffer.text();
    assert!(
        text.contains("nerdctl ignored 3 unsupported compose fields (see log)\n"),
        "one summary line counts the ignored fields: {text}"
    );
    assert!(text.contains(log.to_str().unwrap()), "{text}");
}

#[test]
fn a_failed_stage_prints_its_bounded_tail_the_log_path_and_one_failure_line() {
    let (sink, buffer, _terminal) = window_sink();
    let log = Path::new("/repo/.effigy/runtime/logs/web.compose.log");
    let (step, _bar) = terminal_step(&sink);
    let live = LiveOutput::start(step, Some(log), 100);
    for index in 0..40 {
        live.observe(&format!("runtime line {index}"));
    }
    live.fail(
        "The stack did not start: nerdctl compose up failed (exit status 1)\nstdout:\n\nstderr:\nno image",
    );
    let text = buffer.text();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("runtime line "))
            .count(),
        FAILURE_TAIL,
        "the failure keeps its own bounded tail: {text}"
    );
    assert!(text.contains("runtime line 39"), "{text}");
    assert!(!text.contains("runtime line 9\n"), "{text}");
    assert!(
        text.contains(&format!("output log (last 64 KiB): {}\n", log.display())),
        "{text}"
    );
    let ending = lines.last().expect("the failure line");
    assert!(
        ending
            .starts_with("✗ The stack did not start: nerdctl compose up failed (exit status 1) ("),
        "the failure line keeps its first sentence: {text}"
    );
    assert!(
        !text.contains("no image"),
        "the tail is not repeated: {text}"
    );
}

#[test]
fn a_piped_or_captured_terminal_gets_no_window() {
    // This binary draws nothing: no terminal, no window, and the caller
    // keeps its plain or JSON planning (the JSON envelope is unchanged).
    let (window, _) = capture_lines(|| {
        live_output(
            ProgressSource::waiting_for("docker compose up to return"),
            Some(Path::new("/repo/.effigy/runtime/logs/web.compose.log")),
        )
        .is_some()
    });
    assert!(!window);
}

#[test]
fn only_nerdctl_capability_warnings_match_the_ignored_form() {
    for line in [
        r#"level=warning msg="Ignoring: volume web_node_modules [Labels]""#,
        r#"level=warning msg="Ignoring: volume web_cache [Driver Labels]""#,
        r#"level=warning msg="Ignoring: service web depends_on: [db] [Required]""#,
    ] {
        assert!(nerdctl_ignored_compose_field(line), "{line}");
    }
    for line in [
        r#"level=warning msg="Ignoring: volume web_data""#,
        r#"level=warning msg="Ignoring: service web [Required]""#,
        r#"level=warning msg="a warning an operator needs""#,
        r#"level=info msg="Ignoring: volume web_node_modules [Labels]""#,
        "Ignoring: volume web_node_modules [Labels]",
    ] {
        assert!(!nerdctl_ignored_compose_field(line), "{line}");
    }
}

/// A spinner-mode sink whose bars draw into a fake terminal, and the sink's
/// own durable lines into a buffer.
fn window_sink() -> (Arc<Sink>, Buffer, FrameTerminal) {
    let (sink, buffer) = sink(StepMode::Spinner);
    let terminal = FrameTerminal::default();
    sink.multi()
        .set_draw_target(ProgressDrawTarget::term_like(Box::new(terminal.clone())));
    (sink, buffer, terminal)
}

/// A live stage step whose clock the test drives, never the wall clock.
fn terminal_step(sink: &Arc<Sink>) -> (Step, ProgressBar) {
    let source = ProgressSource::new(
        ProgressFrame::default()
            .label("Starting web stack")
            .waiting_for("docker compose up to return"),
    );
    let step = Step::new_live(Arc::clone(sink), None, source);
    let bar = step.inner.bar.as_ref().expect("a terminal step").clone();
    bar.disable_steady_tick();
    (step, bar)
}

fn last_frame(terminal: &FrameTerminal) -> String {
    terminal
        .0
        .lock()
        .unwrap()
        .1
        .last()
        .cloned()
        .unwrap_or_default()
}

fn suspend_terminal_with(sink: &Arc<Sink>) -> TerminalGuard {
    sink.suspended.fetch_add(1, Ordering::SeqCst);
    TerminalGuard {
        sink: Arc::clone(sink),
    }
}
