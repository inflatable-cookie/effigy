use std::collections::{HashMap, VecDeque};
use std::io;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::Terminal;
use vt100::Parser as VtParser;

use crate::process_manager::{
    ProcessEventKind, ProcessManagerError, ProcessSpec, ProcessSupervisor, ShutdownProgress,
};
use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{KeyValue, OutputMode, PlainRenderer, Renderer, UiError};

mod render;
mod terminal_text;

const MAX_LOG_LINES: usize = 2000;
const MAX_EVENTS_PER_TICK: usize = 200;
const VT_SPIKE_ROWS: u16 = 2000;
const VT_SPIKE_COLS: u16 = 240;
const VT_SPIKE_SCROLLBACK: usize = 8000;

use render::{options_actions, render_ui};
use terminal_text::{
    format_elapsed, ingest_log_payload, is_expected_shutdown_diagnostic, push_entry,
    sanitize_log_text, styled_text, vt_logs,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Command,
    Insert,
}

#[derive(Debug, Clone)]
enum LogEntryKind {
    Stdout,
    Stderr,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessExitState {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OptionsAction {
    ToggleFollow,
    Restart,
    Stop,
    Cancel,
    Quit,
}

#[derive(Debug, Clone)]
struct LogEntry {
    kind: LogEntryKind,
    line: String,
}

#[derive(Debug)]
pub enum MultiProcessTuiError {
    Io(io::Error),
    Ui(UiError),
    Process(ProcessManagerError),
    NoProcesses,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiProcessTuiOutcome {
    pub non_zero_exits: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MultiProcessTuiOptions {
    pub esc_quit_on_complete: bool,
}

impl std::fmt::Display for MultiProcessTuiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultiProcessTuiError::Io(err) => write!(f, "{err}"),
            MultiProcessTuiError::Ui(err) => write!(f, "{err}"),
            MultiProcessTuiError::Process(err) => write!(f, "{err}"),
            MultiProcessTuiError::NoProcesses => write!(f, "managed TUI session has no processes"),
        }
    }
}

impl std::error::Error for MultiProcessTuiError {}

impl From<io::Error> for MultiProcessTuiError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ProcessManagerError> for MultiProcessTuiError {
    fn from(value: ProcessManagerError) -> Self {
        Self::Process(value)
    }
}

impl From<UiError> for MultiProcessTuiError {
    fn from(value: UiError) -> Self {
        Self::Ui(value)
    }
}

pub fn run_multiprocess_tui(
    repo_root: PathBuf,
    processes: Vec<ProcessSpec>,
    tab_order: Vec<String>,
    options: MultiProcessTuiOptions,
) -> Result<MultiProcessTuiOutcome, MultiProcessTuiError> {
    if processes.is_empty() {
        return Err(MultiProcessTuiError::NoProcesses);
    }

    let process_names = processes
        .iter()
        .map(|process| process.name.clone())
        .collect::<Vec<String>>();
    let process_names = if tab_order.is_empty() {
        process_names
    } else {
        tab_order
    };
    let supervisor = ProcessSupervisor::spawn(repo_root, processes)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut logs: HashMap<String, VecDeque<LogEntry>> = process_names
        .iter()
        .map(|name| (name.clone(), VecDeque::new()))
        .collect();
    let mut scroll_offsets: HashMap<String, usize> = process_names
        .iter()
        .map(|name| (name.clone(), 0usize))
        .collect();
    let mut follow_mode: HashMap<String, bool> = process_names
        .iter()
        .map(|name| (name.clone(), true))
        .collect();
    let mut output_seen: HashMap<String, bool> = process_names
        .iter()
        .map(|name| (name.clone(), false))
        .collect();
    let mut restart_pending: HashMap<String, bool> = process_names
        .iter()
        .map(|name| (name.clone(), false))
        .collect();
    let mut process_started_at: HashMap<String, Instant> = process_names
        .iter()
        .map(|name| (name.clone(), Instant::now()))
        .collect();
    let mut process_restart_count: HashMap<String, usize> = process_names
        .iter()
        .map(|name| (name.clone(), 0usize))
        .collect();
    let mut active_index: usize = 0;
    let mut input_line = String::new();
    let mut input_mode = InputMode::Command;
    let mut shell_capture_mode = false;
    let mut show_help = false;
    let mut show_options = false;
    let mut options_index = 0usize;
    let mut observed_non_zero: HashMap<String, String> = HashMap::new();
    let mut exit_states: HashMap<String, ProcessExitState> = HashMap::new();
    let mut spinner_tick: usize = 0;
    let vt_emulator_enabled = std::env::var("EFFIGY_TUI_VT100")
        .ok()
        .is_none_or(|value| value != "0" && !value.eq_ignore_ascii_case("false"));
    let mut vt_parsers: HashMap<String, VtParser> = process_names
        .iter()
        .map(|name| {
            (
                name.clone(),
                VtParser::new(VT_SPIKE_ROWS, VT_SPIKE_COLS, VT_SPIKE_SCROLLBACK),
            )
        })
        .collect();
    let mut vt_saw_chunk: HashMap<String, bool> = process_names
        .iter()
        .map(|name| (name.clone(), false))
        .collect();

    let result: Result<(), MultiProcessTuiError> = loop {
        let mut drained_events = 0usize;
        while drained_events < MAX_EVENTS_PER_TICK {
            let Some(event_item) = supervisor.next_event_timeout(Duration::from_millis(1)) else {
                break;
            };
            drained_events += 1;
            if let Some(buffer) = logs.get_mut(&event_item.process) {
                match event_item.kind {
                    ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {
                        restart_pending.insert(event_item.process.clone(), false);
                        let had_output = *output_seen.get(&event_item.process).unwrap_or(&false);
                        output_seen.insert(event_item.process.clone(), true);
                        if vt_emulator_enabled {
                            if !had_output {
                                vt_parsers.insert(
                                    event_item.process.clone(),
                                    VtParser::new(
                                        VT_SPIKE_ROWS,
                                        VT_SPIKE_COLS,
                                        VT_SPIKE_SCROLLBACK,
                                    ),
                                );
                                vt_saw_chunk.insert(event_item.process.clone(), false);
                            }
                            if let Some(chunk) = event_item.chunk.as_ref() {
                                if let Some(parser) = vt_parsers.get_mut(&event_item.process) {
                                    parser.process(chunk);
                                    vt_saw_chunk.insert(event_item.process.clone(), true);
                                }
                            }
                        }
                    }
                    ProcessEventKind::Stdout => {
                        if vt_emulator_enabled
                            && *vt_saw_chunk.get(&event_item.process).unwrap_or(&false)
                        {
                            continue;
                        }
                        restart_pending.insert(event_item.process.clone(), false);
                        output_seen.insert(event_item.process.clone(), true);
                        ingest_log_payload(buffer, LogEntryKind::Stdout, &event_item.payload);
                    }
                    ProcessEventKind::Stderr => {
                        if vt_emulator_enabled
                            && *vt_saw_chunk.get(&event_item.process).unwrap_or(&false)
                        {
                            continue;
                        }
                        restart_pending.insert(event_item.process.clone(), false);
                        output_seen.insert(event_item.process.clone(), true);
                        ingest_log_payload(buffer, LogEntryKind::Stderr, &event_item.payload);
                    }
                    ProcessEventKind::Exit => {
                        let pending_restart =
                            *restart_pending.get(&event_item.process).unwrap_or(&false);
                        if pending_restart
                            && (is_expected_shutdown_diagnostic(&event_item.payload)
                                || event_item.payload.trim() == "exit=0")
                        {
                            continue;
                        }
                        restart_pending.insert(event_item.process.clone(), false);
                        if event_item.payload.trim() == "exit=0"
                            || is_expected_shutdown_diagnostic(&event_item.payload)
                        {
                            observed_non_zero.remove(&event_item.process);
                            exit_states
                                .insert(event_item.process.clone(), ProcessExitState::Success);
                        } else {
                            observed_non_zero
                                .insert(event_item.process.clone(), event_item.payload.clone());
                            exit_states
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
        spinner_tick = spinner_tick.wrapping_add(1);

        let active = &process_names[active_index];
        let size = terminal.size()?;
        let output_height = size.height.saturating_sub(9) as usize;
        let output_width = size.width.saturating_sub(4) as usize;
        let is_follow = *follow_mode.get(active).unwrap_or(&true);
        let (active_logs, scroll_offset, max_offset, render_scroll_offset, scrollbar_total) =
            if vt_emulator_enabled && *vt_saw_chunk.get(active).unwrap_or(&false) {
                let parser = vt_parsers
                    .get_mut(active)
                    .expect("active vt parser missing unexpectedly");
                let stored = *scroll_offsets.get(active).unwrap_or(&0usize);
                let (mut rendered, clamped, max_vt) = vt_logs(
                    parser,
                    output_height.saturating_sub(1).max(1),
                    output_width.max(1),
                    stored,
                    is_follow,
                );
                if let Some(buffer) = logs.get(active) {
                    rendered.extend(buffer.iter().filter_map(|entry| {
                        if matches!(entry.kind, LogEntryKind::Exit) {
                            Some(entry.clone())
                        } else {
                            None
                        }
                    }));
                }
                scroll_offsets.insert(active.clone(), clamped);
                (
                    rendered,
                    clamped,
                    max_vt,
                    0usize,
                    max_vt.saturating_add(output_height.max(1)),
                )
            } else {
                let rendered = logs
                    .get(active)
                    .map(|entries| entries.iter().cloned().collect::<Vec<LogEntry>>())
                    .unwrap_or_default();
                let max = rendered.len().saturating_sub(output_height);
                let stored = *scroll_offsets.get(active).unwrap_or(&0usize);
                let clamped = stored.min(max);
                scroll_offsets.insert(active.clone(), clamped);
                let render = if is_follow { max } else { clamped };
                (
                    rendered,
                    clamped,
                    max,
                    render,
                    output_height.max(1).saturating_add(max),
                )
            };
        let shell_cursor = if active == "shell" && vt_emulator_enabled {
            vt_parsers
                .get(active)
                .map(|parser| parser.screen().cursor_position())
        } else {
            None
        };
        let now = Instant::now();
        let active_elapsed = process_started_at
            .get(active)
            .map(|started| now.saturating_duration_since(*started))
            .unwrap_or_default();
        let active_restart_count = *process_restart_count.get(active).unwrap_or(&0);
        terminal.draw(|frame| {
            render_ui(
                frame,
                &process_names,
                active_index,
                &active_logs,
                scroll_offset,
                max_offset,
                render_scroll_offset,
                scrollbar_total,
                is_follow,
                active,
                &input_line,
                input_mode,
                shell_capture_mode,
                &exit_states,
                show_help,
                show_options,
                options_index,
                *output_seen.get(active).unwrap_or(&false),
                spinner_tick,
                active_elapsed,
                active_restart_count,
                shell_cursor,
            )
        })?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                let active_process = &process_names[active_index];
                let active_is_shell = active_process == "shell";
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('c'))
                {
                    if active_is_shell && shell_capture_mode && !show_help && !show_options {
                        supervisor.send_input(active_process, "\u{3}")?;
                        continue;
                    }
                    break Ok(());
                }
                if active_is_shell
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('g'))
                {
                    shell_capture_mode = !shell_capture_mode;
                    input_mode = InputMode::Command;
                    continue;
                }
                if active_is_shell && shell_capture_mode && !show_help && !show_options {
                    if let Some(input) = shell_key_input(&key) {
                        supervisor.send_input(active_process, &input)?;
                    }
                    continue;
                }
                if matches!(key.code, KeyCode::Esc)
                    && options.esc_quit_on_complete
                    && !show_help
                    && !show_options
                    && input_mode == InputMode::Command
                    && all_processes_exited(&exit_states, process_names.len())
                {
                    break Ok(());
                }
                if matches!(key.code, KeyCode::Tab) {
                    if active_is_shell {
                        if !shell_capture_mode {
                            shell_capture_mode = true;
                        }
                        continue;
                    }
                    input_mode = if input_mode == InputMode::Insert {
                        InputMode::Command
                    } else {
                        InputMode::Insert
                    };
                    if input_mode == InputMode::Insert {
                        show_help = false;
                        show_options = false;
                    }
                    continue;
                }
                if show_options {
                    let follow_active = *follow_mode
                        .get(&process_names[active_index])
                        .unwrap_or(&true);
                    let actions = options_actions(follow_active);
                    let active = process_names[active_index].clone();
                    match key.code {
                        KeyCode::Esc => {
                            show_options = false;
                        }
                        KeyCode::Char('o') => {
                            show_options = false;
                        }
                        KeyCode::Up => {
                            options_index = options_index.saturating_sub(1);
                        }
                        KeyCode::Down => {
                            let max = actions.len().saturating_sub(1);
                            options_index = (options_index + 1).min(max);
                        }
                        KeyCode::Char('f') => {
                            let should_quit = apply_options_action(
                                OptionsAction::ToggleFollow,
                                &active,
                                &supervisor,
                                &mut follow_mode,
                                &mut scroll_offsets,
                                max_offset,
                                &mut exit_states,
                                &mut observed_non_zero,
                                &mut output_seen,
                                &mut restart_pending,
                                &mut logs,
                                &mut process_started_at,
                                &mut process_restart_count,
                            )?;
                            if should_quit {
                                break Ok(());
                            }
                        }
                        KeyCode::Char('r') => {
                            let should_quit = apply_options_action(
                                OptionsAction::Restart,
                                &active,
                                &supervisor,
                                &mut follow_mode,
                                &mut scroll_offsets,
                                max_offset,
                                &mut exit_states,
                                &mut observed_non_zero,
                                &mut output_seen,
                                &mut restart_pending,
                                &mut logs,
                                &mut process_started_at,
                                &mut process_restart_count,
                            )?;
                            show_options = false;
                            if should_quit {
                                break Ok(());
                            }
                        }
                        KeyCode::Char('s') => {
                            let should_quit = apply_options_action(
                                OptionsAction::Stop,
                                &active,
                                &supervisor,
                                &mut follow_mode,
                                &mut scroll_offsets,
                                max_offset,
                                &mut exit_states,
                                &mut observed_non_zero,
                                &mut output_seen,
                                &mut restart_pending,
                                &mut logs,
                                &mut process_started_at,
                                &mut process_restart_count,
                            )?;
                            show_options = false;
                            if should_quit {
                                break Ok(());
                            }
                        }
                        KeyCode::Char('q') => {
                            let should_quit = apply_options_action(
                                OptionsAction::Quit,
                                &active,
                                &supervisor,
                                &mut follow_mode,
                                &mut scroll_offsets,
                                max_offset,
                                &mut exit_states,
                                &mut observed_non_zero,
                                &mut output_seen,
                                &mut restart_pending,
                                &mut logs,
                                &mut process_started_at,
                                &mut process_restart_count,
                            )?;
                            show_options = false;
                            if should_quit {
                                break Ok(());
                            }
                        }
                        KeyCode::Enter => {
                            let action = actions[options_index];
                            let should_quit = apply_options_action(
                                action,
                                &active,
                                &supervisor,
                                &mut follow_mode,
                                &mut scroll_offsets,
                                max_offset,
                                &mut exit_states,
                                &mut observed_non_zero,
                                &mut output_seen,
                                &mut restart_pending,
                                &mut logs,
                                &mut process_started_at,
                                &mut process_restart_count,
                            )?;
                            if should_quit {
                                break Ok(());
                            }
                            if !matches!(action, OptionsAction::ToggleFollow) {
                                show_options = false;
                            }
                        }
                        _ => {}
                    }
                    continue;
                }
                if input_mode == InputMode::Insert {
                    match key.code {
                        KeyCode::Enter => {
                            if !input_line.is_empty() {
                                let target = &process_names[active_index];
                                let mut payload = input_line.clone();
                                payload.push('\n');
                                supervisor.send_input(target, &payload)?;
                                input_line.clear();
                            }
                        }
                        KeyCode::Backspace => {
                            input_line.pop();
                        }
                        KeyCode::Esc => {
                            input_mode = InputMode::Command;
                        }
                        KeyCode::Char(c) => {
                            input_line.push(c);
                        }
                        _ => {}
                    }
                    continue;
                }
                match key.code {
                    KeyCode::Char('i') => {
                        if process_names[active_index] == "shell" {
                            continue;
                        }
                        input_mode = InputMode::Insert;
                        show_help = false;
                        show_options = false;
                    }
                    KeyCode::Char('h') => {
                        show_help = !show_help;
                        if show_help {
                            show_options = false;
                        }
                    }
                    KeyCode::Char('o') => {
                        show_options = !show_options;
                        if show_options {
                            show_help = false;
                            options_index = 0;
                        }
                    }
                    KeyCode::BackTab => {
                        shell_capture_mode = false;
                        input_mode = InputMode::Command;
                        active_index = if active_index == 0 {
                            process_names.len() - 1
                        } else {
                            active_index - 1
                        };
                    }
                    KeyCode::Right => {
                        shell_capture_mode = false;
                        input_mode = InputMode::Command;
                        active_index = (active_index + 1) % process_names.len();
                    }
                    KeyCode::Left => {
                        shell_capture_mode = false;
                        input_mode = InputMode::Command;
                        active_index = if active_index == 0 {
                            process_names.len() - 1
                        } else {
                            active_index - 1
                        };
                    }
                    KeyCode::Up => {
                        let active = &process_names[active_index];
                        if let Some(follow) = follow_mode.get_mut(active) {
                            *follow = false;
                        }
                        if let Some(offset) = scroll_offsets.get_mut(active) {
                            *offset = offset.saturating_sub(1);
                        }
                    }
                    KeyCode::Down => {
                        let active = &process_names[active_index];
                        if let Some(offset) = scroll_offsets.get_mut(active) {
                            *offset = offset.saturating_add(1).min(max_offset);
                        }
                    }
                    KeyCode::PageUp => {
                        let active = &process_names[active_index];
                        if let Some(follow) = follow_mode.get_mut(active) {
                            *follow = false;
                        }
                        if let Some(offset) = scroll_offsets.get_mut(active) {
                            *offset = offset.saturating_sub(10);
                        }
                    }
                    KeyCode::PageDown => {
                        let active = &process_names[active_index];
                        if let Some(offset) = scroll_offsets.get_mut(active) {
                            *offset = offset.saturating_add(10).min(max_offset);
                        }
                    }
                    KeyCode::Home => {
                        let active = &process_names[active_index];
                        if let Some(follow) = follow_mode.get_mut(active) {
                            *follow = false;
                        }
                        if let Some(offset) = scroll_offsets.get_mut(active) {
                            *offset = 0;
                        }
                    }
                    KeyCode::End => {
                        let active = &process_names[active_index];
                        if let Some(follow) = follow_mode.get_mut(active) {
                            *follow = true;
                        }
                        if let Some(offset) = scroll_offsets.get_mut(active) {
                            *offset = max_offset;
                        }
                    }
                    KeyCode::Esc => {
                        show_help = false;
                        show_options = false;
                    }
                    _ => {}
                }
            }
        }
    };

    supervisor.terminate_all_graceful_with_progress(Duration::from_secs(3), |progress| {
        let label = match progress {
            ShutdownProgress::SendingTerm => "Shutdown: sending SIGTERM to managed processes...",
            ShutdownProgress::Waiting => "Shutdown: waiting for managed processes to exit...",
            ShutdownProgress::ForceKilling => {
                "Shutdown: forcing remaining managed processes to stop..."
            }
            ShutdownProgress::Complete { .. } => "Shutdown: complete.",
        };
        let _ = draw_shutdown_status(&mut terminal, label);
    });
    let mut non_zero_map = observed_non_zero;
    for (name, diagnostic) in supervisor
        .exit_diagnostics()
        .into_iter()
        .filter(|(_, diagnostic)| {
            diagnostic != "exit=0" && !is_expected_shutdown_diagnostic(diagnostic)
        })
    {
        non_zero_map.insert(name, diagnostic);
    }
    let mut non_zero_exits = non_zero_map.into_iter().collect::<Vec<(String, String)>>();
    non_zero_exits.sort_by(|a, b| a.0.cmp(&b.0));
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, EnableLineWrap)?;
    terminal.show_cursor()?;

    let mut renderer = PlainRenderer::stdout(OutputMode::from_env());
    renderer.section("Process Results")?;
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let theme = Theme::default();
    let diagnostics = supervisor.exit_diagnostics();
    let now = Instant::now();
    for (name, diagnostic) in diagnostics {
        let elapsed = process_started_at
            .get(&name)
            .map(|started| format_elapsed(now.saturating_duration_since(*started)))
            .unwrap_or_else(|| "0s".to_owned());
        let status = if diagnostic == "exit=0" || is_expected_shutdown_diagnostic(&diagnostic) {
            if color_enabled {
                format!(
                    "{} {}",
                    styled_text(theme.success, "✓ OK"),
                    styled_text(theme.muted, &elapsed)
                )
            } else {
                format!("OK {elapsed}")
            }
        } else if color_enabled {
            format!(
                "{} {}",
                styled_text(theme.error, &diagnostic),
                styled_text(theme.muted, &elapsed)
            )
        } else {
            format!("{diagnostic} {elapsed}")
        };
        renderer.key_values(&[KeyValue::new(name, status)])?;
    }
    renderer.text("")?;

    result?;
    Ok(MultiProcessTuiOutcome { non_zero_exits })
}

fn draw_shutdown_status(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    status: &str,
) -> Result<(), io::Error> {
    terminal.draw(|frame| {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let footer = Paragraph::new(status.to_owned()).style(Style::default().fg(Color::Yellow));
        frame.render_widget(footer, chunks[1]);
    })?;
    Ok(())
}

fn all_processes_exited(
    exit_states: &HashMap<String, ProcessExitState>,
    process_count: usize,
) -> bool {
    process_count > 0 && exit_states.len() >= process_count
}

fn shell_key_input(key: &crossterm::event::KeyEvent) -> Option<String> {
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
    follow_mode: &mut HashMap<String, bool>,
    scroll_offsets: &mut HashMap<String, usize>,
    max_offset: usize,
    exit_states: &mut HashMap<String, ProcessExitState>,
    observed_non_zero: &mut HashMap<String, String>,
    output_seen: &mut HashMap<String, bool>,
    restart_pending: &mut HashMap<String, bool>,
    logs: &mut HashMap<String, VecDeque<LogEntry>>,
    process_started_at: &mut HashMap<String, Instant>,
    process_restart_count: &mut HashMap<String, usize>,
) -> Result<bool, MultiProcessTuiError> {
    match action {
        OptionsAction::ToggleFollow => {
            if let Some(follow) = follow_mode.get_mut(active) {
                *follow = !*follow;
                if *follow {
                    if let Some(offset) = scroll_offsets.get_mut(active) {
                        *offset = max_offset;
                    }
                }
            }
            Ok(false)
        }
        OptionsAction::Restart => {
            match supervisor.restart_process(active) {
                Ok(()) => {
                    exit_states.remove(active);
                    observed_non_zero.remove(active);
                    output_seen.insert(active.to_owned(), false);
                    restart_pending.insert(active.to_owned(), true);
                    process_started_at.insert(active.to_owned(), Instant::now());
                    process_restart_count
                        .entry(active.to_owned())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                    push_log_line(
                        logs,
                        active,
                        LogEntryKind::Stdout,
                        "[effigy] restarted process".to_owned(),
                    );
                }
                Err(err) => push_log_line(
                    logs,
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
                    logs,
                    active,
                    LogEntryKind::Stdout,
                    "[effigy] stop requested".to_owned(),
                ),
                Err(err) => push_log_line(
                    logs,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_processes_exited_requires_full_count() {
        let mut exits = HashMap::new();
        exits.insert("a".to_owned(), ProcessExitState::Success);
        assert!(!all_processes_exited(&exits, 2));
        exits.insert("b".to_owned(), ProcessExitState::Failure);
        assert!(all_processes_exited(&exits, 2));
    }
}
