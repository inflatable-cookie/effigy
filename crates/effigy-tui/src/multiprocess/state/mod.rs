use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use vt100::Parser as VtParser;

use crate::core::{InputMode, LogEntry, ProcessExitState};

mod accessors;
mod init;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionsAction {
    ToggleFollow,
    Restart,
    Stop,
    Cancel,
    Quit,
}

pub struct SessionState {
    pub process_names: Vec<String>,
    pub logs: HashMap<String, VecDeque<LogEntry>>,
    pub scroll_offsets: HashMap<String, usize>,
    pub follow_mode: HashMap<String, bool>,
    pub output_seen: HashMap<String, bool>,
    pub restart_pending: HashMap<String, bool>,
    pub process_started_at: HashMap<String, Instant>,
    pub process_restart_count: HashMap<String, usize>,
    pub active_index: usize,
    pub input_line: String,
    pub input_mode: InputMode,
    pub shell_capture_mode: bool,
    pub show_help: bool,
    pub show_options: bool,
    pub options_index: usize,
    pub observed_non_zero: HashMap<String, String>,
    pub exit_states: HashMap<String, ProcessExitState>,
    pub shutdown_on_exit_processes: HashSet<String>,
    pub shutdown_requested: bool,
    pub spinner_tick: usize,
    pub vt_parsers: HashMap<String, VtParser>,
    pub vt_saw_chunk: HashMap<String, bool>,
}
