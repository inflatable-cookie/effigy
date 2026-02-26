use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Tabs,
};
use ratatui::{Frame, Terminal};

use crate::process_manager::{
    ProcessEventKind, ProcessManagerError, ProcessSpec, ProcessSupervisor, ShutdownProgress,
};
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, UiError,
};

const MAX_LOG_LINES: usize = 2000;

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
    let mut active_index: usize = 0;
    let mut input_line = String::new();
    let mut input_mode = InputMode::Command;
    let mut show_help = false;
    let mut shutdown_summary: Option<String> = None;
    let mut shutdown_total: Option<usize> = None;
    let mut shutdown_forced: Option<usize> = None;
    let mut observed_non_zero: HashMap<String, String> = HashMap::new();
    let mut exit_states: HashMap<String, ProcessExitState> = HashMap::new();

    let result: Result<(), DevTuiError> = loop {
        while let Some(event_item) = supervisor.next_event_timeout(Duration::from_millis(1)) {
            if let Some(buffer) = logs.get_mut(&event_item.process) {
                let entry = match event_item.kind {
                    ProcessEventKind::Stdout => LogEntry {
                        kind: LogEntryKind::Stdout,
                        line: event_item.payload,
                    },
                    ProcessEventKind::Stderr => LogEntry {
                        kind: LogEntryKind::Stderr,
                        line: event_item.payload,
                    },
                    ProcessEventKind::Exit => {
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
                        LogEntry {
                            kind: LogEntryKind::Exit,
                            line: event_item.payload,
                        }
                    }
                };
                buffer.push_back(entry);
                while buffer.len() > MAX_LOG_LINES {
                    buffer.pop_front();
                }
            }
        }

        let active = &process_names[active_index];
        let active_logs = logs
            .get(active)
            .map(|entries| entries.iter().cloned().collect::<Vec<LogEntry>>())
            .unwrap_or_default();
        let size = terminal.size()?;
        let output_height = size.height.saturating_sub(9) as usize;
        let max_offset = active_logs.len().saturating_sub(output_height);
        let stored = *scroll_offsets.get(active).unwrap_or(&0usize);
        let scroll_offset = stored.min(max_offset);
        scroll_offsets.insert(active.clone(), scroll_offset);
        let is_follow = *follow_mode.get(active).unwrap_or(&true);
        let status = format!("follow: {} (f)", if is_follow { "on" } else { "off" },);

        terminal.draw(|frame| {
            render_ui(
                frame,
                &process_names,
                active_index,
                &active_logs,
                scroll_offset,
                is_follow,
                &input_line,
                input_mode,
                &status,
                &exit_states,
                show_help,
            )
        })?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('c'))
                {
                    break Ok(());
                }
                if matches!(key.code, KeyCode::Tab) {
                    input_mode = if input_mode == InputMode::Insert {
                        InputMode::Command
                    } else {
                        InputMode::Insert
                    };
                    if input_mode == InputMode::Insert {
                        show_help = false;
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
                        input_mode = InputMode::Insert;
                        show_help = false;
                    }
                    KeyCode::Char('h') => {
                        show_help = !show_help;
                    }
                    KeyCode::BackTab => {
                        active_index = if active_index == 0 {
                            process_names.len() - 1
                        } else {
                            active_index - 1
                        };
                    }
                    KeyCode::Right => {
                        active_index = (active_index + 1) % process_names.len();
                    }
                    KeyCode::Left => {
                        active_index = if active_index == 0 {
                            process_names.len() - 1
                        } else {
                            active_index - 1
                        };
                    }
                    KeyCode::Char('f') => {
                        let active = &process_names[active_index];
                        if let Some(follow) = follow_mode.get_mut(active) {
                            *follow = !*follow;
                        }
                        if *follow_mode.get(active).unwrap_or(&false) {
                            if let Some(offset) = scroll_offsets.get_mut(active) {
                                *offset = max_offset;
                            }
                        }
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
            ShutdownProgress::Complete { total, forced } => {
                shutdown_total = Some(total);
                shutdown_summary = Some(format_shutdown_summary(total, forced));
                shutdown_forced = Some(forced);
                shutdown_summary.as_deref().unwrap_or("Shutdown: complete.")
            }
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

    let total = shutdown_total.unwrap_or(process_names.len());
    let forced = shutdown_forced.unwrap_or(0);
    let graceful = total.saturating_sub(forced);
    let mut renderer = PlainRenderer::stdout(OutputMode::from_env());
    renderer.section("Managed Session Ended")?;
    renderer.key_values(&[
        KeyValue::new("processes", total.to_string()),
        KeyValue::new("graceful", graceful.to_string()),
        KeyValue::new("forced", forced.to_string()),
    ])?;
    if forced > 0 {
        renderer.notice(
            NoticeLevel::Warning,
            &format!("forced termination used for {forced} process(es)."),
        )?;
    }
    if !non_zero_exits.is_empty() {
        renderer.text("")?;
        renderer.notice(NoticeLevel::Warning, "managed process exits:")?;
        let items = non_zero_exits
            .iter()
            .map(|(name, diagnostic)| format!("{name}: {diagnostic}"))
            .collect::<Vec<String>>();
        renderer.bullet_list("exits", &items)?;
    }
    renderer.text("")?;
    renderer.summary(SummaryCounts {
        ok: graceful,
        warn: if forced > 0 || !non_zero_exits.is_empty() {
            1
        } else {
            0
        },
        err: non_zero_exits.len(),
    })?;
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

fn render_ui(
    frame: &mut Frame<'_>,
    process_names: &[String],
    active_index: usize,
    active_logs: &[LogEntry],
    scroll_offset: usize,
    follow: bool,
    input_line: &str,
    input_mode: InputMode,
    status: &str,
    exit_states: &HashMap<String, ProcessExitState>,
    show_help: bool,
) {
    let input_height = if input_mode == InputMode::Insert {
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
            let style = match exit_states.get(name) {
                Some(ProcessExitState::Success) => Style::default().fg(Color::Green),
                Some(ProcessExitState::Failure) => Style::default().fg(Color::Red),
                None => {
                    if idx == active_index {
                        Style::default().fg(Color::Magenta)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }
                }
            };
            Line::from(Span::styled(name.clone(), style))
        })
        .collect::<Vec<Line>>();
    let tabs = Tabs::new(titles)
        .select(active_index)
        .block(panel_block(Some(" EFFIGY "), true, Color::Magenta))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, chunks[0]);

    let output_height = chunks[1].height.saturating_sub(2) as usize;
    let total_lines = active_logs.len();
    let max_offset = total_lines.saturating_sub(output_height);
    let clamped_offset = if follow {
        max_offset
    } else {
        scroll_offset.min(max_offset)
    };

    let lines = active_logs
        .iter()
        .map(|entry| match entry.kind {
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
        })
        .collect::<Vec<Line>>();

    if show_help {
        let help_lines = vec![
            Line::from(vec![Span::styled(
                "Command Mode",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("tab             toggle insert/command mode"),
            Line::from("left/right       switch process tabs"),
            Line::from("up/down          scroll output line-by-line"),
            Line::from("pgup/pgdn        scroll output by page"),
            Line::from("home/end         jump to top/bottom (end re-enables follow)"),
            Line::from("f               toggle follow for active tab"),
            Line::from("h               toggle this help"),
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
        let logs = Paragraph::new(lines)
            .block(panel_block(None, false, Color::DarkGray))
            .scroll((clamped_offset.min(u16::MAX as usize) as u16, 0));
        frame.render_widget(logs, chunks[1]);
        let mut scrollbar_state = ScrollbarState::new(total_lines.max(1))
            .viewport_content_length(output_height.max(1))
            .position(clamped_offset);
        frame.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            chunks[1],
            &mut scrollbar_state,
        );
    }

    let input = if input_mode == InputMode::Insert {
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
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(status.to_owned(), if follow { active } else { muted }),
        Span::styled("  |  ", muted),
        Span::styled(
            format!("mode:{mode_label} (tab)"),
            if input_mode == InputMode::Insert {
                active
            } else {
                muted
            },
        ),
        Span::styled("  |  ", muted),
        Span::styled("help (h)", if show_help { active } else { muted }),
    ]));
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
            while i < chars.len() && chars[i] != 'm' {
                code.push(chars[i]);
                i += 1;
            }
            if i < chars.len() && chars[i] == 'm' {
                style = apply_sgr(style, &code, base);
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

fn format_shutdown_summary(total: usize, forced: usize) -> String {
    let graceful = total.saturating_sub(forced);
    if forced == 0 {
        format!("effigy: shutdown complete ({graceful}/{total} graceful, 0 forced)")
    } else {
        format!("effigy: shutdown complete ({graceful}/{total} graceful, {forced} forced)")
    }
}

fn is_expected_shutdown_diagnostic(diagnostic: &str) -> bool {
    matches!(diagnostic, "signal=15" | "signal=9")
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
    fn format_shutdown_summary_reports_graceful_and_forced_counts() {
        assert_eq!(
            format_shutdown_summary(4, 0),
            "effigy: shutdown complete (4/4 graceful, 0 forced)"
        );
        assert_eq!(
            format_shutdown_summary(4, 1),
            "effigy: shutdown complete (3/4 graceful, 1 forced)"
        );
    }

    #[test]
    fn expected_shutdown_diagnostics_are_ignored() {
        assert!(is_expected_shutdown_diagnostic("signal=15"));
        assert!(is_expected_shutdown_diagnostic("signal=9"));
        assert!(!is_expected_shutdown_diagnostic("exit=1"));
        assert!(!is_expected_shutdown_diagnostic("signal=11"));
    }
}
