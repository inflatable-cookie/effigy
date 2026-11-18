//! Terminal hand-over: a surface that owns the terminal (the managed-run TUI)
//! suspends step drawing, and dropping the guard restores it.

use std::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};

use indicatif::ProgressDrawTarget;

use super::{global_sink, Sink};

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

/// Run `work` with the step lines cleared from the terminal and every
/// spinner held until it returns: a prompt must be the only thing on the
/// terminal, or nobody knows what to answer (effigy#1036).
///
/// The progress display is not merely silenced: [`MultiProgress::suspend`]
/// erases the lines already drawn, blocks a tick or a finish line from drawing
/// underneath the prompt while `work` runs, and redraws the live steps
/// afterwards.
pub fn prompt_terminal<T>(work: impl FnOnce() -> T) -> T {
    let sink = Arc::clone(global_sink());
    prompt_terminal_with(&sink, work)
}

/// [`prompt_terminal`] for a caller that owns its own sink (tests).
pub(in crate::steps) fn prompt_terminal_with<T>(sink: &Arc<Sink>, work: impl FnOnce() -> T) -> T {
    match sink.multi.get() {
        Some(multi) => multi.suspend(work),
        // Nothing has ever drawn: there is no line to clear and no lock to
        // hold, so the prompt runs on its own.
        None => work(),
    }
}

/// Restores step drawing when dropped.
#[derive(Debug)]
pub struct TerminalGuard {
    pub(in crate::steps) sink: Arc<Sink>,
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
