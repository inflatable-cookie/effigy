//! Producer streaming and the bounded live-output window.
//!
//! A streaming step hands the terminal to a producer's own output: the
//! spinner sinks disappear while it runs, and the window under the stage line
//! keeps only the newest bounded lines, collapsing into the ordinary finish
//! line.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use indicatif::ProgressDrawTarget;

use super::{global_sink, streams_output, ProgressSource, Sink, Step};

/// How many of a producer's latest lines the window shows at once.
pub(in crate::steps) const WINDOW_LINES: usize = 7;
/// How many retained lines a failed producer prints before its summary.
pub(in crate::steps) const FAILURE_TAIL: usize = 30;
/// The columns a producer line may use when the terminal width is unknown.
const DEFAULT_COLUMNS: usize = 100;

impl Step {
    pub(in crate::steps) fn new_streaming(sink: Arc<Sink>, source: ProgressSource) -> Self {
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

pub(in crate::steps) struct StreamingGuard {
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
pub(in crate::steps) struct Window {
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

    pub(in crate::steps) fn visible(&self) -> impl Iterator<Item = &String> {
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
    pub(in crate::steps) fn start(step: Step, log: Option<&Path>, columns: usize) -> Self {
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
pub(in crate::steps) fn nerdctl_ignored_compose_field(line: &str) -> bool {
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
