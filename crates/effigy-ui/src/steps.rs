//! One step-progress API for every command plane.
//!
//! A step names what is happening as it happens, and ends with one line. On a
//! terminal it is an asynchronous spinner on stderr; with `--json`, piped
//! output, CI or any non-terminal it draws nothing unless the operator asked
//! for verbose output, in which case it prints one plain line per step.
//!
//! Labels are short, present tense and name their subject: `starting web`,
//! `waiting for web to report healthy (attempt 4)`, `fetching bundle
//! decodelabs`. The label changes as the work moves on.
//!
//! ```no_run
//! let step = effigy_ui::steps::step("starting web");
//! step.set_label("waiting for web to report healthy (attempt 2)");
//! step.succeed("web is healthy");
//! ```
//!
//! Steps are safe to run concurrently: every step draws through one
//! process-wide sink, which serialises finish lines and never interleaves a
//! spinner with them. While the managed-run TUI owns the terminal,
//! [`suspend_terminal`] silences the sink so the two never draw over each
//! other. Nested Effigy processes in managed panes instead write bounded
//! plain start and finish lines.

use std::cell::RefCell;
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
use std::time::{Duration, Instant};

use anstyle::Style;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::theme::{is_ci_environment, resolve_color_enabled, OutputMode};

/// Product handoff for children whose output belongs to a managed TUI pane.
pub const MANAGED_PANE_ENV: &str = "EFFIGY_INTERNAL_MANAGED_PANE";

/// Whether this process writes feedback into a managed pane.
pub fn in_managed_pane() -> bool {
    std::env::var_os(MANAGED_PANE_ENV).is_some_and(|value| value == "1")
}

const TICK: Duration = Duration::from_millis(80);
const SPINNER_TEMPLATE: &str = "{spinner} {msg}";
const SUCCESS: &str = "✓";
const FAILURE: &str = "✗";

/// How a step draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
    /// Nothing is drawn: `--json`, piped output, CI and any non-terminal.
    Silent,
    /// One plain `✓`/`✗` line per finished step, for verbose non-terminal
    /// output where an animated spinner would be noise.
    Lines,
    /// Plain start and finish lines in a managed pane.
    Pane,
    /// An asynchronous spinner on stderr, plus one line per finished step.
    Spinner,
}

impl StepMode {
    /// Decide the mode once, at process start: JSON and non-terminal output
    /// stay silent unless verbose output asked for plain step lines.
    pub fn resolve(output_json: bool, verbose: bool, stderr_is_terminal: bool, ci: bool) -> Self {
        if output_json {
            return Self::Silent;
        }
        if stderr_is_terminal && !ci {
            return Self::Spinner;
        }
        if verbose {
            return Self::Lines;
        }
        Self::Silent
    }

    fn as_u8(self) -> u8 {
        match self {
            Self::Silent => 0,
            Self::Lines => 1,
            Self::Spinner => 2,
            Self::Pane => 3,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Lines,
            2 => Self::Spinner,
            3 => Self::Pane,
            _ => Self::Silent,
        }
    }
}

/// Shared, operation-specific progress state. The awaited object or probe is
/// always present, even before the operation has emitted its first event.
#[derive(Debug, Clone)]
pub struct ProgressSource {
    state: Arc<Mutex<ProgressState>>,
    started: Instant,
}

#[derive(Debug)]
struct ProgressState {
    awaiting: String,
    sub_step: Option<String>,
    count: Option<(u64, u64)>,
    event: Option<String>,
}

impl ProgressSource {
    /// Name the exact object or probe being awaited.
    pub fn waiting_for(awaiting: impl Into<String>) -> Self {
        Self {
            state: Arc::new(Mutex::new(ProgressState {
                awaiting: awaiting.into(),
                sub_step: None,
                count: None,
                event: None,
            })),
            started: Instant::now(),
        }
    }

