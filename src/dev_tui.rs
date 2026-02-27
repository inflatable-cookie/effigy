use std::collections::{HashMap, VecDeque};
use std::io;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anstyle::Style as AnsiStyle;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Tabs,
};
use ratatui::{Frame, Terminal};
use vt100::Parser as VtParser;

use crate::process_manager::{
    ProcessEventKind, ProcessManagerError, ProcessSpec, ProcessSupervisor, ShutdownProgress,
};
use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{KeyValue, OutputMode, PlainRenderer, Renderer, UiError};

const MAX_LOG_LINES: usize = 2000;
const MAX_EVENTS_PER_TICK: usize = 200;
const VT_SPIKE_ROWS: u16 = 2000;
const VT_SPIKE_COLS: u16 = 240;
const VT_SPIKE_SCROLLBACK: usize = 8000;

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
pub enum DevTuiError {
    Io(io::Error),
    Ui(UiError),
    Process(ProcessManagerError),
    NoProcesses,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevTuiOutcome {
    pub non_zero_exits: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DevTuiOptions {
    pub esc_quit_on_complete: bool,
}

impl std::fmt::Display for DevTuiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DevTuiError::Io(err) => write!(f, "{err}"),
            DevTuiError::Ui(err) => write!(f, "{err}"),
            DevTuiError::Process(err) => write!(f, "{err}"),
            DevTuiError::NoProcesses => write!(f, "managed TUI session has no processes"),
        }
    }
}

impl std::error::Error for DevTuiError {}

impl From<io::Error> for DevTuiError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ProcessManagerError> for DevTuiError {
    fn from(value: ProcessManagerError) -> Self {
        Self::Process(value)
    }
}

impl From<UiError> for DevTuiError {
    fn from(value: UiError) -> Self {
        Self::Ui(value)
    }
}

