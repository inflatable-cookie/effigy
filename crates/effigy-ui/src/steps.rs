//! One step-progress API for every command plane.
//!
//! A step names what is happening as it happens, and ends with one line. On a
//! terminal it is an asynchronous spinner on stderr; with `--json`, piped
//! output, CI or any non-terminal it draws nothing unless the operator asked
//! for verbose output, in which case it prints one plain line per step.
//!
//! Labels are short, present tense and name their subject: `starting web`,
//! `fetching bundle decodelabs`. A step is driven by a [`ProgressSource`], so
//! its frames name the current sub-step or object and known counts. Meaningful
//! events explain blockers; elapsed time appears after two seconds. Diagnostic
//! evidence stays in verbose frames and plain logs. When work cannot report
//! more, the frame names the exact thing awaited. A canned label that never
//! moves is not progress and is not expressible (Q-017).
//!
//! ```no_run
//! use effigy_ui::steps::{live_step, ProgressFrame, ProgressSource};
//!
//! let source = ProgressSource::waiting_for("the web health probe on :8080");
//! let step = live_step(source.clone());
//! source.update(|_| ProgressFrame::new("waiting for web to report healthy").count(Some((2, 4))));
//! step.succeed("web is healthy");
//! ```
//!
//! Steps are safe to run concurrently: every step draws through one
//! process-wide sink, which serialises finish lines and never interleaves a
//! spinner with them. While the managed-run TUI owns the terminal,
//! [`suspend_terminal`] silences the sink so the two never draw over each
//! other. A process whose output Effigy captures and renders as plain lines -
//! a managed pane, or any captured run log - carries
//! [`MANAGED_PANE_ENV`] instead and writes bounded plain start and finish
//! lines, never spinner frames or the CLI banner (effigy#729).

use std::cell::RefCell;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
use std::time::{Duration, Instant};

use anstyle::Style;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::theme::{is_ci_environment, resolve_color_enabled, OutputMode};

/// Product fact for a child whose output Effigy captures and renders as plain
/// lines: a managed TUI pane, or any captured run log.
///
/// The fact is inherited, so every Effigy process a captured child starts - a
/// nested task run, a run step, a wrapper script's own `effigy` call - keeps
/// drawing bounded start and finish lines instead of spinner frames and the
/// CLI banner.
///
/// It deliberately sits outside the `EFFIGY_INTERNAL_` namespace. Effigy's own
/// child environment and consumer wrappers (the shipped bundle's
/// `profile_env.py` scrubs `EFFIGY_INTERNAL_*` and `EFFIGY_MANAGED_HOST_*`)
/// drop that namespace, and a nested child that loses the fact animates its
/// frames into a line-oriented log: a staircase of stale frames, repeated
/// waits and a nested banner (effigy#729).
pub const MANAGED_PANE_ENV: &str = "EFFIGY_MANAGED_PANE";

/// The first spelling of [`MANAGED_PANE_ENV`]. No longer written, still
/// honoured: an invocation already carrying it is inside a pane either way.
pub const LEGACY_MANAGED_PANE_ENV: &str = "EFFIGY_INTERNAL_MANAGED_PANE";

/// Whether this process writes feedback into a managed pane.
pub fn in_managed_pane() -> bool {
    pane_marker([MANAGED_PANE_ENV, LEGACY_MANAGED_PANE_ENV], |name| {
        std::env::var_os(name)
    })
}

/// The pane fact as a child's environment carries it: any known name set to
/// `1` marks the process, so a spelling this binary stopped writing still
/// counts.
fn pane_marker(names: [&str; 2], lookup: impl Fn(&str) -> Option<std::ffi::OsString>) -> bool {
    names
        .iter()
        .filter_map(|name| lookup(name))
        .any(|value| value == "1")
}

static VERBOSE: AtomicBool = AtomicBool::new(false);

const TICK: Duration = Duration::from_millis(80);
const SPINNER_TEMPLATE: &str = "{spinner} {msg}";
/// How many of a producer's latest lines the window shows at once.
const WINDOW_LINES: usize = 7;
/// How many retained lines a failed producer prints before its summary.
const FAILURE_TAIL: usize = 30;
/// The columns a producer line may use when the terminal width is unknown.
const DEFAULT_COLUMNS: usize = 100;
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
    state: Arc<Mutex<ProgressFrame>>,
    started: Instant,
}

/// A phase's complete presentation, built inside a source update.
/// An omitted wait keeps the operation's current awaited object; all other
/// evidence is replaced, so a new phase cannot inherit an old blocker.
#[derive(Debug, Default)]
pub struct ProgressFrame {
    awaiting: Option<String>,
    sub_step: Option<String>,
    count: Option<(u64, u64)>,
    event: Option<String>,
    concise_label: Option<String>,
    concise_event: Option<String>,
}