    /// Name the exact object or probe now awaited, when the work moves on to
    /// something other than the one first named.
    pub fn waiting_on(&self, awaiting: impl Into<String>) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .awaiting = awaiting.into();
    }

    /// Update the current object or sub-step, known count and latest event.
    pub fn update(
        &self,
        sub_step: impl Into<String>,
        count: Option<(u64, u64)>,
        event: Option<String>,
    ) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.sub_step = Some(sub_step.into());
        state.count = count;
        if event.is_some() {
            state.event = event;
        }
    }

    /// The frame a live step draws after `elapsed`: the current sub-step, the
    /// known count, the latest event or the awaited object, then the elapsed
    /// wait. Public so a test can check frames on a fake clock.
    pub fn frame_at(&self, elapsed: Duration) -> String {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let mut label = state
            .sub_step
            .clone()
            .unwrap_or_else(|| format!("waiting for {}", state.awaiting));
        if let Some((done, total)) = state.count {
            label.push_str(&format!(" ({done}/{total})"));
        }
        if let Some(event) = &state.event {
            label.push_str(&format!("; last: {event}"));
        } else if state.sub_step.is_some() {
            label.push_str(&format!("; waiting for {}", state.awaiting));
        }
        label.push_str(&format!("; {}s elapsed", elapsed.as_secs()));
        label
    }

    fn frame(&self) -> String {
        self.frame_at(self.started.elapsed())
    }
}

/// One step's own state. Cloning a [`Step`] shares it, so a finish from any
/// clone is the step's single finish.
struct StepInner {
    label: Mutex<String>,
    mode: StepMode,
    sink: Arc<Sink>,
    bar: Option<ProgressBar>,
    finished: AtomicBool,
    children: Mutex<Vec<Weak<StepInner>>>,
    render_lock: Mutex<()>,
}

impl StepInner {
    fn render_live(&self, source: &ProgressSource, elapsed: Duration) -> bool {
        let _guard = self
            .render_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.finished.load(Ordering::SeqCst) {
            return false;
        }
        let label = source.frame_at(elapsed);
        *self.label.lock().unwrap_or_else(PoisonError::into_inner) = label.clone();
        if let Some(bar) = &self.bar {
            bar.set_message(self.sink.style_label(&label));
        }
        true
    }
}

/// A running step. Dropping it does not finish it; call [`Step::succeed`],
/// [`Step::fail`] or [`Step::finish_clear`] so the operator sees an ending.
#[derive(Clone)]
pub struct Step {
    inner: Arc<StepInner>,
}

impl std::fmt::Debug for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Step")
            .field("label", &self.label())
            .field("mode", &self.inner.mode)
            .finish()
    }
}

impl Step {
    fn new(sink: Arc<Sink>, parent: Option<&Step>, label: &str) -> Self {
        let mut active = sink.active.lock().unwrap_or_else(PoisonError::into_inner);
        let mode = sink.mode();
        let bar = (mode == StepMode::Spinner && !is_capturing() && !sink.is_suspended())
            .then(|| sink.start_bar(label));
        if mode == StepMode::Pane && !is_capturing() && !sink.is_suspended() {
            sink.write_line(&format!("Started: {label}\n"));
        }
        let step = Self {
            inner: Arc::new(StepInner {
                label: Mutex::new(label.to_owned()),
                mode,
                sink: Arc::clone(&sink),
                bar,
                finished: AtomicBool::new(false),
                children: Mutex::new(Vec::new()),
                render_lock: Mutex::new(()),
            }),
        };
        active.retain(|entry| entry.strong_count() > 0);
        active.push(Arc::downgrade(&step.inner));
        drop(active);
        if let Some(parent) = parent {
            parent
                .inner
                .children
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(Arc::downgrade(&step.inner));
        }
        step
    }

    fn new_live(sink: Arc<Sink>, parent: Option<&Step>, source: ProgressSource) -> Self {
        let step = Self::new(sink, parent, &source.frame());
        if step.inner.bar.is_some() {
            let weak = Arc::downgrade(&step.inner);
            std::thread::spawn(move || loop {
                std::thread::sleep(TICK);
                let Some(inner) = weak.upgrade() else { break };
                if !inner.render_live(&source, source.started.elapsed()) {
                    break;
                }
            });
        }
        step
    }

