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
//! other.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
use std::time::Duration;

use anstyle::Style;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::theme::{is_ci_environment, resolve_color_enabled, OutputMode};

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
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Lines,
            2 => Self::Spinner,
            _ => Self::Silent,
        }
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
        let mode = sink.mode();
        let bar = (mode == StepMode::Spinner).then(|| sink.start_bar(label));
        let step = Self {
            inner: Arc::new(StepInner {
                label: Mutex::new(label.to_owned()),
                mode,
                sink: Arc::clone(&sink),
                bar,
                finished: AtomicBool::new(false),
                children: Mutex::new(Vec::new()),
            }),
        };
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
        if self.inner.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        self.finish_children();
        if let Some(bar) = self.inner.bar.as_ref() {
            bar.finish_and_clear();
        }
    }

    fn finish(&self, ok: bool, summary: &str) {
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
        self.multi.get_or_init(|| {
            MultiProgress::with_draw_target(ProgressDrawTarget::stderr())
        })
    }

    fn style_label(&self, label: &str) -> String {
        crate::style_text(self.color, self.styles.muted, label)
    }

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
            crate::style_text(self.color, style, mark),
            summary
        );
        // While spinners draw, a finish line has to be written between two
        // redraws, or a bar's escape sequence can land in the middle of it.
        match self.multi.get() {
            Some(multi) => multi.suspend(|| self.write_line(&line)),
            None => self.write_line(&line),
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

/// Start a top-level step. The label is short, present tense and names its
/// subject: `starting web`.
pub fn step(label: &str) -> Step {
    Step::new(Arc::clone(global_sink()), None, label)
}

/// Silence steps while another surface owns the terminal (the managed-run
/// TUI). Dropping the guard restores drawing.
pub fn suspend_terminal() -> TerminalGuard {
    let sink = Arc::clone(global_sink());
    sink.suspended.fetch_add(1, Ordering::SeqCst);
    if let Some(multi) = sink.multi.get() {
        multi.set_draw_target(ProgressDrawTarget::hidden());
    }
    TerminalGuard { sink }
}

/// Restores step drawing when dropped.
#[derive(Debug)]
pub struct TerminalGuard {
    sink: Arc<Sink>,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
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
    fn mode_resolution_keeps_json_and_pipes_silent() {
        // --json is silent wherever it runs.
        assert_eq!(StepMode::resolve(true, true, true, false), StepMode::Silent);
        assert_eq!(StepMode::resolve(true, false, true, false), StepMode::Silent);
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

    fn suspend_terminal_with(sink: &Arc<Sink>) -> TerminalGuard {
        sink.suspended.fetch_add(1, Ordering::SeqCst);
        TerminalGuard {
            sink: Arc::clone(sink),
        }
    }
}
