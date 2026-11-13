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

/// One step's own state. Cloning a [`Step`] shares it, so a finish from any
/// clone is the step's single finish.
pub(in crate::steps) struct StepInner {
    pub(in crate::steps) label: Mutex<String>,
    pub(in crate::steps) mode: StepMode,
    pub(in crate::steps) sink: Arc<Sink>,
    pub(in crate::steps) bar: Option<ProgressBar>,
    pub(in crate::steps) finished: AtomicBool,
    pub(in crate::steps) children: Mutex<Vec<Weak<StepInner>>>,
    pub(in crate::steps) render_lock: Mutex<()>,
    pub(in crate::steps) streaming: Mutex<Option<StreamingGuard>>,
    pub(in crate::steps) window: Mutex<Option<Window>>,
    pub(in crate::steps) started: Instant,
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
            source.detailed_frame_at(source.elapsed())
        } else {
            source.frame()
        };
        let step = Self::new(sink, parent, &label);
        if step.inner.bar.is_some() {
            let weak = Arc::downgrade(&step.inner);
            std::thread::spawn(move || loop {
                std::thread::sleep(TICK);
                let Some(inner) = weak.upgrade() else { break };
                if !inner.render_live(&source, source.elapsed()) {
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
pub(in crate::steps) struct Sink {
    pub(in crate::steps) mode: AtomicU8,
    pub(in crate::steps) color: bool,
    pub(in crate::steps) styles: Styles,
    pub(in crate::steps) multi: OnceLock<MultiProgress>,
    pub(in crate::steps) lines: Mutex<Box<dyn Write + Send>>,
    pub(in crate::steps) suspended: AtomicUsize,
    pub(in crate::steps) streaming: AtomicUsize,
    pub(in crate::steps) active: Mutex<Vec<Weak<StepInner>>>,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::steps) struct Styles {
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

/// The one producer-output window model, kept beside the streaming step that
/// draws it.
mod live;
/// The one progress-source spelling: the operation publishes frames, a step
/// renders them.
mod source;
/// Terminal hand-over for surfaces that own the terminal (the managed-run TUI).
mod terminal;

pub use live::{live_output, LiveOutput};
pub use source::{ProgressFrame, ProgressSource};
pub use terminal::{suspend_terminal, TerminalGuard};

pub(in crate::steps) use live::{StreamingGuard, Window};

#[cfg(test)]
pub(in crate::steps) use live::{nerdctl_ignored_compose_field, FAILURE_TAIL, WINDOW_LINES};

#[cfg(test)]
mod tests;