    /// Start a nested step driven by an operation's live state.
    pub fn live_step(&self, source: ProgressSource) -> Step {
        Step::new_live(Arc::clone(&self.inner.sink), Some(self), source)
    }

    /// The label the operator last saw for this step.
    pub fn label(&self) -> String {
        self.inner
            .label
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// What is happening now. Called while the work moves on.
    pub fn set_label(&self, label: &str) {
        if self.inner.finished.load(Ordering::SeqCst) {
            return;
        }
        *self
            .inner
            .label
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = label.to_owned();
        if let Some(bar) = self.inner.bar.as_ref() {
            bar.set_message(self.inner.sink.style_label(label));
        }
    }

    /// A nested step: a distinct line for the part of the work now running.
    #[cfg_attr(
        effigy_feedback_lint,
        deprecated(note = "feedback legacy spinner: migrate to a live progress source")
    )]
    pub fn step(&self, label: &str) -> Step {
        Step::new(Arc::clone(&self.inner.sink), Some(self), label)
    }

    /// The step finished successfully. `summary` names what now holds; an
    /// empty summary falls back to the step's label.
    pub fn succeed(&self, summary: &str) {
        self.finish(true, summary);
    }

    /// The step failed. `summary` says what is wrong, not which variant fired.
    pub fn fail(&self, summary: &str) {
        self.finish(false, summary);
    }

    /// Finished without a line: a step that resolved into other output.
    pub fn finish_clear(&self) {
        let _guard = self
            .inner
            .render_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.inner.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        self.finish_children();
        if let Some(bar) = self.inner.bar.as_ref() {
            bar.finish_and_clear();
        }
        if self.inner.mode == StepMode::Pane && !is_capturing() && !self.inner.sink.is_suspended() {
            self.inner
                .sink
                .write_line(&format!("Finished: {}\n", self.label()));
        }
    }

    fn finish(&self, ok: bool, summary: &str) {
        let _guard = self
            .inner
            .render_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.inner.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        self.finish_children();
        let summary = if summary.is_empty() {
            self.label()
        } else {
            summary.to_owned()
        };
        if let Some(bar) = self.inner.bar.as_ref() {
            bar.finish_and_clear();
        }
        if capture_finish(ok, &summary) {
            return;
        }
        self.inner.sink.write_finish(self.inner.mode, ok, &summary);
    }

    /// A parent that ends while a child still runs clears the child instead of
    /// leaving a stale spinner on the terminal.
    fn finish_children(&self) {
        let children = std::mem::take(
            &mut *self
                .inner
                .children
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for child in children {
            let Some(child) = child.upgrade() else {
                continue;
            };
            let _guard = child
                .render_lock
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if child.finished.swap(true, Ordering::SeqCst) {
                continue;
            }
            if let Some(bar) = child.bar.as_ref() {
                bar.finish_and_clear();
            }
        }
    }
}

/// The process-wide step sink.
struct Sink {
    mode: AtomicU8,
    color: bool,
    styles: Styles,
    multi: OnceLock<MultiProgress>,
    lines: Mutex<Box<dyn Write + Send>>,
    suspended: AtomicUsize,
    active: Mutex<Vec<Weak<StepInner>>>,
}

#[derive(Debug, Clone, Copy)]
struct Styles {
    success: Style,
    error: Style,
    muted: Style,
}

