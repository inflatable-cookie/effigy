//! Terminal hand-over: a surface that owns the terminal (the managed-run TUI,
//! an interactive prompt, or a foreground child) suspends step drawing, and
//! dropping the guard restores it.

use std::io::{StderrLock, StdinLock, StdoutLock};
use std::process::Stdio;
use std::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};

use indicatif::ProgressDrawTarget;

use super::{global_sink, Sink};

/// Silence steps while another surface owns the terminal (the managed-run TUI
/// or a foreground child). Dropping the guard restores drawing.
pub fn suspend_terminal() -> TerminalGuard {
    suspend_terminal_with(global_sink())
}

/// [`suspend_terminal`] for a caller that owns its own sink (tests).
pub(in crate::steps) fn suspend_terminal_with(sink: &Arc<Sink>) -> TerminalGuard {
    {
        let _active = sink.active.lock().unwrap_or_else(PoisonError::into_inner);
        sink.suspended.fetch_add(1, Ordering::SeqCst);
        if let Some(multi) = sink.multi.get() {
            let _ = multi.clear();
            multi.set_draw_target(ProgressDrawTarget::hidden());
        }
    }
    TerminalGuard {
        sink: Arc::clone(sink),
    }
}

/// Run `work` with the step lines cleared from the terminal and every
/// spinner held until it returns: a prompt must be the only thing on the
/// terminal, or nobody knows what to answer (effigy#1036).
///
/// Acquires [`TerminalGuard`] for the entire closure (including across panics),
/// clearing any drawn spinner lines, hiding `MultiProgress`, and blocking any
/// concurrent thread from starting a new spinner or printing a finish line
/// while the prompt runs, even when `multi` was not yet initialized.
pub fn prompt_terminal<T>(work: impl FnOnce() -> T) -> T {
    let sink = Arc::clone(global_sink());
    prompt_terminal_with(&sink, work)
}

/// [`prompt_terminal`] for a caller that owns its own sink (tests).
pub(in crate::steps) fn prompt_terminal_with<T>(sink: &Arc<Sink>, work: impl FnOnce() -> T) -> T {
    let _guard = suspend_terminal_with(sink);
    work()
}

/// Run an interactive prompt that writes to stdout and reads from stdin while
/// step drawing is suspended.
pub fn prompt_terminal_stdio<T>(
    work: impl FnOnce(&mut StdinLock<'_>, &mut StdoutLock<'_>) -> T,
) -> T {
    prompt_terminal(|| {
        let mut stdin = std::io::stdin().lock();
        let mut stdout = std::io::stdout().lock();
        work(&mut stdin, &mut stdout)
    })
}

/// Run an interactive prompt that writes to stderr and reads from stdin while
/// step drawing is suspended.
pub fn prompt_terminal_stderr<T>(
    work: impl FnOnce(&mut StdinLock<'_>, &mut StderrLock<'_>) -> T,
) -> T {
    prompt_terminal(|| {
        let mut stdin = std::io::stdin().lock();
        let mut stderr = std::io::stderr().lock();
        work(&mut stdin, &mut stderr)
    })
}

/// Prompt for a password on the controlling terminal without echo while step
/// drawing is suspended.
pub fn prompt_terminal_password(prompt: &str) -> std::io::Result<String> {
    prompt_terminal(|| rpassword::prompt_password(prompt))
}

/// Restores step drawing when dropped.
#[derive(Debug)]
pub struct TerminalGuard {
    pub(in crate::steps) sink: Arc<Sink>,
}

impl TerminalGuard {
    /// Inherit the process's stdio for a child only while step drawing is
    /// suspended by this guard.
    pub fn inherit_stdio(&self) -> Stdio {
        Stdio::inherit()
    }

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
        let active = self
            .sink
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.sink.suspended.fetch_sub(1, Ordering::SeqCst) == 1 && !self.sink.is_streaming() {
            if let Some(multi) = self.sink.multi.get() {
                multi.set_draw_target(self.sink.restore_draw_target());
                for inner in active.iter().filter_map(|step| step.upgrade()) {
                    if !inner.finished.load(Ordering::SeqCst) {
                        if let Some(bar) = &inner.bar {
                            bar.force_draw();
                        }
                    }
                }
            }
        }
    }
}