impl ProgressFrame {
    pub fn new(sub_step: impl Into<String>) -> Self {
        Self {
            sub_step: Some(sub_step.into()),
            ..Self::default()
        }
    }

    pub fn count(mut self, count: Option<(u64, u64)>) -> Self {
        self.count = count;
        self
    }

    pub fn event(mut self, event: Option<String>) -> Self {
        self.concise_event = event.clone();
        self.event = event;
        self
    }

    pub fn waiting_for(mut self, awaiting: impl Into<String>) -> Self {
        self.awaiting = Some(awaiting.into());
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.concise_label = Some(label.into());
        self
    }

    pub fn event_label(mut self, label: impl Into<String>) -> Self {
        self.concise_event = Some(label.into());
        self
    }

    /// Carry a still-current observation explicitly while the same wait advances.
    pub fn observations_from(mut self, previous: &Self) -> Self {
        self.event = previous.event.clone();
        self.concise_event = previous.concise_event.clone();
        self
    }

    pub fn detail_event(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());
        self.concise_event = None;
        self
    }
}

impl ProgressSource {
    /// Name the exact object or probe being awaited.
    pub fn waiting_for(awaiting: impl Into<String>) -> Self {
        Self::new(ProgressFrame::default().waiting_for(awaiting))
    }

    pub fn new(frame: ProgressFrame) -> Self {
        Self {
            state: Arc::new(Mutex::new(frame)),
            started: Instant::now(),
        }
    }