impl std::fmt::Debug for Sink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sink")
            .field("mode", &self.mode())
            .field("color", &self.color)
            .field("suspended", &self.suspended.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl Sink {
    fn new<W: Write + Send + 'static>(mode: StepMode, color: bool, lines: W) -> Arc<Self> {
        let theme = crate::theme::Theme::default();
        Arc::new(Self {
            mode: AtomicU8::new(mode.as_u8()),
            color,
            styles: Styles {
                success: theme.success,
                error: theme.error,
                muted: theme.muted,
            },
            multi: OnceLock::new(),
            lines: Mutex::new(Box::new(lines)),
            suspended: AtomicUsize::new(0),
            active: Mutex::new(Vec::new()),
        })
    }

    fn mode(&self) -> StepMode {
        StepMode::from_u8(self.mode.load(Ordering::SeqCst))
    }

    fn set_mode(&self, mode: StepMode) {
        self.mode.store(mode.as_u8(), Ordering::SeqCst);
    }

    fn is_suspended(&self) -> bool {
        self.suspended.load(Ordering::SeqCst) > 0
    }

    fn multi(&self) -> &MultiProgress {
        self.multi
            .get_or_init(|| MultiProgress::with_draw_target(ProgressDrawTarget::stderr()))
    }

    fn style_label(&self, label: &str) -> String {
        crate::style_text(self.color, self.styles.muted, label)
    }

    #[cfg_attr(
        effigy_feedback_lint,
        deprecated(note = "feedback legacy spinner: migrate to a live progress source")
    )]
    fn start_bar(&self, label: &str) -> ProgressBar {
        let bar = self.multi().add(ProgressBar::new_spinner());
        if let Ok(style) = ProgressStyle::with_template(SPINNER_TEMPLATE) {
            bar.set_style(style);
        }
        bar.set_message(self.style_label(label));
        bar.enable_steady_tick(TICK);
        bar
    }

    fn write_finish(&self, mode: StepMode, ok: bool, summary: &str) {
        if mode == StepMode::Silent || self.is_suspended() {
            return;
        }
        let (mark, style) = if ok {
            (SUCCESS, self.styles.success)
        } else {
            (FAILURE, self.styles.error)
        };
        let line = format!(
            "{} {}\n",
            crate::style_text(self.color && mode != StepMode::Pane, style, mark),
            summary
        );
        self.write_above_bars(&line);
    }

    /// A plain line, written between spinner redraws so a bar's escape
    /// sequence never lands in the middle of it.
    fn write_above_bars(&self, line: &str) {
        match self.multi.get() {
            Some(multi) => multi.suspend(|| self.write_line(line)),
            None => self.write_line(line),
        }
    }

    fn write_line(&self, line: &str) {
        let mut lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = lines.write_all(line.as_bytes());
        let _ = lines.flush();
    }
}

static MODE: AtomicU8 = AtomicU8::new(0);
static SINK: OnceLock<Arc<Sink>> = OnceLock::new();
static JSON_OUTPUT: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Finish lines a test asked this thread to record instead of draw.
    static CAPTURE: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

fn is_capturing() -> bool {
    CAPTURE.with(|capture| capture.borrow().is_some())
}

fn capture_finish(ok: bool, summary: &str) -> bool {
    let mark = if ok { SUCCESS } else { FAILURE };
    CAPTURE.with(|capture| {
        let mut capture = capture.borrow_mut();
        let Some(lines) = capture.as_mut() else {
            return false;
        };
        lines.push(format!("{mark} {summary}"));
        true
    })
}

/// Record the step lines this thread finishes instead of drawing them.
///
/// Tests use it to prove a command's step sequence and wording: `work` runs
/// with every step drawing into the returned lines, so nothing reaches the
/// terminal and no spinner is started. Production never calls this.
#[doc(hidden)]
pub fn capture_lines<T>(work: impl FnOnce() -> T) -> (T, Vec<String>) {
    CAPTURE.with(|capture| *capture.borrow_mut() = Some(Vec::new()));
    let value = work();
    let lines = CAPTURE
        .with(|capture| capture.borrow_mut().take())
        .unwrap_or_default();
    (value, lines)
}

