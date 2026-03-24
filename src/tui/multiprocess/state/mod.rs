use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use vt100::Parser as VtParser;

use crate::tui::core::{InputMode, LogEntry, ProcessExitState};

mod accessors;
mod init;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OptionsAction {
    ToggleFollow,
    Restart,
    Stop,
    Cancel,
    Quit,
}

pub(super) struct SessionState {
    pub(super) process_names: Vec<String>,
    pub(super) logs: HashMap<String, VecDeque<LogEntry>>,
    pub(super) scroll_offsets: HashMap<String, usize>,
    pub(super) follow_mode: HashMap<String, bool>,
    pub(super) output_seen: HashMap<String, bool>,
    pub(super) restart_pending: HashMap<String, bool>,
    pub(super) process_started_at: HashMap<String, Instant>,
    pub(super) process_restart_count: HashMap<String, usize>,
    pub(super) active_index: usize,
    pub(super) input_line: String,
    pub(super) input_mode: InputMode,
    pub(super) shell_capture_mode: bool,
    pub(super) show_help: bool,
    pub(super) show_options: bool,
    pub(super) options_index: usize,
    pub(super) observed_non_zero: HashMap<String, String>,
    pub(super) exit_states: HashMap<String, ProcessExitState>,
    pub(super) shutdown_on_exit_processes: HashSet<String>,
    pub(super) shutdown_requested: bool,
    pub(super) spinner_tick: usize,
    pub(super) vt_parsers: HashMap<String, VtParser>,
    pub(super) vt_saw_chunk: HashMap<String, bool>,
}