pub fn run_dev_process_tui(
    repo_root: PathBuf,
    processes: Vec<ProcessSpec>,
    tab_order: Vec<String>,
    options: DevTuiOptions,
) -> Result<DevTuiOutcome, DevTuiError> {
    if processes.is_empty() {
        return Err(DevTuiError::NoProcesses);
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

    let result: Result<(), DevTuiError> = loop {
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
    Ok(DevTuiOutcome { non_zero_exits })
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

fn render_ui(
    frame: &mut Frame<'_>,
    process_names: &[String],
    active_index: usize,
    active_logs: &[LogEntry],
    scroll_offset: usize,
    max_offset: usize,
    render_scroll_offset: usize,
    scrollbar_total: usize,
    follow: bool,
    active_process: &str,
    input_line: &str,
    input_mode: InputMode,
    shell_capture_mode: bool,
    exit_states: &HashMap<String, ProcessExitState>,
    show_help: bool,
    show_options: bool,
    options_index: usize,
    active_output_seen: bool,
    spinner_tick: usize,
    active_elapsed: Duration,
    active_restart_count: usize,
    shell_cursor: Option<(u16, u16)>,
) {
    let active_is_shell = active_process == "shell";
    let input_height = if active_is_shell {
        0
    } else if input_mode == InputMode::Insert {
        3
    } else {
        0
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(input_height),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let titles = process_names
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let label = if name == "shell" {
                if shell_capture_mode {
                    "shell [live]".to_owned()
                } else {
                    "shell".to_owned()
                }
            } else {
                name.clone()
            };
            let style = match exit_states.get(name) {
                Some(ProcessExitState::Success) => Style::default().fg(Color::Green),
                Some(ProcessExitState::Failure) => Style::default().fg(Color::Red),
                None => {
                    if name == "shell" && shell_capture_mode && idx == active_index {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else if idx == active_index {
                        Style::default().fg(Color::Magenta)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }
                }
            };
            Line::from(Span::styled(label, style))
        })
        .collect::<Vec<Line>>();
    let tabs = Tabs::new(titles)
        .select(active_index)
        .block(panel_block(Some(" EFFIGY "), true, Color::Magenta))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, chunks[0]);

    let output_height = chunks[1].height.saturating_sub(2) as usize;
    let mut lines = Vec::with_capacity(active_logs.len() + 1);
    if !active_is_shell {
        lines.push(runtime_meta_line(active_elapsed, active_restart_count));
    }
    lines.extend(active_logs.iter().map(|entry| match entry.kind {
        LogEntryKind::Stdout => ansi_line(&entry.line, Style::default()),
        LogEntryKind::Stderr => {
            let mut spans = vec![Span::styled("[stderr] ", Style::default().fg(Color::Red))];
            spans.extend(ansi_line(&entry.line, Style::default()).spans);
            Line::from(spans)
        }
        LogEntryKind::Exit => Line::from(vec![
            Span::styled("[exit] ", Style::default().fg(Color::Yellow)),
            Span::styled(entry.line.clone(), Style::default().fg(Color::Gray)),
        ]),
    }));
    if show_help {
        let help_lines = vec![
            Line::from(vec![Span::styled(
                "Command Mode",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("tab             toggle insert/command mode"),
            Line::from("tab             enter shell capture mode (shell tab)"),
            Line::from("ctrl+g          toggle shell capture mode (shell tab)"),
            Line::from("left/right       switch process tabs"),
            Line::from("up/down          scroll output line-by-line"),
            Line::from("pgup/pgdn        scroll output by page"),
            Line::from("home/end         jump to top/bottom (end re-enables follow)"),
            Line::from("h               toggle this help"),
            Line::from("o               open per-process options menu"),
            Line::from("ctrl+c          quit and shut down managed processes"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Insert Mode",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("type            send input text to active process"),
            Line::from("enter           submit input"),
            Line::from("esc             return to command mode"),
        ];
        let help =
            Paragraph::new(help_lines).block(panel_block(Some("Help"), false, Color::Magenta));
        frame.render_widget(help, chunks[1]);
    } else {
        let panel = panel_block(None, false, Color::DarkGray);
        let logs = if !active_output_seen && !exit_states.contains_key(&process_names[active_index])
        {
            let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let spinner = spinner_frames[spinner_tick % spinner_frames.len()];
            Paragraph::new(vec![
                runtime_meta_line(active_elapsed, active_restart_count),
                Line::from(vec![
                    Span::styled(spinner.to_owned(), Style::default().fg(Color::Yellow)),
                    Span::styled(
                        " waiting for first output...",
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
            ])
            .block(panel)
        } else {
            Paragraph::new(lines)
                .block(panel)
                .scroll((render_scroll_offset.min(u16::MAX as usize) as u16, 0))
        };
        frame.render_widget(logs, chunks[1]);
        // Blink shell caret at ~500ms cadence (event loop ticks every ~50ms).
        let show_shell_caret = (spinner_tick / 10).is_multiple_of(2);
        if active_is_shell && shell_capture_mode && show_shell_caret {
            if let Some((row, col)) = shell_cursor {
                let inner_x = chunks[1].x.saturating_add(1);
                let inner_y = chunks[1].y.saturating_add(1);
                let inner_w = chunks[1].width.saturating_sub(2);
                let inner_h = chunks[1].height.saturating_sub(2);
                if inner_w > 0 && inner_h > 0 {
                    let cursor_x = inner_x.saturating_add(col.min(inner_w.saturating_sub(1)));
                    let cursor_y = inner_y.saturating_add(row.min(inner_h.saturating_sub(1)));
                    frame.set_cursor_position((cursor_x, cursor_y));
                }
            }
        }
        if active_output_seen || exit_states.contains_key(&process_names[active_index]) {
            let mut scrollbar_state = ScrollbarState::new(scrollbar_total.max(1))
                .viewport_content_length(output_height.max(1))
                .position(scroll_offset.min(max_offset));
            frame.render_stateful_widget(
                Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
                chunks[1],
                &mut scrollbar_state,
            );
        }
    }

    if show_options {
        render_options_overlay(
            frame,
            process_names[active_index].as_str(),
            options_index,
            follow,
        );
    }

    let input = if active_is_shell {
        Paragraph::new("")
    } else if input_mode == InputMode::Insert {
        let mut spans = vec![Span::styled("> ", Style::default().fg(Color::Yellow))];
        spans.push(Span::styled(
            input_line.to_owned(),
            Style::default().fg(Color::Gray),
        ));
        spans.push(Span::styled("▏", Style::default().fg(Color::Yellow)));
        Paragraph::new(Line::from(spans)).block(panel_block(
            Some("Input (Esc command, Enter send)"),
            false,
            Color::Magenta,
        ))
    } else {
        Paragraph::new("")
    };
    frame.render_widget(input, chunks[2]);

    let mode_label = if input_mode == InputMode::Insert {
        "insert"
    } else {
        "command"
    };
    let muted = Style::default().fg(Color::DarkGray);
    let active = Style::default().fg(Color::Yellow);
    let mut footer_spans = vec![
        Span::styled(
            if active_is_shell {
                format!(
                    "mode:{} (ctrl+g)",
                    if shell_capture_mode {
                        "shell"
                    } else {
                        "command"
                    }
                )
            } else {
                format!("mode:{mode_label} (tab)")
            },
            if (active_is_shell && shell_capture_mode) || input_mode == InputMode::Insert {
                active
            } else {
                muted
            },
        ),
        Span::styled("  |  ", muted),
        Span::styled("help (h)", if show_help { active } else { muted }),
        Span::styled("  |  ", muted),
        Span::styled("options (o)", if show_options { active } else { muted }),
    ];
    if active_is_shell {
        footer_spans.push(Span::styled("  |  ", muted));
        footer_spans.push(Span::styled(
            if shell_capture_mode {
                "shell: live (ctrl+g to exit)"
            } else {
                "shell: command (tab/ctrl+g to enter)"
            },
            active,
        ));
    }
    let footer = Paragraph::new(Line::from(footer_spans));
    frame.render_widget(footer, chunks[3]);
}

fn panel_block<'a>(title: Option<&'a str>, show_version: bool, border_color: Color) -> Block<'a> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(border_color));
    if let Some(title) = title {
        block = block.title_top(
            Line::from(Span::styled(
                title.to_owned(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ))
            .left_aligned(),
        );
    }
    if show_version {
        let version = format!(" v{} ", env!("CARGO_PKG_VERSION"));
        block = block.title_bottom(
            Line::from(Span::styled(
                version,
                Style::default().fg(Color::LightMagenta),
            ))
            .right_aligned(),
        );
    }
    block
}

const OPTIONS_ACTIONS: [OptionsAction; 5] = [
    OptionsAction::ToggleFollow,
    OptionsAction::Restart,
    OptionsAction::Stop,
    OptionsAction::Cancel,
    OptionsAction::Quit,
];

fn options_actions(_follow_enabled: bool) -> Vec<OptionsAction> {
    OPTIONS_ACTIONS.to_vec()
}

fn options_action_label(action: OptionsAction, follow_enabled: bool) -> &'static str {
    match action {
        OptionsAction::ToggleFollow => {
            if follow_enabled {
                "Disable follow (f)"
            } else {
                "Enable follow (f)"
            }
        }
        OptionsAction::Restart => "Restart process (r)",
        OptionsAction::Stop => "Stop process (s)",
        OptionsAction::Cancel => "Cancel (o)",
        OptionsAction::Quit => "Quit (q)",
    }
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
) -> Result<bool, DevTuiError> {
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

fn render_options_overlay(
    frame: &mut Frame<'_>,
    process: &str,
    selected: usize,
    follow_enabled: bool,
) {
    let area = centered_rect(54, 44, frame.area());
    frame.render_widget(Clear, area);
    let rows = options_actions(follow_enabled)
        .iter()
        .enumerate()
        .map(|(idx, action)| {
            let marker = if idx == selected { "› " } else { "  " };
            let style = if idx == selected {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Gray)
            };
            Line::from(Span::styled(
                format!("{marker}{}", options_action_label(*action, follow_enabled)),
                style,
            ))
        })
        .collect::<Vec<Line>>();
    let block = panel_block(Some(" Options "), false, Color::Magenta);
    let paragraph = Paragraph::new({
        let mut lines = vec![Line::from(Span::styled(
            format!("process: {process}"),
            Style::default().fg(Color::DarkGray),
        ))];
        lines.push(Line::from(""));
        lines.extend(rows);
        lines
    })
    .block(block);
    frame.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
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

fn vt_logs(
    parser: &mut VtParser,
    panel_rows: usize,
    panel_cols: usize,
    ui_scroll_offset: usize,
    follow: bool,
) -> (Vec<LogEntry>, usize, usize) {
    let safe_rows = panel_rows.max(1);
    parser.set_size(safe_rows as u16, panel_cols.max(1) as u16);
    // vt100 0.15.x can panic when scrollback offset exceeds visible row count.
    // Clamp to a safe range until we move to a parser version without this bug.
    let max_offset = vt_max_scrollback(parser).min(safe_rows.saturating_sub(1));
    let clamped = if follow {
        max_offset
    } else {
        ui_scroll_offset.min(max_offset)
    };
    let vt_scrollback = max_offset.saturating_sub(clamped);
    parser.set_scrollback(vt_scrollback);
    let rows = parser
        .screen()
        .rows_formatted(0, panel_cols.max(1) as u16)
        .map(|row| LogEntry {
            kind: LogEntryKind::Stdout,
            line: String::from_utf8_lossy(&row).into_owned(),
        })
        .collect::<Vec<LogEntry>>();
    (rows, clamped, max_offset)
}

fn vt_max_scrollback(parser: &mut VtParser) -> usize {
    let current = parser.screen().scrollback();
    parser.set_scrollback(usize::MAX);
    let max = parser.screen().scrollback();
    parser.set_scrollback(current);
    max
}

fn push_entry(buffer: &mut VecDeque<LogEntry>, entry: LogEntry) {
    buffer.push_back(entry);
    while buffer.len() > MAX_LOG_LINES {
        buffer.pop_front();
    }
}

fn ingest_log_payload(buffer: &mut VecDeque<LogEntry>, kind: LogEntryKind, payload: &str) {
    let (normalized, cursor_up) = normalize_terminal_payload(payload);
    let fragments = normalized
        .split('\r')
        .map(sanitize_log_text)
        .filter(|line| !line.is_empty())
        .collect::<Vec<String>>();
    if fragments.is_empty() {
        return;
    }

    if fragments.len() == 1 && !normalized.contains('\r') {
        if cursor_up > 0 {
            replace_last_renderable_line(buffer, kind, fragments[0].clone());
        } else {
            push_entry(
                buffer,
                LogEntry {
                    kind,
                    line: fragments[0].clone(),
                },
            );
        }
        return;
    }

    let mut append_on_first_rewrite = false;
    let mut first = true;
    for fragment in fragments {
        if first {
            if cursor_up > 0 {
                replace_last_renderable_line(buffer, kind.clone(), fragment);
            } else {
                push_entry(
                    buffer,
                    LogEntry {
                        kind: kind.clone(),
                        line: fragment,
                    },
                );
            }
            first = false;
            continue;
        }
        if append_on_first_rewrite {
            push_entry(
                buffer,
                LogEntry {
                    kind: kind.clone(),
                    line: fragment,
                },
            );
            append_on_first_rewrite = false;
        } else {
            replace_last_renderable_line(buffer, kind.clone(), fragment);
        }
    }
}

fn normalize_terminal_payload(raw: &str) -> (String, usize) {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    let mut cursor_up = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\u{1b}' && i + 1 < chars.len() {
            match chars[i + 1] {
                '[' => {
                    let start = i;
                    i += 2;
                    let mut params = String::new();
                    while i < chars.len() {
                        let final_byte = chars[i];
                        if ('@'..='~').contains(&final_byte) {
                            if final_byte == 'm' {
                                out.extend(chars[start..=i].iter());
                            } else if final_byte == 'A' {
                                let count = params
                                    .split(';')
                                    .next()
                                    .and_then(|value| {
                                        if value.is_empty() {
                                            Some(1usize)
                                        } else {
                                            value.parse::<usize>().ok()
                                        }
                                    })
                                    .unwrap_or(1usize);
                                cursor_up = cursor_up.saturating_add(count);
                            }
                            break;
                        }
                        params.push(final_byte);
                        i += 1;
                    }
                }
                ']' => {
                    i += 2;
                    while i < chars.len() {
                        if chars[i] == '\u{0007}' {
                            break;
                        }
                        if chars[i] == '\u{1b}' && i + 1 < chars.len() && chars[i + 1] == '\\' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {}
            }
        } else {
            out.push(ch);
        }
        i += 1;
    }
    (out, cursor_up)
}

fn replace_last_renderable_line(buffer: &mut VecDeque<LogEntry>, kind: LogEntryKind, line: String) {
    if let Some(last) = buffer.back_mut() {
        if matches!(last.kind, LogEntryKind::Stdout | LogEntryKind::Stderr) {
            last.kind = kind;
            last.line = line;
            return;
        }
    }
    push_entry(buffer, LogEntry { kind, line });
}

fn sanitize_log_text(raw: &str) -> String {
    raw.chars()
        .filter(|ch| {
            !matches!(
                ch,
                '\r'
                    | '\u{0000}'..='\u{0008}'
                    | '\u{000B}'
                    | '\u{000C}'
                    | '\u{000E}'..='\u{001A}'
                    | '\u{001C}'..='\u{001F}'
                    | '\u{007F}'
            )
        })
        .collect()
}

fn ansi_line(raw: &str, base: Style) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut style = base;
    let mut buf = String::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '\u{1b}' && i + 1 < chars.len() && chars[i + 1] == '[' {
            if !buf.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut buf), style));
            }
            i += 2;
            let mut code = String::new();
            while i < chars.len() {
                let final_byte = chars[i];
                if ('@'..='~').contains(&final_byte) {
                    if final_byte == 'm' {
                        style = apply_sgr(style, &code, base);
                    }
                    break;
                }
                code.push(chars[i]);
                i += 1;
            }
        } else {
            buf.push(chars[i]);
        }
        i += 1;
    }
    if !buf.is_empty() {
        spans.push(Span::styled(buf, style));
    }
    if spans.is_empty() {
        return Line::from("");
    }
    Line::from(spans)
}

fn apply_sgr(current: Style, sgr: &str, base: Style) -> Style {
    let mut style = current;
    let parts = if sgr.is_empty() {
        vec!["0"]
    } else {
        sgr.split(';').collect::<Vec<&str>>()
    };
    for part in parts {
        match part.parse::<u8>() {
            Ok(0) => style = base,
            Ok(1) => style = style.add_modifier(Modifier::BOLD),
            Ok(2) => style = style.add_modifier(Modifier::DIM),
            Ok(3) => style = style.add_modifier(Modifier::ITALIC),
            Ok(4) => style = style.add_modifier(Modifier::UNDERLINED),
            Ok(22) => style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            Ok(23) => style = style.remove_modifier(Modifier::ITALIC),
            Ok(24) => style = style.remove_modifier(Modifier::UNDERLINED),
            Ok(30) => style = style.fg(Color::Black),
            Ok(31) => style = style.fg(Color::Red),
            Ok(32) => style = style.fg(Color::Green),
            Ok(33) => style = style.fg(Color::Yellow),
            Ok(34) => style = style.fg(Color::Blue),
            Ok(35) => style = style.fg(Color::Magenta),
            Ok(36) => style = style.fg(Color::Cyan),
            Ok(37) => style = style.fg(Color::Gray),
            Ok(39) => style = style.fg(base.fg.unwrap_or(Color::Reset)),
            Ok(90) => style = style.fg(Color::DarkGray),
            Ok(91) => style = style.fg(Color::LightRed),
            Ok(92) => style = style.fg(Color::LightGreen),
            Ok(93) => style = style.fg(Color::LightYellow),
            Ok(94) => style = style.fg(Color::LightBlue),
            Ok(95) => style = style.fg(Color::LightMagenta),
            Ok(96) => style = style.fg(Color::LightCyan),
            Ok(97) => style = style.fg(Color::White),
            _ => {}
        }
    }
    style
}

fn is_expected_shutdown_diagnostic(diagnostic: &str) -> bool {
    matches!(diagnostic, "signal=15" | "signal=9")
}

fn format_elapsed(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}h{minutes:02}m{secs:02}s")
    } else if minutes > 0 {
        format!("{minutes}m{secs:02}s")
    } else {
        format!("{secs}s")
    }
}