fn global_sink() -> &'static Arc<Sink> {
    SINK.get_or_init(|| {
        let mode = StepMode::from_u8(MODE.load(Ordering::SeqCst));
        let color = resolve_color_enabled(OutputMode::from_env(), std::io::stderr().is_terminal());
        Sink::new(mode, color, std::io::stderr())
    })
}

/// Configure step drawing for this process. The CLI entrypoint calls this once
/// before any command runs; calling it again replaces the mode.
pub fn configure(mode: StepMode) {
    MODE.store(mode.as_u8(), Ordering::SeqCst);
    if let Some(sink) = SINK.get() {
        sink.set_mode(mode);
    }
}

/// Configure the mode from the terminal and the invocation's output mode.
pub fn configure_from_terminal(output_json: bool, verbose: bool) {
    JSON_OUTPUT.store(output_json, Ordering::SeqCst);
    if !output_json && in_managed_pane() {
        configure(StepMode::Pane);
        return;
    }
    configure(StepMode::resolve(
        output_json,
        verbose,
        std::io::stderr().is_terminal(),
        is_ci_environment(),
    ));
}

/// The mode steps draw in right now.
pub fn mode() -> StepMode {
    global_sink().mode()
}

/// Write one plain line to stderr, above any running spinner: a warning or a
/// note that is not a step's outcome. `--json` writes none; the envelope
/// carries what the operator needs.
pub fn print_line(line: &str) {
    print_line_to(global_sink(), JSON_OUTPUT.load(Ordering::SeqCst), line);
}

fn print_line_to(sink: &Sink, json: bool, line: &str) {
    if json {
        return;
    }
    sink.write_above_bars(&format!("{line}\n"));
}

/// Start a top-level step. The label is short, present tense and names its
/// subject: `starting web`.
#[cfg_attr(
    effigy_feedback_lint,
    deprecated(note = "feedback legacy spinner: migrate to a live progress source")
)]
pub fn step(label: &str) -> Step {
    Step::new(Arc::clone(global_sink()), None, label)
}

/// Start a top-level step driven by an operation's live state.
pub fn live_step(source: ProgressSource) -> Step {
    Step::new_live(Arc::clone(global_sink()), None, source)
}

/// One finished line for an outcome that resolved without a running step.
pub fn outcome_line(ok: bool, summary: &str) {
    if capture_finish(ok, summary) {
        return;
    }
    let sink = global_sink();
    sink.write_finish(sink.mode(), ok, summary);
}

/// Silence steps while another surface owns the terminal (the managed-run
/// TUI). Dropping the guard restores drawing.
pub fn suspend_terminal() -> TerminalGuard {
    let sink = Arc::clone(global_sink());
    {
        let _active = sink.active.lock().unwrap_or_else(PoisonError::into_inner);
        sink.suspended.fetch_add(1, Ordering::SeqCst);
        if let Some(multi) = sink.multi.get() {
            multi.set_draw_target(ProgressDrawTarget::hidden());
        }
    }
    TerminalGuard { sink }
}

/// Restores step drawing when dropped.
#[derive(Debug)]
pub struct TerminalGuard {
    sink: Arc<Sink>,
}

