use std::collections::HashMap;
use std::time::Instant;

use vt100::Parser as VtParser;

use crate::tui::core::{InputMode, LogEntry};

use super::SessionState;

impl SessionState {
    pub(in super::super) fn new(
        process_names: Vec<String>,
        vt_rows: u16,
        vt_cols: u16,
        vt_scrollback: usize,
    ) -> Self {
        let logs = map_for_processes(&process_names, || std::collections::VecDeque::<LogEntry>::new());
        let scroll_offsets = map_for_processes(&process_names, || 0usize);
        let follow_mode = map_for_processes(&process_names, || true);
        let output_seen = map_for_processes(&process_names, || false);
        let restart_pending = map_for_processes(&process_names, || false);
        let process_started_at = map_for_processes(&process_names, Instant::now);
        let process_restart_count = map_for_processes(&process_names, || 0usize);
        let vt_parsers =
            map_for_processes(&process_names, || VtParser::new(vt_rows, vt_cols, vt_scrollback));
        let vt_saw_chunk = map_for_processes(&process_names, || false);

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
}

fn map_for_processes<T, F>(process_names: &[String], mut value_fn: F) -> HashMap<String, T>
where
    F: FnMut() -> T,
{
    process_names
        .iter()
        .map(|name| (name.clone(), value_fn()))
        .collect::<HashMap<String, T>>()
}
