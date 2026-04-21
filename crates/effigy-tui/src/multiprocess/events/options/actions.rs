use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use crate::core::{LogEntry, LogEntryKind};
use effigy_process::ProcessSupervisor;

use super::super::super::state::{OptionsAction, SessionState};
use super::super::super::terminal_text::{push_entry, sanitize_log_text};
use super::super::super::transcript::export_active_process_transcript;
use super::super::super::MultiProcessTuiError;

pub(super) fn apply_options_action(
    action: OptionsAction,
    active: &str,
    supervisor: &ProcessSupervisor,
    state: &mut SessionState,
    max_offset: usize,
) -> Result<bool, MultiProcessTuiError> {
    match action {
        OptionsAction::ToggleFollow => {
            let follow = state.follow_for(active);
            state.set_follow_for(active, !follow);
            if !follow {
                state.set_scroll_offset_for(active, max_offset);
            }
            Ok(false)
        }
        OptionsAction::ExportTranscript => {
            let export_path = export_active_process_transcript(state)?;
            let display_path = export_path
                .strip_prefix(&state.repo_root)
                .unwrap_or(export_path.as_path());
            state.set_footer_message(format!(
                "exported clean transcript to {}",
                display_path.display()
            ));
            Ok(false)
        }
        OptionsAction::Restart => {
            match supervisor.restart_process(active) {
                Ok(()) => {
                    state.exit_states.remove(active);
                    state.observed_non_zero.remove(active);
                    state.output_seen.insert(active.to_owned(), false);
                    state.restart_pending.insert(active.to_owned(), true);
                    state
                        .process_started_at
                        .insert(active.to_owned(), Instant::now());
                    state
                        .process_restart_count
                        .entry(active.to_owned())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                    push_log_line(
                        &mut state.logs,
                        active,
                        LogEntryKind::Stdout,
                        "[effigy] restarted process".to_owned(),
                    );
                }
                Err(err) => push_log_line(
                    &mut state.logs,
                    active,
                    LogEntryKind::Stderr,
                    format!("[effigy] restart failed: {err}"),
                ),
            }
            Ok(false)
        }
        OptionsAction::Stop => {
            match supervisor.terminate_process(active) {
                Ok(()) => push_log_line(
                    &mut state.logs,
                    active,
                    LogEntryKind::Stdout,
                    "[effigy] stop requested".to_owned(),
                ),
                Err(err) => push_log_line(
                    &mut state.logs,
                    active,
                    LogEntryKind::Stderr,
                    format!("[effigy] stop failed: {err}"),
                ),
            }
            Ok(false)
        }
        OptionsAction::Cancel => Ok(false),
        OptionsAction::Quit => Ok(true),
    }
}

fn push_log_line(
    logs: &mut HashMap<String, VecDeque<LogEntry>>,
    process: &str,
    kind: LogEntryKind,
    line: String,
) {
    if let Some(buffer) = logs.get_mut(process) {
        push_entry(
            buffer,
            LogEntry {
                kind,
                line: sanitize_log_text(&line),
            },
        );
    }
}
