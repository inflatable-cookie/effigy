use std::collections::VecDeque;
use std::time::Instant;

use crossterm::event::KeyEvent;

const MAX_TRACE_LINES: usize = 48;

#[derive(Debug, Clone)]
pub struct RuntimeDiagnostics {
    enabled: bool,
    started_at: Instant,
    frame_count: usize,
    keypress_count: usize,
    stdout_chunks: usize,
    stderr_chunks: usize,
    stdout_lines: usize,
    stderr_lines: usize,
    exit_events: usize,
    vt_resets: usize,
    traces: VecDeque<String>,
}

impl RuntimeDiagnostics {
    pub fn from_env() -> Self {
        let enabled = std::env::var("EFFIGY_TUI_DIAGNOSTICS")
            .ok()
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        Self {
            enabled,
            started_at: Instant::now(),
            frame_count: 0,
            keypress_count: 0,
            stdout_chunks: 0,
            stderr_chunks: 0,
            stdout_lines: 0,
            stderr_lines: 0,
            exit_events: 0,
            vt_resets: 0,
            traces: VecDeque::new(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.started_at.elapsed().as_millis()
    }

    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    pub fn keypress_count(&self) -> usize {
        self.keypress_count
    }

    pub fn stdout_chunks(&self) -> usize {
        self.stdout_chunks
    }

    pub fn stderr_chunks(&self) -> usize {
        self.stderr_chunks
    }

    pub fn stdout_lines(&self) -> usize {
        self.stdout_lines
    }

    pub fn stderr_lines(&self) -> usize {
        self.stderr_lines
    }

    pub fn exit_events(&self) -> usize {
        self.exit_events
    }

    pub fn vt_resets(&self) -> usize {
        self.vt_resets
    }

    pub fn traces(&self) -> Vec<String> {
        self.traces.iter().cloned().collect()
    }

    pub fn record_frame(&mut self) {
        if !self.enabled {
            return;
        }
        self.frame_count = self.frame_count.saturating_add(1);
    }

    pub fn record_keypress(&mut self, key: &KeyEvent) {
        if !self.enabled {
            return;
        }
        self.keypress_count = self.keypress_count.saturating_add(1);
        self.push_trace(format!(
            "key code={:?} modifiers={:?}",
            key.code, key.modifiers
        ));
    }

    pub fn record_stdout_chunk(&mut self, process: &str, size: usize) {
        if !self.enabled {
            return;
        }
        self.stdout_chunks = self.stdout_chunks.saturating_add(1);
        self.push_trace(format!("stdout-chunk process={process} bytes={size}"));
    }

    pub fn record_stderr_chunk(&mut self, process: &str, size: usize) {
        if !self.enabled {
            return;
        }
        self.stderr_chunks = self.stderr_chunks.saturating_add(1);
        self.push_trace(format!("stderr-chunk process={process} bytes={size}"));
    }

    pub fn record_stdout_lines(&mut self, count: usize) {
        if !self.enabled {
            return;
        }
        self.stdout_lines = self.stdout_lines.saturating_add(count);
    }

    pub fn record_stderr_lines(&mut self, count: usize) {
        if !self.enabled {
            return;
        }
        self.stderr_lines = self.stderr_lines.saturating_add(count);
    }

    pub fn record_exit_event(&mut self, process: &str, payload: &str) {
        if !self.enabled {
            return;
        }
        self.exit_events = self.exit_events.saturating_add(1);
        self.push_trace(format!("exit process={process} payload={}", payload.trim()));
    }

    pub fn record_vt_reset(&mut self, process: &str) {
        if !self.enabled {
            return;
        }
        self.vt_resets = self.vt_resets.saturating_add(1);
        self.push_trace(format!("vt-reset process={process}"));
    }

    fn push_trace(&mut self, line: String) {
        self.traces.push_back(line);
        while self.traces.len() > MAX_TRACE_LINES {
            self.traces.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn enabled() -> RuntimeDiagnostics {
        let mut diagnostics = RuntimeDiagnostics::from_env();
        diagnostics.enabled = true;
        // `from_env` owns the switch in production; the tests drive the
        // counters directly so a stray `EFFIGY_TUI_DIAGNOSTICS` in the
        // harness cannot silence them.
        diagnostics
    }

    #[test]
    fn counters_and_traces_stay_silent_until_enabled() {
        let mut diagnostics = RuntimeDiagnostics::from_env();
        diagnostics.enabled = false;
        diagnostics.record_frame();
        diagnostics.record_keypress(&KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        diagnostics.record_stdout_chunk("build", 12);
        diagnostics.record_stderr_chunk("build", 3);
        diagnostics.record_stdout_lines(4);
        diagnostics.record_stderr_lines(1);
        diagnostics.record_exit_event("build", "exit code 0");
        diagnostics.record_vt_reset("build");
        assert_eq!(diagnostics.frame_count(), 0);
        assert_eq!(diagnostics.keypress_count(), 0);
        assert_eq!(diagnostics.stdout_chunks(), 0);
        assert_eq!(diagnostics.stderr_chunks(), 0);
        assert_eq!(diagnostics.stdout_lines(), 0);
        assert_eq!(diagnostics.stderr_lines(), 0);
        assert_eq!(diagnostics.exit_events(), 0);
        assert_eq!(diagnostics.vt_resets(), 0);
        assert!(diagnostics.traces().is_empty());
    }

    #[test]
    fn enabled_counters_record_what_the_runtime_saw() {
        let mut diagnostics = enabled();
        diagnostics.record_frame();
        diagnostics.record_frame();
        diagnostics.record_keypress(&KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
        diagnostics.record_stdout_chunk("build", 40);
        diagnostics.record_stderr_chunk("build", 7);
        diagnostics.record_stdout_lines(3);
        diagnostics.record_stderr_lines(2);
        diagnostics.record_exit_event("build", "exit code 2\n");
        diagnostics.record_vt_reset("build");

        assert_eq!(diagnostics.frame_count(), 2);
        assert_eq!(diagnostics.keypress_count(), 1);
        assert_eq!(diagnostics.stdout_chunks(), 1);
        assert_eq!(diagnostics.stderr_chunks(), 1);
        assert_eq!(diagnostics.stdout_lines(), 3);
        assert_eq!(diagnostics.stderr_lines(), 2);
        assert_eq!(diagnostics.exit_events(), 1);
        assert_eq!(diagnostics.vt_resets(), 1);

        let traces = diagnostics.traces();
        assert_eq!(traces.len(), 5, "{traces:?}");
        assert!(traces[0].contains("key code=Char('j')"), "{traces:?}");
        assert_eq!(traces[1], "stdout-chunk process=build bytes=40");
        assert_eq!(traces[2], "stderr-chunk process=build bytes=7");
        assert_eq!(traces[3], "exit process=build payload=exit code 2");
        assert_eq!(traces[4], "vt-reset process=build");
        // The elapsed clock is the session's, not a per-record stamp.
        let _ = diagnostics.elapsed_ms();
    }

    #[test]
    fn the_trace_ring_keeps_only_the_latest_lines() {
        let mut diagnostics = enabled();
        for index in 0..(MAX_TRACE_LINES + 5) {
            diagnostics.record_stdout_chunk("build", index);
        }
        let traces = diagnostics.traces();
        assert_eq!(traces.len(), MAX_TRACE_LINES);
        assert!(
            traces[0].ends_with("bytes=5"),
            "the oldest five traces are dropped: {traces:?}"
        );
        assert!(
            traces[MAX_TRACE_LINES - 1].ends_with("bytes=52"),
            "{traces:?}"
        );
    }
}
