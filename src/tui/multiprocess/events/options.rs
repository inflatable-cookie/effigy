use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};

use crate::process_manager::ProcessSupervisor;
use crate::tui::core::{toggle_follow_for_active, LogEntry, LogEntryKind};

use super::super::render::options_actions;
use super::super::state::{OptionsAction, SessionState};
use super::super::terminal_text::{push_entry, sanitize_log_text};
use super::super::MultiProcessTuiError;
use super::LoopControl;

pub(super) fn handle_options_overlay_key(
    key: &KeyEvent,
    supervisor: &ProcessSupervisor,
    state: &mut SessionState,
    max_offset: usize,
) -> Result<Option<LoopControl>, MultiProcessTuiError> {
    if !state.show_options {
        return Ok(None);
    }

    let follow_active = *state
        .follow_mode
        .get(&state.process_names[state.active_index])
        .unwrap_or(&true);
    let actions = options_actions(follow_active);
    let active = state.process_names[state.active_index].clone();
    match key.code {
        KeyCode::Esc => {
            state.show_options = false;
        }
        KeyCode::Char('o') => {
            state.show_options = false;
        }
        KeyCode::Up => {
            state.options_index = state.options_index.saturating_sub(1);
        }
        KeyCode::Down => {
            let max = actions.len().saturating_sub(1);
            state.options_index = (state.options_index + 1).min(max);
        }
        KeyCode::Char('f') => {
            if apply_options_action(
                OptionsAction::ToggleFollow,
                &active,
                supervisor,
                state,
                max_offset,
            )? {
                return Ok(Some(LoopControl::Quit));
            }
        }
        KeyCode::Char('r') => {
            if apply_options_action(
                OptionsAction::Restart,
                &active,
                supervisor,
                state,
                max_offset,
            )? {
                return Ok(Some(LoopControl::Quit));
            }
            state.show_options = false;
        }
        KeyCode::Char('s') => {
            if apply_options_action(OptionsAction::Stop, &active, supervisor, state, max_offset)? {
                return Ok(Some(LoopControl::Quit));
            }
            state.show_options = false;
        }
        KeyCode::Char('q') => {
            if apply_options_action(OptionsAction::Quit, &active, supervisor, state, max_offset)? {
                return Ok(Some(LoopControl::Quit));
            }
            state.show_options = false;
        }
        KeyCode::Enter => {
            let action = actions[state.options_index];
            if apply_options_action(action, &active, supervisor, state, max_offset)? {
                return Ok(Some(LoopControl::Quit));
            }
            if !matches!(action, OptionsAction::ToggleFollow) {
                state.show_options = false;
            }
        }
        _ => {}
    }
    Ok(Some(LoopControl::Continue))
}

fn apply_options_action(
    action: OptionsAction,
    active: &str,
    supervisor: &ProcessSupervisor,
    state: &mut SessionState,
    max_offset: usize,
) -> Result<bool, MultiProcessTuiError> {
    match action {
        OptionsAction::ToggleFollow => {
            toggle_follow_for_active(
                &mut state.follow_mode,
                &mut state.scroll_offsets,
                active,
                max_offset,
            );
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
