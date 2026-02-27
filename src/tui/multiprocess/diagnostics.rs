use std::collections::VecDeque;
use std::time::Instant;

use crossterm::event::KeyEvent;

const MAX_TRACE_LINES: usize = 48;

#[derive(Debug, Clone)]
pub(super) struct RuntimeDiagnostics {
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
    pub(super) fn from_env() -> Self {
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

    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn elapsed_ms(&self) -> u128 {
        self.started_at.elapsed().as_millis()
    }

    pub(super) fn frame_count(&self) -> usize {
        self.frame_count
    }

    pub(super) fn keypress_count(&self) -> usize {
        self.keypress_count
    }

    pub(super) fn stdout_chunks(&self) -> usize {
        self.stdout_chunks
    }

    pub(super) fn stderr_chunks(&self) -> usize {
        self.stderr_chunks
    }

    pub(super) fn stdout_lines(&self) -> usize {
        self.stdout_lines
    }

    pub(super) fn stderr_lines(&self) -> usize {
        self.stderr_lines
    }

    pub(super) fn exit_events(&self) -> usize {
        self.exit_events
    }

    pub(super) fn vt_resets(&self) -> usize {
        self.vt_resets
    }

    pub(super) fn traces(&self) -> Vec<String> {
        self.traces.iter().cloned().collect()
    }

    pub(super) fn record_frame(&mut self) {
        if !self.enabled {
            return;
        }
        self.frame_count = self.frame_count.saturating_add(1);
    }

    pub(super) fn record_keypress(&mut self, key: &KeyEvent) {
        if !self.enabled {
            return;
        }
        self.keypress_count = self.keypress_count.saturating_add(1);
        self.push_trace(format!(
            "key code={:?} modifiers={:?}",
            key.code, key.modifiers
        ));
    }

    pub(super) fn record_stdout_chunk(&mut self, process: &str, size: usize) {
        if !self.enabled {
            return;
        }
        self.stdout_chunks = self.stdout_chunks.saturating_add(1);
        self.push_trace(format!("stdout-chunk process={process} bytes={size}"));
    }

    pub(super) fn record_stderr_chunk(&mut self, process: &str, size: usize) {
        if !self.enabled {
            return;
        }
        self.stderr_chunks = self.stderr_chunks.saturating_add(1);
        self.push_trace(format!("stderr-chunk process={process} bytes={size}"));
    }

    pub(super) fn record_stdout_lines(&mut self, count: usize) {
        if !self.enabled {
            return;
        }
        self.stdout_lines = self.stdout_lines.saturating_add(count);
    }

    pub(super) fn record_stderr_lines(&mut self, count: usize) {
        if !self.enabled {
            return;
        }
        self.stderr_lines = self.stderr_lines.saturating_add(count);
    }

    pub(super) fn record_exit_event(&mut self, process: &str, payload: &str) {
        if !self.enabled {
            return;
        }
        self.exit_events = self.exit_events.saturating_add(1);
        self.push_trace(format!("exit process={process} payload={}", payload.trim()));
    }

    pub(super) fn record_vt_reset(&mut self, process: &str) {
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
