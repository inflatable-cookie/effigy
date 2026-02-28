use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use vt100::Parser as VtParser;

use crate::tui::core::{InputMode, LogEntry, ProcessExitState};

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
    pub(super) spinner_tick: usize,
    pub(super) vt_parsers: HashMap<String, VtParser>,
    pub(super) vt_saw_chunk: HashMap<String, bool>,
}

impl SessionState {
    pub(super) fn new(
        process_names: Vec<String>,
        vt_rows: u16,
        vt_cols: u16,
        vt_scrollback: usize,
    ) -> Self {
        let logs = process_names
            .iter()
            .map(|name| (name.clone(), VecDeque::new()))
            .collect::<HashMap<String, VecDeque<LogEntry>>>();
        let scroll_offsets = process_names
            .iter()
            .map(|name| (name.clone(), 0usize))
            .collect::<HashMap<String, usize>>();
        let follow_mode = process_names
            .iter()
            .map(|name| (name.clone(), true))
            .collect::<HashMap<String, bool>>();
        let output_seen = process_names
            .iter()
            .map(|name| (name.clone(), false))
            .collect::<HashMap<String, bool>>();
        let restart_pending = process_names
            .iter()
            .map(|name| (name.clone(), false))
            .collect::<HashMap<String, bool>>();
        let process_started_at = process_names
            .iter()
            .map(|name| (name.clone(), Instant::now()))
            .collect::<HashMap<String, Instant>>();
        let process_restart_count = process_names
            .iter()
            .map(|name| (name.clone(), 0usize))
            .collect::<HashMap<String, usize>>();
        let vt_parsers = process_names
            .iter()
            .map(|name| (name.clone(), VtParser::new(vt_rows, vt_cols, vt_scrollback)))
            .collect::<HashMap<String, VtParser>>();
        let vt_saw_chunk = process_names
            .iter()
            .map(|name| (name.clone(), false))
            .collect::<HashMap<String, bool>>();

        Self {
            process_names,
            logs,
            scroll_offsets,
            follow_mode,
            output_seen,
            restart_pending,
            process_started_at,
            process_restart_count,
            active_index: 0,
            input_line: String::new(),
            input_mode: InputMode::Command,
            shell_capture_mode: false,
            show_help: false,
            show_options: false,
            options_index: 0,
            observed_non_zero: HashMap::new(),
            exit_states: HashMap::new(),
            spinner_tick: 0,
            vt_parsers,
            vt_saw_chunk,
        }
    }

    pub(super) fn active_process(&self) -> &str {
        &self.process_names[self.active_index]
    }
}