    /// Build and replace the phase, including concise and detailed evidence,
    /// under one lock. The builder must not call back into this source.
    pub fn update(&self, build: impl FnOnce(&ProgressFrame) -> ProgressFrame) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let mut frame = build(&state);
        if frame.awaiting.is_none() {
            frame.awaiting = state.awaiting.take();
        }
        *state = frame;
    }

    /// Publish an observation without changing the phase or its count.
    pub fn event(&self, event: impl Into<String>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let event = event.into();
        state.concise_event = Some(event.clone());
        state.event = Some(event);
    }

    /// Publish a blocker and its short terminal wording in one observation.
    pub fn blocker(&self, event: impl Into<String>, label: impl Into<String>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.event = Some(event.into());
        state.concise_event = Some(label.into());
    }

    /// Keep diagnostic evidence for verbose output without crowding the spinner.
    pub fn detail_event(&self, event: impl Into<String>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.event = Some(event.into());
        state.concise_event = None;
    }

    /// A concise terminal frame. Time earns a place after two seconds; a
    /// sub-step replaces the wait rather than repeating it.
    pub fn frame_at(&self, elapsed: Duration) -> String {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let awaiting = state.awaiting.as_deref().unwrap_or_default();
        let mut label = state
            .concise_label
            .clone()
            .or_else(|| state.sub_step.clone())
            .unwrap_or_else(|| format!("Waiting for {}", awaiting));
        if let Some((done, total)) = state.count {
            label.push_str(&format!(" ({done}/{total})"));
        }
        if let Some(event) = &state.concise_event {
            label.push_str(&format!(" · {event}"));
        }
        if elapsed >= Duration::from_secs(2) {
            label.push_str(&format!(" · {}s", elapsed.as_secs()));
        }
        label
    }

    /// What the step ends with when it succeeds: the sub-step last set, or
    /// the awaited object if the operation never named a sub-step.
    pub fn summary(&self) -> String {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state
            .sub_step
            .clone()
            .unwrap_or_else(|| state.awaiting.clone().unwrap_or_default())
    }

    /// The operator-visible label at `elapsed`. Tests use this as a fake clock.
    ///
    /// A frame always names what is still awaited once its sub-step is known,
    /// and the last event is added on top: an event says what just happened,
    /// never what the operation is waiting for, so publishing one cannot hide
    /// the awaited object for the rest of the phase (Q-017).
    pub fn detailed_frame_at(&self, elapsed: Duration) -> String {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let awaiting = state.awaiting.as_deref().unwrap_or_default();
        let mut label = state
            .sub_step
            .clone()
            .unwrap_or_else(|| format!("waiting for {}", awaiting));
        if let Some((done, total)) = state.count {
            label.push_str(&format!(" ({done}/{total})"));
        }
        if state.sub_step.is_some() && !awaiting.is_empty() {
            label.push_str(&format!("; waiting for {}", awaiting));
        }
        if let Some(event) = &state.event {
            label.push_str(&format!("; last: {event}"));
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
    streaming: Mutex<Option<StreamingGuard>>,
    window: Mutex<Option<Window>>,
    started: Instant,
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
        let label = if VERBOSE.load(Ordering::SeqCst) {
            source.detailed_frame_at(elapsed)
        } else {
            source.frame_at(elapsed)
        };
        *self.label.lock().unwrap_or_else(PoisonError::into_inner) = label.clone();
        self.set_bar_message(&label);
        true
    }

    /// One draw: the stage label, then the producer's window under it. The
    /// window is part of the same message, so the label, its elapsed time and
    /// the producer's latest lines are one frame and collapse together.
    fn set_bar_message(&self, label: &str) {
        let Some(bar) = &self.bar else {
            return;
        };
        let mut message = self.sink.style_label(label);
        let window = self.window.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(window) = window.as_ref() {
            for line in window.visible() {
                message.push('\n');
                message.push_str("  ");
                message.push_str(&self.sink.style_label(line));
            }
        }
        drop(window);
        bar.set_message(message);
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
        let bar = (mode == StepMode::Spinner
            && !is_capturing()
            && !sink.is_suspended()
            && !sink.is_streaming())
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
                streaming: Mutex::new(None),
                window: Mutex::new(None),
                started: Instant::now(),
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
        let label = if sink.mode() != StepMode::Spinner || VERBOSE.load(Ordering::SeqCst) {
            source.detailed_frame_at(source.started.elapsed())
        } else {
            source.frame()
        };
        let step = Self::new(sink, parent, &label);
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

    /// The legacy fixed-label nested step. It exists only as a lint anchor:
    /// `lint:feedback` fails on any use. Start a nested live step with
    /// [`Step::live_step`] instead.
    #[deprecated(
        note = "feedback legacy spinner: migrate to a live progress source; use `live_step(ProgressSource::waiting_for(\"the exact wait\"))` for work that can wait, or `finish_line` for an immediate outcome"
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
        self.inner
            .streaming
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
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
        let streaming = self
            .inner
            .streaming
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if streaming.is_some() {
            let mark = if ok { SUCCESS } else { FAILURE };
            self.inner.sink.write_line(&format!(
                "{mark} {summary} ({})\n",
                crate::human::human_duration(self.inner.started.elapsed())
            ));
            drop(streaming);
            return;
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
    streaming: AtomicUsize,
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
            streaming: AtomicUsize::new(0),
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

    fn is_streaming(&self) -> bool {
        self.streaming.load(Ordering::SeqCst) > 0
    }

    fn multi(&self) -> &MultiProgress {
        self.multi
            .get_or_init(|| MultiProgress::with_draw_target(ProgressDrawTarget::stderr()))
    }

    fn style_label(&self, label: &str) -> String {
        crate::style_text(self.color, self.styles.muted, label)
    }

    /// The one reviewed spinner sink. `clippy::disallowed_methods` forbids
    /// every other `ProgressBar::new_spinner` in the workspace, and the
    /// feedback inventory exempts exactly this construction.
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
        if mode == StepMode::Silent || self.is_suspended() || self.is_streaming() {
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
        Sink::new(mode, color, crate::output::ProcessStderr)
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
    VERBOSE.store(verbose, Ordering::SeqCst);
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

/// End an outcome that was immediate: one ✓/✗ line, no spinner. Use it where a
/// step would start and succeed in the same breath.
pub fn finish_line(ok: bool, summary: &str) {
    if capture_finish(ok, summary) {
        return;
    }
    let sink = global_sink();
    sink.write_finish(sink.mode(), ok, summary);
}

/// The legacy fixed-label top-level step. It exists only as a lint anchor:
/// `lint:feedback` fails on any use. Start a live step with [`live_step`], or
/// end an immediate outcome with [`finish_line`].
#[deprecated(
    note = "feedback legacy spinner: migrate to a live progress source; use `live_step(ProgressSource::waiting_for(\"the exact wait\"))` for work that can wait, or `finish_line` for an immediate outcome"
)]
pub fn step(label: &str) -> Step {
    Step::new(Arc::clone(global_sink()), None, label)
}

/// Start a top-level step driven by an operation's live state.
pub fn live_step(source: ProgressSource) -> Step {
    Step::new_live(Arc::clone(global_sink()), None, source)
}

/// Whether a human terminal can receive a producer's live output.
/// Nested producers remain eligible while another producer suppresses spinners.
pub fn streams_output() -> bool {
    mode() == StepMode::Spinner
        && !JSON_OUTPUT.load(Ordering::SeqCst)
        && !in_managed_pane()
        && !is_capturing()
        && !global_sink().is_suspended()
}

/// Show a producer's header and timed ending, leaving its own output visible.
/// Machine, piped and managed-pane feedback keeps its existing behavior.
pub fn streaming_step(source: ProgressSource) -> Step {
    if !streams_output() {
        return live_step(source);
    }
    Step::new_streaming(Arc::clone(global_sink()), source)
}

impl Step {
    fn new_streaming(sink: Arc<Sink>, source: ProgressSource) -> Self {
        let guard = StreamingGuard::new(Arc::clone(&sink));
        let label = source.frame_at(Duration::ZERO);
        let step = Self::new(Arc::clone(&sink), None, &label);
        sink.write_line(&format!("{label}…\n"));
        *step
            .inner
            .streaming
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(guard);
        step
    }
}

struct StreamingGuard {
    sink: Arc<Sink>,
}

impl StreamingGuard {
    fn new(sink: Arc<Sink>) -> Self {
        let _active = sink.active.lock().unwrap_or_else(PoisonError::into_inner);
        sink.streaming.fetch_add(1, Ordering::SeqCst);
        if let Some(multi) = sink.multi.get() {
            // Clear the current frame before handing the terminal to output.
            let _ = multi.clear();
            multi.set_draw_target(ProgressDrawTarget::hidden());
        }
        drop(_active);
        Self { sink }
    }
}

impl Drop for StreamingGuard {
    fn drop(&mut self) {
        if self.sink.streaming.fetch_sub(1, Ordering::SeqCst) == 1 && !self.sink.is_suspended() {
            if let Some(multi) = self.sink.multi.get() {
                multi.set_draw_target(ProgressDrawTarget::stderr());
            }
        }
    }
}

/// The producer lines one stage shows, newest last. The visible window is a
/// suffix of the retained tail, so a failure prints what the operator watched
/// without reading the log a second time.
struct Window {
    lines: std::collections::VecDeque<String>,
    columns: usize,
}

impl Window {
    fn new(columns: usize) -> Self {
        Self {
            lines: std::collections::VecDeque::new(),
            columns: columns.max(24),
        }
    }

    fn push(&mut self, line: &str) {
        self.lines
            .push_back(bound_producer_line(line, self.columns));
        while self.lines.len() > FAILURE_TAIL {
            self.lines.pop_front();
        }
    }

    fn visible(&self) -> impl Iterator<Item = &String> {
        self.lines
            .iter()
            .skip(self.lines.len().saturating_sub(WINDOW_LINES))
    }

    fn tail(&self, limit: usize) -> Vec<String> {
        self.lines
            .iter()
            .skip(self.lines.len().saturating_sub(limit))
            .cloned()
            .collect()
    }
}

/// One producer line as the window shows it: no terminal controls, no known
/// secret, and never wider than the window.
fn bound_producer_line(line: &str, columns: usize) -> String {
    let cleaned: String = line
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let redacted = crate::human::redact(cleaned.trim());
    let room = columns.saturating_sub(2).max(16);
    if redacted.chars().count() <= room {
        return redacted;
    }
    let mut cut: String = redacted.chars().take(room - 1).collect();
    cut.push('…');
    cut
}

/// A bounded live view of a producer's own output under its stage line
/// (effigy#925). #835's live output stays; this bounds what a terminal draws.
///
/// The window is the step's own message, drawn through the one sink and the
/// one spinner: about [`WINDOW_LINES`] width-truncated lines rewrite in place
/// under the stage label, the label's elapsed time keeps moving, and the whole
/// window collapses into the ordinary finish line. Nothing here reads the
/// producer a second time - [`LiveOutput::observe`] receives the same lines the
/// caller already follows - so there is one output path.
pub struct LiveOutput {
    step: Step,
    log: Option<PathBuf>,
    ignored: AtomicUsize,
}

/// Start a bounded output window under a stage line.
///
/// `None` when this terminal cannot show one: piped, CI, `--json`, a managed
/// pane or a capturing test keeps the caller's existing plain or JSON output.
/// `log` is the retained backend record the producer's stream is written to:
/// its last 64 KiB, with a visible marker when earlier output was dropped.
pub fn live_output(source: ProgressSource, log: Option<&Path>) -> Option<LiveOutput> {
    if !streams_output() {
        return None;
    }
    let step = Step::new_live(Arc::clone(global_sink()), None, source);
    Some(LiveOutput::start(step, log, window_columns()))
}

/// The columns a window may draw in. `COLUMNS` is the terminal's own answer
/// when it is set; otherwise a conventional width keeps lines from wrapping.
fn window_columns() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|columns| columns.trim().parse::<usize>().ok())
        .filter(|columns| *columns >= 40)
        .unwrap_or(DEFAULT_COLUMNS)
}