fn runtime_meta_line(elapsed: Duration, restart_count: usize) -> Line<'static> {
    let label = if restart_count == 0 {
        "started"
    } else {
        "restarted"
    };
    Line::from(vec![
        Span::styled(format!("{label}: "), Style::default().fg(Color::LightBlue)),
        Span::styled(
            format!("{} ago", format_elapsed(elapsed)),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

fn styled_text(style: AnsiStyle, text: &str) -> String {
    format!("{}{}{}", style.render(), text, style.render_reset())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_line_parses_basic_colour_sequence() {
        let line = ansi_line("\u{1b}[31merror\u{1b}[0m ok", Style::default());
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content.as_ref(), "error");
        assert_eq!(line.spans[1].content.as_ref(), " ok");
    }

    #[test]
    fn expected_shutdown_diagnostics_are_ignored() {
        assert!(is_expected_shutdown_diagnostic("signal=15"));
        assert!(is_expected_shutdown_diagnostic("signal=9"));
        assert!(!is_expected_shutdown_diagnostic("exit=1"));
        assert!(!is_expected_shutdown_diagnostic("signal=11"));
    }

    #[test]
    fn format_elapsed_uses_compact_human_time() {
        assert_eq!(format_elapsed(Duration::from_secs(9)), "9s");
        assert_eq!(format_elapsed(Duration::from_secs(65)), "1m05s");
        assert_eq!(format_elapsed(Duration::from_secs(3665)), "1h01m05s");
    }

    #[test]
    fn runtime_meta_line_marks_restart_state() {
        let started = runtime_meta_line(Duration::from_secs(9), 0);
        assert_eq!(started.spans[0].content.as_ref(), "started: ");
        let restarted = runtime_meta_line(Duration::from_secs(9), 1);
        assert_eq!(restarted.spans[0].content.as_ref(), "restarted: ");
    }

    #[test]
    fn sanitize_log_text_removes_control_bytes_but_keeps_ansi() {
        let raw = "a\u{0008}b\r\u{001b}[31merr\u{001b}[0m";
        let sanitized = sanitize_log_text(raw);
        assert_eq!(sanitized, "ab\u{001b}[31merr\u{001b}[0m");
    }

    #[test]
    fn ingest_log_payload_carriage_return_overwrites_last_line() {
        let mut buffer = VecDeque::new();
        ingest_log_payload(
            &mut buffer,
            LogEntryKind::Stdout,
            "building\rfinished\rdone",
        );
        assert_eq!(buffer.len(), 1);
        let line = buffer.back().expect("line");
        assert!(matches!(line.kind, LogEntryKind::Stdout));
        assert_eq!(line.line, "done");
    }

    #[test]
    fn ingest_log_payload_cursor_up_replaces_prior_line() {
        let mut buffer = VecDeque::new();
        ingest_log_payload(&mut buffer, LogEntryKind::Stdout, "line 1");
        ingest_log_payload(&mut buffer, LogEntryKind::Stdout, "line 2");
        ingest_log_payload(
            &mut buffer,
            LogEntryKind::Stdout,
            "\u{1b}[1A\u{1b}[2K\rline 2 updated",
        );
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer[0].line, "line 1");
        assert_eq!(buffer[1].line, "line 2 updated");
    }

    #[test]
    fn ingest_log_payload_cursor_up_without_replacement_does_not_drop_lines() {
        let mut buffer = VecDeque::new();
        ingest_log_payload(&mut buffer, LogEntryKind::Stdout, "line 1");
        ingest_log_payload(&mut buffer, LogEntryKind::Stdout, "line 2");
        ingest_log_payload(&mut buffer, LogEntryKind::Stdout, "\u{1b}[1A\u{1b}[2K");
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer[0].line, "line 1");
        assert_eq!(buffer[1].line, "line 2");
    }

    #[test]
    fn ansi_line_ignores_non_sgr_escape_sequences() {
        let line = ansi_line(
            "\u{1b}[2K\u{1b}[1Ahello \u{1b}[31mred\u{1b}[0m",
            Style::default(),
        );
        let rendered = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert_eq!(rendered, "hello red");
    }

    #[test]
    fn vt_logs_trims_empty_padding_lines() {
        let mut parser = VtParser::new(8, 40, 100);
        parser.process(b"\n\nhello\nworld\n\n");
        let (rows, _, _) = vt_logs(&mut parser, 8, 40, 0, true);
        assert!(rows.iter().any(|line| line.line.contains("hello")));
        assert!(rows.iter().any(|line| line.line.contains("world")));
    }

    #[test]
    fn vt_logs_clamps_overscroll_without_panicking() {
        let mut parser = VtParser::new(8, 40, 200);
        for i in 0..200 {
            parser.process(format!("line-{i}\n").as_bytes());
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            vt_logs(&mut parser, 8, 40, usize::MAX / 2, false)
        }));
        assert!(result.is_ok(), "overscroll should be clamped safely");
    }

    #[test]
    fn all_processes_exited_requires_full_count() {
        let mut exits = HashMap::new();
        exits.insert("a".to_owned(), ProcessExitState::Success);
        assert!(!all_processes_exited(&exits, 2));
        exits.insert("b".to_owned(), ProcessExitState::Failure);
        assert!(all_processes_exited(&exits, 2));
    }
}
