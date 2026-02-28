use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::process_manager::{ProcessEventKind, ProcessSupervisor};
use crate::tui::core::{
    next_index, prev_index, toggle_follow_for_active, InputMode, LogEntry, LogEntryKind,
    ProcessExitState,
};

use super::config::{EVENT_DRAIN_WAIT, VT_PARSER_COLS, VT_PARSER_ROWS, VT_PARSER_SCROLLBACK};
use super::diagnostics::RuntimeDiagnostics;
use super::render::options_actions;
use super::state::{OptionsAction, SessionState};
use super::terminal_text::{
    ingest_log_payload, is_expected_shutdown_diagnostic, push_entry, sanitize_log_text,
};
use super::{MultiProcessTuiError, MultiProcessTuiOptions};

pub(super) enum LoopControl {
    Continue,
    Quit,
}

pub(super) fn drain_process_events(
    supervisor: &ProcessSupervisor,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
    max_events: usize,
    vt_emulator_enabled: bool,
) {
    let mut drained_events = 0usize;
    while drained_events < max_events {
        let Some(event_item) = supervisor.next_event_timeout(EVENT_DRAIN_WAIT) else {
            break;
        };
        drained_events += 1;
        if let Some(buffer) = state.logs.get_mut(&event_item.process) {
            match event_item.kind {
                ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {
                    state
                        .restart_pending
                        .insert(event_item.process.clone(), false);
                    let had_output = *state.output_seen.get(&event_item.process).unwrap_or(&false);
                    state.output_seen.insert(event_item.process.clone(), true);
                    if vt_emulator_enabled {
                        if !had_output {
                            state.vt_parsers.insert(
                                event_item.process.clone(),
                                vt100::Parser::new(
                                    VT_PARSER_ROWS,
                                    VT_PARSER_COLS,
                                    VT_PARSER_SCROLLBACK,
                                ),
                            );
                            state.vt_saw_chunk.insert(event_item.process.clone(), false);
                            diagnostics.record_vt_reset(&event_item.process);
                        }
                        if let Some(chunk) = event_item.chunk.as_ref() {
                            if let Some(parser) = state.vt_parsers.get_mut(&event_item.process) {
                                parser.process(chunk);
                                state.vt_saw_chunk.insert(event_item.process.clone(), true);
                                match event_item.kind {
                                    ProcessEventKind::StdoutChunk => diagnostics
                                        .record_stdout_chunk(&event_item.process, chunk.len()),
                                    ProcessEventKind::StderrChunk => diagnostics
                                        .record_stderr_chunk(&event_item.process, chunk.len()),
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                ProcessEventKind::Stdout => {
                    if vt_emulator_enabled
                        && *state
                            .vt_saw_chunk
                            .get(&event_item.process)
                            .unwrap_or(&false)
                    {
                        continue;
                    }
                    state
                        .restart_pending
                        .insert(event_item.process.clone(), false);
                    state.output_seen.insert(event_item.process.clone(), true);
                    diagnostics.record_stdout_lines(payload_line_count(&event_item.payload));
                    ingest_log_payload(buffer, LogEntryKind::Stdout, &event_item.payload);
                }
                ProcessEventKind::Stderr => {
                    if vt_emulator_enabled
                        && *state
                            .vt_saw_chunk
                            .get(&event_item.process)
                            .unwrap_or(&false)
                    {
                        continue;
                    }
                    state
                        .restart_pending
                        .insert(event_item.process.clone(), false);
                    state.output_seen.insert(event_item.process.clone(), true);
                    diagnostics.record_stderr_lines(payload_line_count(&event_item.payload));
                    ingest_log_payload(buffer, LogEntryKind::Stderr, &event_item.payload);
                }
                ProcessEventKind::Exit => {
                    diagnostics.record_exit_event(&event_item.process, &event_item.payload);
                    let pending_restart = *state
                        .restart_pending
                        .get(&event_item.process)
                        .unwrap_or(&false);
                    if pending_restart
                        && (is_expected_shutdown_diagnostic(&event_item.payload)
                            || event_item.payload.trim() == "exit=0")
                    {
                        continue;
                    }
                    state
                        .restart_pending
                        .insert(event_item.process.clone(), false);
                    if event_item.payload.trim() == "exit=0"
                        || is_expected_shutdown_diagnostic(&event_item.payload)
                    {
                        state.observed_non_zero.remove(&event_item.process);
                        state
                            .exit_states
                            .insert(event_item.process.clone(), ProcessExitState::Success);
                    } else {
                        state
                            .observed_non_zero
                            .insert(event_item.process.clone(), event_item.payload.clone());
                        state
                            .exit_states
                            .insert(event_item.process.clone(), ProcessExitState::Failure);
                    }
                    push_entry(
                        buffer,
                        LogEntry {
                            kind: LogEntryKind::Exit,
                            line: sanitize_log_text(&event_item.payload),
                        },
                    );
                }
            };
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_key_event(
    key: &KeyEvent,
    supervisor: &ProcessSupervisor,
    state: &mut SessionState,
    diagnostics: &mut RuntimeDiagnostics,
    options: MultiProcessTuiOptions,
    max_offset: usize,
) -> Result<LoopControl, MultiProcessTuiError> {
    diagnostics.record_keypress(key);
    let active_process = state.active_process().to_owned();
    let active_is_shell = active_process == "shell";

    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        if active_is_shell && state.shell_capture_mode && !state.show_help && !state.show_options {
            supervisor.send_input(&active_process, "\u{3}")?;
            return Ok(LoopControl::Continue);
        }
        return Ok(LoopControl::Quit);
    }
    if active_is_shell
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('g'))
    {
        state.shell_capture_mode = !state.shell_capture_mode;
        state.input_mode = InputMode::Command;
        return Ok(LoopControl::Continue);
    }
    if active_is_shell && state.shell_capture_mode && !state.show_help && !state.show_options {
        if let Some(input) = shell_key_input(key) {
            supervisor.send_input(&active_process, &input)?;
        }
        return Ok(LoopControl::Continue);
    }
    if matches!(key.code, KeyCode::Esc)
        && options.esc_quit_on_complete
        && !state.show_help
        && !state.show_options
        && state.input_mode == InputMode::Command
        && all_processes_exited(&state.exit_states, state.process_names.len())
    {
        return Ok(LoopControl::Quit);
    }
    if matches!(key.code, KeyCode::Tab) {
        if active_is_shell {
            if !state.shell_capture_mode {
                state.shell_capture_mode = true;
            }
            return Ok(LoopControl::Continue);
        }
        state.input_mode = if state.input_mode == InputMode::Insert {
            InputMode::Command
        } else {
            InputMode::Insert
        };
        if state.input_mode == InputMode::Insert {
            state.show_help = false;
            state.show_options = false;
        }
        return Ok(LoopControl::Continue);
    }
    if state.show_options {
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
                    return Ok(LoopControl::Quit);
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
                    return Ok(LoopControl::Quit);
                }
                state.show_options = false;
            }
            KeyCode::Char('s') => {
                if apply_options_action(
                    OptionsAction::Stop,
                    &active,
                    supervisor,
                    state,
                    max_offset,
                )? {
                    return Ok(LoopControl::Quit);
                }
                state.show_options = false;
            }
            KeyCode::Char('q') => {
                if apply_options_action(
                    OptionsAction::Quit,
                    &active,
                    supervisor,
                    state,
                    max_offset,
                )? {
                    return Ok(LoopControl::Quit);
                }
                state.show_options = false;
            }
            KeyCode::Enter => {
                let action = actions[state.options_index];
                if apply_options_action(action, &active, supervisor, state, max_offset)? {
                    return Ok(LoopControl::Quit);
                }
                if !matches!(action, OptionsAction::ToggleFollow) {
                    state.show_options = false;
                }
            }
            _ => {}
        }
        return Ok(LoopControl::Continue);
    }
    if state.input_mode == InputMode::Insert {
        match key.code {
            KeyCode::Enter => {
                if !state.input_line.is_empty() {
                    let target = &state.process_names[state.active_index];
                    let mut payload = state.input_line.clone();
                    payload.push('\n');
                    supervisor.send_input(target, &payload)?;
                    state.input_line.clear();
                }
            }
            KeyCode::Backspace => {
                state.input_line.pop();
            }
            KeyCode::Esc => {
                state.input_mode = InputMode::Command;
            }
            KeyCode::Char(c) => {
                state.input_line.push(c);
            }
            _ => {}
        }
        return Ok(LoopControl::Continue);
    }

    match key.code {
        KeyCode::Char('i') => {
            if state.process_names[state.active_index] != "shell" {
                state.input_mode = InputMode::Insert;
                state.show_help = false;
                state.show_options = false;
            }
        }
        KeyCode::Char('h') => {
            state.show_help = !state.show_help;
            if state.show_help {
                state.show_options = false;
            }
        }
        KeyCode::Char('o') => {
            state.show_options = !state.show_options;
            if state.show_options {
                state.show_help = false;
                state.options_index = 0;
            }
        }
        KeyCode::BackTab => {
            state.shell_capture_mode = false;
            state.input_mode = InputMode::Command;
            state.active_index = prev_index(state.active_index, state.process_names.len());
        }
        KeyCode::Right => {
            state.shell_capture_mode = false;
            state.input_mode = InputMode::Command;
            state.active_index = next_index(state.active_index, state.process_names.len());
        }
        KeyCode::Left => {
            state.shell_capture_mode = false;
            state.input_mode = InputMode::Command;
            state.active_index = prev_index(state.active_index, state.process_names.len());
        }
        KeyCode::Up => {
            let active = &state.process_names[state.active_index];
            if let Some(follow) = state.follow_mode.get_mut(active) {
                *follow = false;
            }
            if let Some(offset) = state.scroll_offsets.get_mut(active) {
                *offset = offset.saturating_sub(1);
            }
        }
        KeyCode::Down => {
            let active = &state.process_names[state.active_index];
            if let Some(offset) = state.scroll_offsets.get_mut(active) {
                *offset = offset.saturating_add(1).min(max_offset);
            }
        }
        KeyCode::PageUp => {
            let active = &state.process_names[state.active_index];
            if let Some(follow) = state.follow_mode.get_mut(active) {
                *follow = false;
            }
            if let Some(offset) = state.scroll_offsets.get_mut(active) {
                *offset = offset.saturating_sub(10);
            }
        }
        KeyCode::PageDown => {
            let active = &state.process_names[state.active_index];
            if let Some(offset) = state.scroll_offsets.get_mut(active) {
                *offset = offset.saturating_add(10).min(max_offset);
            }
        }
        KeyCode::Home => {
            let active = &state.process_names[state.active_index];
            if let Some(follow) = state.follow_mode.get_mut(active) {
                *follow = false;
            }
            if let Some(offset) = state.scroll_offsets.get_mut(active) {
                *offset = 0;
            }
        }
        KeyCode::End => {
            let active = &state.process_names[state.active_index];
            if let Some(follow) = state.follow_mode.get_mut(active) {
                *follow = true;
            }
            if let Some(offset) = state.scroll_offsets.get_mut(active) {
                *offset = max_offset;
            }
        }
        KeyCode::Esc => {
            state.show_help = false;
            state.show_options = false;
        }
        _ => {}
    }

    Ok(LoopControl::Continue)
}

fn all_processes_exited(
    exit_states: &std::collections::HashMap<String, ProcessExitState>,
    process_count: usize,
) -> bool {
    process_count > 0 && exit_states.len() >= process_count
}

fn payload_line_count(raw: &str) -> usize {
    raw.lines().count().max(1)
}

fn shell_key_input(key: &KeyEvent) -> Option<String> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char(c) = key.code {
            let lower = c.to_ascii_lowercase() as u8;
            if (b'a'..=b'z').contains(&lower) {
                let value = lower - b'a' + 1;
                return Some((value as char).to_string());
            }
        }
    }

    let mapped = match key.code {
        KeyCode::Enter => "\n",
        KeyCode::Tab => "\t",
        KeyCode::Backspace => "\u{7f}",
        KeyCode::Left => "\u{1b}[D",
        KeyCode::Right => "\u{1b}[C",
        KeyCode::Up => "\u{1b}[A",
        KeyCode::Down => "\u{1b}[B",
        KeyCode::Home => "\u{1b}[H",
        KeyCode::End => "\u{1b}[F",
        KeyCode::Delete => "\u{1b}[3~",
        KeyCode::Char(c) => return Some(c.to_string()),
        _ => return None,
    };
    Some(mapped.to_owned())
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
    logs: &mut std::collections::HashMap<String, std::collections::VecDeque<LogEntry>>,
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::tui::core::{next_index, prev_index, toggle_follow_for_active};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{all_processes_exited, shell_key_input, ProcessExitState};

    #[test]
    fn all_processes_exited_requires_full_count() {
        let mut exits = HashMap::new();
        exits.insert("a".to_owned(), ProcessExitState::Success);
        assert!(!all_processes_exited(&exits, 2));
        exits.insert("b".to_owned(), ProcessExitState::Failure);
        assert!(all_processes_exited(&exits, 2));
    }

    #[test]
    fn tab_index_helpers_wrap_correctly() {
        assert_eq!(next_index(0, 4), 1);
        assert_eq!(next_index(3, 4), 0);
        assert_eq!(prev_index(0, 4), 3);
        assert_eq!(prev_index(2, 4), 1);
    }

    #[test]
    fn toggle_follow_updates_mode_and_offset() {
        let mut follow = HashMap::from([("api".to_owned(), false)]);
        let mut offsets = HashMap::from([("api".to_owned(), 1usize)]);
        toggle_follow_for_active(&mut follow, &mut offsets, "api", 42);
        assert_eq!(follow.get("api"), Some(&true));
        assert_eq!(offsets.get("api"), Some(&42usize));

        toggle_follow_for_active(&mut follow, &mut offsets, "api", 99);
        assert_eq!(follow.get("api"), Some(&false));
        assert_eq!(offsets.get("api"), Some(&42usize));
    }

    #[test]
    fn shell_key_input_maps_control_keys() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(shell_key_input(&key), Some("\u{3}".to_owned()));
        let key = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(shell_key_input(&key), Some("\u{1b}[D".to_owned()));
    }
}