impl LiveOutput {
    fn start(step: Step, log: Option<&Path>, columns: usize) -> Self {
        *step
            .inner
            .window
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Window::new(columns));
        Self {
            step,
            log: log.map(Path::to_path_buf),
            ignored: AtomicUsize::new(0),
        }
    }

    /// One complete line the producer wrote. A known nerdctl capability
    /// warning is counted instead of echoed: it repeats for every unsupported
    /// Compose field and says nothing the operator can act on.
    pub fn observe(&self, line: &str) {
        if nerdctl_ignored_compose_field(line) {
            self.ignored.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.step.observe_producer_line(line);
    }

    /// The stage succeeded: the window collapses into the timed finish line.
    pub fn succeed(&self, summary: &str) {
        self.note_counted();
        self.note_log();
        self.step.succeed(&self.timed(summary));
    }

    /// The stage failed: the retained tail and the log's path, then the
    /// ordinary failure line.
    pub fn fail(&self, summary: &str) {
        let sink = &self.step.inner.sink;
        for line in self.step.producer_tail(FAILURE_TAIL) {
            sink.write_above_bars(&format!("  {line}\n"));
        }
        self.note_counted();
        self.note_log();
        self.step.fail(&self.timed(summary));
    }

    /// The operator stopped the stage: same bounded ending, no failure tail.
    pub fn interrupted(&self, summary: &str) {
        self.note_counted();
        self.note_log();
        self.step.fail(&self.timed(summary));
    }

    /// `summary (1.2s)`: the stage line says how long the work ran.
    fn timed(&self, summary: &str) -> String {
        format!(
            "{} ({})",
            headline(summary),
            crate::human::human_duration(self.step.inner.started.elapsed())
        )
    }

    fn note_counted(&self) {
        let count = self.ignored.load(Ordering::Relaxed);
        if count == 0 {
            return;
        }
        let plural = if count == 1 { "field" } else { "fields" };
        let pointer = if self.log.is_some() { " (see log)" } else { "" };
        self.note(&format!(
            "nerdctl ignored {count} unsupported compose {plural}{pointer}"
        ));
    }

    fn note_log(&self) {
        if let Some(log) = &self.log {
            // The record is bounded (#277): name what it actually holds rather
            // than promising a complete stream it may have trimmed (effigy#925).
            self.note(&format!("output log (last 64 KiB): {}", log.display()));
        }
    }

    /// One dim line above the window: a note that is not the stage's outcome.
    fn note(&self, line: &str) {
        let sink = &self.step.inner.sink;
        sink.write_above_bars(&format!("{}\n", sink.style_label(line)));
    }
}