impl TerminalGuard {
    /// Retire the progress that preceded the TUI before shutdown takes over.
    pub fn retire_running(&self) {
        let mut active = self
            .sink
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for inner in active.drain(..).filter_map(|step| step.upgrade()) {
            if !inner.finished.swap(true, Ordering::SeqCst) {
                if let Some(bar) = &inner.bar {
                    bar.finish_and_clear();
                }
            }
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _active = self
            .sink
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.sink.suspended.fetch_sub(1, Ordering::SeqCst) == 1 {
            if let Some(multi) = self.sink.multi.get() {
                multi.set_draw_target(ProgressDrawTarget::stderr());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn live_frames_show_state_and_elapsed_until_finish() {
        let source = ProgressSource::waiting_for("web health probe on port 8080");
        let (sink, _) = sink(StepMode::Silent);
        let step = Step::new_live(sink, None, source.clone());
        assert!(step.inner.render_live(&source, Duration::from_secs(2)));
        assert_eq!(
            step.label(),
            "waiting for web health probe on port 8080; 2s elapsed"
        );
        source.update("checking web", Some((2, 3)), Some("HTTP 404".into()));
        assert!(step.inner.render_live(&source, Duration::from_secs(5)));
        assert_eq!(
            step.label(),
            "checking web (2/3); last: HTTP 404; 5s elapsed"
        );
        source.update(
            "checking database",
            Some((3, 3)),
            Some("connection refused".into()),
        );
        assert!(step.inner.render_live(&source, Duration::from_secs(7)));
        assert_eq!(
            step.label(),
            "checking database (3/3); last: connection refused; 7s elapsed"
        );
        step.finish_clear();
        assert!(!step.inner.render_live(&source, Duration::from_secs(8)));
        assert!(step.label().ends_with("7s elapsed"));
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
        let (sink, buffer) = sink(StepMode::Lines);
        let ok = Step::new(sink.clone(), None, "starting web");
        ok.set_label("waiting for web to report healthy");
        ok.succeed("web is healthy");
        let failed = Step::new(sink, None, "starting db");
        failed.fail("db refused connections on port 5432");
        assert_eq!(
            buffer.text(),
            "✓ web is healthy\n✗ db refused connections on port 5432\n"
        );
    }

    #[test]
    fn a_step_that_never_finished_wrote_nothing() {
        let (sink, buffer) = sink(StepMode::Lines);
        let running = Step::new(sink.clone(), None, "starting web");
        running.set_label("waiting for web to report healthy (attempt 4)");
        assert_eq!(buffer.text(), "");
        let cleared = Step::new(sink, None, "starting gateway");
        cleared.finish_clear();
        assert_eq!(buffer.text(), "");
    }

    #[test]
    fn finishing_twice_writes_one_line() {
        let (sink, buffer) = sink(StepMode::Lines);
        let step = Step::new(sink, None, "starting web");
        step.succeed("web is healthy");
        step.succeed("web is healthy again");
        step.fail("and now it failed");
        step.finish_clear();
        assert_eq!(buffer.text(), "✓ web is healthy\n");
    }

    #[test]
    fn an_empty_summary_falls_back_to_the_step_label() {
        let (sink, buffer) = sink(StepMode::Lines);
        let step = Step::new(sink, None, "starting web");
        step.set_label("web is healthy");
        step.succeed("");
        assert_eq!(buffer.text(), "✓ web is healthy\n");
    }

    #[test]
    fn nested_steps_finish_with_their_parent() {
        let (sink, buffer) = sink(StepMode::Lines);
        let parent = Step::new(sink, None, "starting web");
        let _child = parent.step("waiting for web to report healthy");
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
    fn pane_mode_writes_one_plain_start_and_finish() {
        let (sink, buffer) = sink(StepMode::Pane);
        let step = Step::new(sink, None, "running web");
        step.succeed("web finished");
        let text = buffer.text();
        assert_eq!(text, "Started: running web\n✓ web finished\n");
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
            let step = step("starting gateway");
            step.set_label("waiting for gateway to answer");
            step.succeed("gateway is running");
            let failed = step.step("starting DNS");
            failed.fail("DNS refused to bind port 53");
            let cleared = step.step("starting proxy");
            cleared.finish_clear();
            let unfinished = step.step("starting broker");
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
        let (_, first) = capture_lines(|| step("starting web").succeed("web is healthy"));
        assert_eq!(first, vec!["✓ web is healthy".to_owned()]);
        let (_, second) = capture_lines(|| step("starting web").fail("web is unhealthy"));
        assert_eq!(second, vec!["✗ web is unhealthy".to_owned()]);
    }

    fn suspend_terminal_with(sink: &Arc<Sink>) -> TerminalGuard {
        sink.suspended.fetch_add(1, Ordering::SeqCst);
        TerminalGuard {
            sink: Arc::clone(sink),
        }
    }
}
