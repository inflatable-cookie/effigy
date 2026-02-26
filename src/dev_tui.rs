use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Tabs, Wrap};
use ratatui::{Frame, Terminal};

use crate::process_manager::{ProcessEventKind, ProcessManagerError, ProcessSpec, ProcessSupervisor};

const MAX_LOG_LINES: usize = 2000;

#[derive(Debug)]
pub enum DevTuiError {
    Io(io::Error),
    Process(ProcessManagerError),
    NoProcesses,
}

impl std::fmt::Display for DevTuiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DevTuiError::Io(err) => write!(f, "{err}"),
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

pub fn run_dev_process_tui(repo_root: PathBuf, processes: Vec<ProcessSpec>) -> Result<(), DevTuiError> {
    if processes.is_empty() {
        return Err(DevTuiError::NoProcesses);
    }

    let process_names = processes
        .iter()
        .map(|process| process.name.clone())
        .collect::<Vec<String>>();
    let expected = process_names.len();
    let supervisor = ProcessSupervisor::spawn(repo_root, processes)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut logs: HashMap<String, VecDeque<String>> = process_names
        .iter()
        .map(|name| (name.clone(), VecDeque::new()))
        .collect();
    let mut active_index: usize = 0;
    let mut input_line = String::new();
    let mut exit_count = 0usize;

    let result = loop {
        while let Some(event_item) = supervisor.next_event_timeout(Duration::from_millis(1)) {
            if let Some(buffer) = logs.get_mut(&event_item.process) {
                let line = match event_item.kind {
                    ProcessEventKind::Stdout => event_item.payload,
                    ProcessEventKind::Stderr => format!("[stderr] {}", event_item.payload),
                    ProcessEventKind::Exit => {
                        exit_count += 1;
                        format!("[exit] {}", event_item.payload)
                    }
                };
                buffer.push_back(line);
                while buffer.len() > MAX_LOG_LINES {
                    buffer.pop_front();
                }
            }
        }

        let active = &process_names[active_index];
        let active_logs = logs
            .get(active)
            .map(|entries| entries.iter().cloned().collect::<Vec<String>>())
            .unwrap_or_default();
        let status = format!(
            "task manager  tab {}/{}  exits {}/{}",
            active_index + 1,
            process_names.len(),
            exit_count,
            expected
        );

        terminal.draw(|frame| render_ui(frame, &process_names, active_index, &active_logs, &input_line, &status))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => break Ok(()),
                    KeyCode::Tab => {
                        active_index = (active_index + 1) % process_names.len();
                    }
                    KeyCode::BackTab => {
                        active_index = if active_index == 0 {
                            process_names.len() - 1
                        } else {
                            active_index - 1
                        };
                    }
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
                        input_line.clear();
                    }
                    KeyCode::Char(c) => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                            break Ok(());
                        }
                        input_line.push(c);
                    }
                    _ => {}
                }
            }
        }
    };

    supervisor.terminate_all();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn render_ui(
    frame: &mut Frame<'_>,
    process_names: &[String],
    active_index: usize,
    active_logs: &[String],
    input_line: &str,
    status: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let titles = process_names
        .iter()
        .map(|name| Line::from(name.clone()))
        .collect::<Vec<Line>>();
    let tabs = Tabs::new(titles)
        .select(active_index)
        .block(Block::default().borders(Borders::ALL).title("Processes"))
        .highlight_style(
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, chunks[0]);

    let logs = Paragraph::new(active_logs.join("\n"))
        .block(Block::default().borders(Borders::ALL).title("Output"))
        .wrap(Wrap { trim: false });
    frame.render_widget(logs, chunks[1]);

    let input = Paragraph::new(input_line.to_owned())
        .block(Block::default().borders(Borders::ALL).title("Input (Enter sends to active tab)"));
    frame.render_widget(input, chunks[2]);

    let footer = Paragraph::new(format!("{status}  |  tab/backtab switch  |  q quit"))
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[3]);
}
