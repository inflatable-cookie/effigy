use std::collections::VecDeque;
use std::time::Instant;

use vt100::Parser as VtParser;

use crate::core::LogEntry;

use super::SessionState;

impl SessionState {
    pub fn active_process(&self) -> &str {
        &self.process_names[self.active_index]
    }

    pub fn follow_for(&self, process: &str) -> bool {
        *self.follow_mode.get(process).unwrap_or(&true)
    }

    pub fn set_follow_for(&mut self, process: &str, value: bool) {
        self.follow_mode.insert(process.to_owned(), value);
    }

    pub fn scroll_offset_for(&self, process: &str) -> usize {
        *self.scroll_offsets.get(process).unwrap_or(&0usize)
    }

    pub fn set_scroll_offset_for(&mut self, process: &str, value: usize) {
        self.scroll_offsets.insert(process.to_owned(), value);
    }

    pub fn output_seen_for(&self, process: &str) -> bool {
        *self.output_seen.get(process).unwrap_or(&false)
    }

    pub fn mark_process_received_output(&mut self, process: &str) -> bool {
        let had_output = self.output_seen_for(process);
        self.output_seen.insert(process.to_owned(), true);
        self.restart_pending.insert(process.to_owned(), false);
        had_output
    }

    pub fn restart_pending_for(&self, process: &str) -> bool {
        *self.restart_pending.get(process).unwrap_or(&false)
    }

    pub fn clear_restart_pending_for(&mut self, process: &str) {
        self.restart_pending.insert(process.to_owned(), false);
    }

    pub fn process_started_at_for(&self, process: &str) -> Option<Instant> {
        self.process_started_at.get(process).copied()
    }

    pub fn restart_count_for(&self, process: &str) -> usize {
        *self.process_restart_count.get(process).unwrap_or(&0usize)
    }

    pub fn vt_saw_chunk_for(&self, process: &str) -> bool {
        *self.vt_saw_chunk.get(process).unwrap_or(&false)
    }

    pub fn set_vt_saw_chunk_for(&mut self, process: &str, value: bool) {
        self.vt_saw_chunk.insert(process.to_owned(), value);
    }

    pub fn vt_enabled_for(&self, process: &str) -> bool {
        self.vt_enabled_processes.contains(process)
    }

    pub fn logs_for(&self, process: &str) -> Option<&VecDeque<LogEntry>> {
        self.logs.get(process)
    }

    pub fn vt_parser_for(&self, process: &str) -> Option<&VtParser> {
        self.vt_parsers.get(process)
    }

    pub fn vt_parser_mut_for(&mut self, process: &str) -> Option<&mut VtParser> {
        self.vt_parsers.get_mut(process)
    }

    pub fn set_footer_message(&mut self, message: impl Into<String>) {
        self.footer_message = Some(message.into());
    }
}