/// The one-line form of a producer failure: the tail already carries the
/// detail, so a multi-line error keeps its first line.
fn headline(summary: &str) -> String {
    summary.lines().next().unwrap_or_default().trim().to_owned()
}

/// nerdctl's structured report of a Compose field it does not support. Only
/// the known capability warnings match: any other warning keeps its place in
/// the window.
fn nerdctl_ignored_compose_field(line: &str) -> bool {
    if !line.contains("level=warning") {
        return false;
    }
    let Some((_, message)) = line.split_once("msg=\"Ignoring: ") else {
        return false;
    };
    if message.starts_with("volume ") {
        return message.contains("[Labels]") || message.contains("[Driver Labels]");
    }
    message.starts_with("service ")
        && message.contains("depends_on")
        && message.contains("[Required]")
}

impl Step {
    /// Record one producer line in this step's window and redraw it.
    fn observe_producer_line(&self, line: &str) {
        let _guard = self
            .inner
            .render_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.inner.finished.load(Ordering::SeqCst) {
            return;
        }
        let label = self
            .inner
            .label
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        {
            let mut window = self
                .inner
                .window
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let Some(window) = window.as_mut() else {
                return;
            };
            window.push(line);
        }
        self.inner.set_bar_message(&label);
    }

    /// The last `limit` producer lines this step retained.
    fn producer_tail(&self, limit: usize) -> Vec<String> {
        self.inner
            .window
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|window| window.tail(limit))
            .unwrap_or_default()
    }
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
        if self.sink.suspended.fetch_sub(1, Ordering::SeqCst) == 1 && !self.sink.is_streaming() {
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
            frame.contains("a warning an operator needs")
                && frame.contains("service app healthcheck"),
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
            ending.starts_with(
                "✗ The stack did not start: nerdctl compose up failed (exit status 1) ("
            ),
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
}
