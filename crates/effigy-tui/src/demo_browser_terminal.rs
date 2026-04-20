use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command as ProcessCommand, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use effigy_cli::DemoSubcommand;
use effigy_demo::browser::DemoDetail;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};

use crate::demo_browser::{
    browser_body_constraints, compact_kv_line, detail_tab_lines, muted_line,
    resolve_repo_relative_path, DetailSelectableItem, DetailTab, DetailRender,
};
use crate::terminal_text::{render_vt_lines, LiveTerminalBuffer};

pub const DEMO_BROWSER_TERMINAL_COLS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_COLS";
pub const DEMO_BROWSER_TERMINAL_ROWS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_ROWS";
pub const DEMO_BROWSER_TERMINAL_PARSER_SCROLLBACK: usize = 2000;
pub const DEMO_BROWSER_TERMINAL_RECENT_LINE_LIMIT: usize = 8;
pub const DEMO_BROWSER_LIVE_TERMINAL_TRANSCRIPT_MAX_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalView {
    pub lines: Vec<Line<'static>>,
    pub source: TerminalViewSource,
    pub scroll_offset: usize,
    pub stderr_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalViewSource {
    LiveAttached,
    ActiveLogs,
    InspectSnapshot,
    LatestAttemptLogs,
    Empty,
}

impl TerminalViewSource {
    pub fn label(self) -> &'static str {
        match self {
            TerminalViewSource::LiveAttached => "live attached",
            TerminalViewSource::ActiveLogs => "live terminal",
            TerminalViewSource::InspectSnapshot => "inspect snapshot",
            TerminalViewSource::LatestAttemptLogs => "latest attempt logs",
            TerminalViewSource::Empty => "none",
        }
    }
}

struct TerminalStreamSource {
    stdout_bytes: Vec<u8>,
    stderr_lines: Vec<String>,
    source: TerminalViewSource,
    terminal_cols: Option<u16>,
    terminal_rows: Option<u16>,
}

pub fn build_terminal_view(
    repo_root: &Path,
    detail: &DemoDetail,
    width: usize,
    height: usize,
    scroll_offset: usize,
) -> TerminalView {
    let width = width.max(1);
    let height = height.max(1);

    if detail.active_terminal_session.available {
        let session = &detail.active_terminal_session;
        let source = terminal_stream_source(
            repo_root,
            session.stdout_log_path.as_deref(),
            session.stderr_log_path.as_deref(),
            session.recent_output.stdout_lines.clone(),
            session.recent_output.stderr_lines.clone(),
            TerminalViewSource::ActiveLogs,
            session.terminal_size.cols,
            session.terminal_size.rows,
        );
        return render_terminal_view_from_source(source, width, height, scroll_offset);
    }

    if detail.latest_attempt.output_available {
        let source = terminal_stream_source(
            repo_root,
            detail.latest_attempt.stdout_log_path.as_deref(),
            detail.latest_attempt.stderr_log_path.as_deref(),
            Vec::new(),
            Vec::new(),
            TerminalViewSource::LatestAttemptLogs,
            None,
            None,
        );
        return render_terminal_view_from_source(source, width, height, scroll_offset);
    }

    TerminalView {
        lines: vec![muted_line(
            "No active or recorded terminal output is available.".to_owned(),
        )],
        source: TerminalViewSource::Empty,
        scroll_offset: 0,
        stderr_lines: Vec::new(),
    }
}

pub fn build_live_terminal_view(
    session: &mut BrowserLiveTerminalSession,
    width: usize,
    height: usize,
    scroll_offset: usize,
) -> TerminalView {
    let (mut lines, clamped_scroll) = session.terminal.render_lines(width, height, scroll_offset);
    if lines.is_empty() {
        lines.push(muted_line(
            "Terminal output exists, but no visible screen content is available yet.".to_owned(),
        ));
    }
    TerminalView {
        lines,
        source: TerminalViewSource::LiveAttached,
        scroll_offset: clamped_scroll,
        stderr_lines: Vec::new(),
    }
}

pub fn terminal_detail_render(
    repo_root: &Path,
    detail: &DemoDetail,
    _selected_item: Option<DetailSelectableItem>,
    _detail_focused: bool,
) -> DetailRender {
    let terminal_view = build_terminal_view(repo_root, detail, 80, 18, 0);
    let mut lines = terminal_status_lines(detail, &terminal_view, false);
    lines.extend(terminal_view.lines);
    DetailRender {
        lines,
        selected_line_index: None,
    }
}

fn terminal_stream_source(
    repo_root: &Path,
    stdout_log_path: Option<&str>,
    stderr_log_path: Option<&str>,
    fallback_stdout_lines: Vec<String>,
    fallback_stderr_lines: Vec<String>,
    source: TerminalViewSource,
    terminal_cols: Option<u16>,
    terminal_rows: Option<u16>,
) -> TerminalStreamSource {
    let stdout_bytes = stdout_log_path
        .map(|path| resolve_repo_relative_path(repo_root, path))
        .and_then(|path| fs::read(path).ok())
        .unwrap_or_default();
    let stderr_lines = stderr_log_path
        .map(|path| resolve_repo_relative_path(repo_root, path))
        .and_then(|path| read_recent_log_lines(&path, DEMO_BROWSER_TERMINAL_RECENT_LINE_LIMIT).ok())
        .unwrap_or_default();
    let using_fallback = stdout_bytes.is_empty() && !fallback_stdout_lines.is_empty()
        || stderr_lines.is_empty() && !fallback_stderr_lines.is_empty();
    let resolved_source = if using_fallback {
        TerminalViewSource::InspectSnapshot
    } else if stdout_log_path.is_some() || stderr_log_path.is_some() {
        source
    } else {
        TerminalViewSource::InspectSnapshot
    };
    TerminalStreamSource {
        stdout_bytes: if stdout_bytes.is_empty() {
            fallback_stdout_lines.join("\n").into_bytes()
        } else {
            stdout_bytes
        },
        stderr_lines: if stderr_lines.is_empty() {
            fallback_stderr_lines
        } else {
            stderr_lines
        },
        source: resolved_source,
        terminal_cols,
        terminal_rows,
    }
}

fn render_terminal_view_from_source(
    source: TerminalStreamSource,
    width: usize,
    height: usize,
    scroll_offset: usize,
) -> TerminalView {
    render_terminal_view_from_bytes(
        &source.stdout_bytes,
        source.source,
        width,
        height,
        scroll_offset,
        source.terminal_cols,
        source.terminal_rows,
        source.stderr_lines,
    )
}

fn render_terminal_view_from_bytes(
    stdout_bytes: &[u8],
    source: TerminalViewSource,
    width: usize,
    height: usize,
    scroll_offset: usize,
    terminal_cols: Option<u16>,
    terminal_rows: Option<u16>,
    stderr_lines: Vec<String>,
) -> TerminalView {
    let parser_rows = terminal_rows.unwrap_or(height as u16).max(1);
    let parser_cols = terminal_cols.unwrap_or(width as u16).max(1);
    let mut parser = vt100::Parser::new(
        parser_rows,
        parser_cols,
        DEMO_BROWSER_TERMINAL_PARSER_SCROLLBACK,
    );
    if !stdout_bytes.is_empty() {
        parser.process(stdout_bytes);
    }
    let (mut lines, clamped_scroll) = browser_vt_lines(&mut parser, width, height, scroll_offset);
    if lines.is_empty() {
        lines.push(muted_line(
            "Terminal output exists, but no visible screen content is available yet.".to_owned(),
        ));
    }
    if !stderr_lines.is_empty() {
        lines.push(Line::from(""));
        lines.push(compact_kv_line("stderr", "recent lines"));
        for line in stderr_lines
            .iter()
            .take(DEMO_BROWSER_TERMINAL_RECENT_LINE_LIMIT)
        {
            lines.push(Line::from(line.clone()));
        }
    }
    TerminalView {
        lines,
        source,
        scroll_offset: clamped_scroll,
        stderr_lines,
    }
}

pub fn browser_vt_lines(
    parser: &mut vt100::Parser,
    width: usize,
    height: usize,
    scroll_offset: usize,
) -> (Vec<Line<'static>>, usize) {
    render_vt_lines(parser, width, height, scroll_offset)
}

pub fn terminal_status_lines(
    detail: &DemoDetail,
    terminal_view: &TerminalView,
    input_mode: bool,
) -> Vec<Line<'static>> {
    let session = &detail.active_terminal_session;
    let transport = if session.available {
        session.transport.as_str()
    } else if detail.latest_attempt.output_available {
        "recorded"
    } else {
        "none"
    };
    let input_label = if input_mode {
        "capturing keys"
    } else if session.available && session.supports_input_forwarding {
        "available"
    } else if session.available {
        session
            .input_forwarding_reason
            .as_deref()
            .unwrap_or("unavailable")
    } else {
        "inactive"
    };
    vec![
        Line::from(vec![
            Span::styled(
                " source: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(terminal_view.source.label()),
            Span::raw("   "),
            Span::styled(
                " transport: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(transport.to_owned()),
            Span::raw("   "),
            Span::styled(
                " size: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(
                session
                    .terminal_size
                    .rendered()
                    .unwrap_or_else(|| "unknown".to_owned()),
            ),
            Span::raw("   "),
            Span::styled(
                " input: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(input_label.to_owned()),
        ]),
        Line::from(""),
    ]
}

pub fn live_terminal_status_lines(
    detail: &DemoDetail,
    terminal_view: &TerminalView,
    input_mode: bool,
    session: &BrowserLiveTerminalSession,
) -> Vec<Line<'static>> {
    let input_label = if input_mode {
        "capturing keys"
    } else {
        "available"
    };
    let state = if session.is_running() {
        "running"
    } else {
        "complete"
    };
    vec![
        Line::from(vec![
            Span::styled(
                " source: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(terminal_view.source.label()),
            Span::raw("   "),
            Span::styled(
                " state: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(state.to_owned()),
            Span::raw("   "),
            Span::styled(
                " transport: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(if detail.mode == "interactive" || detail.mode == "hybrid" {
                "attached"
            } else {
                "stream"
            }),
            Span::raw("   "),
            Span::styled(
                " input: ",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(input_label.to_owned()),
        ]),
        Line::from(""),
    ]
}

pub fn browser_terminal_viewport_size(total_cols: u16, total_rows: u16) -> (u16, u16) {
    let area = Rect::new(0, 0, total_cols, total_rows);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(4),
        ])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(browser_body_constraints())
        .split(layout[1]);
    let inner = Block::default()
        .borders(Borders::ALL)
        .border_set(ratatui::symbols::border::ROUNDED)
        .inner(body[1]);
    let tabs = detail_tab_lines(DetailTab::Terminal, true, inner.width as usize);
    let terminal_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(tabs.len() as u16),
            Constraint::Length(2),
            Constraint::Min(1),
        ])
        .split(inner);
    (
        terminal_layout[2].width.max(1),
        terminal_layout[2].height.max(1),
    )
}

pub fn read_recent_log_lines(path: &Path, limit: usize) -> Result<Vec<String>, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let mut lines = content.lines().map(str::to_owned).collect::<Vec<_>>();
    if lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }
    Ok(lines)
}

pub struct BrowserLiveTerminalSession {
    pub demo_id: String,
    pub terminal: LiveTerminalBuffer,
    child: Child,
    stdin: Option<ChildStdin>,
    receiver: Receiver<LiveTerminalEvent>,
    transcript: Vec<u8>,
    exit_status: Option<ExitStatus>,
}

enum LiveTerminalEvent {
    Output(Vec<u8>),
}

impl BrowserLiveTerminalSession {
    pub fn spawn(
        executable: PathBuf,
        repo_root: PathBuf,
        demo_id: String,
        subcommand: DemoSubcommand,
        viewport_size: Option<(u16, u16)>,
    ) -> Result<Self, String> {
        let mut command = ProcessCommand::new(executable);
        command.current_dir(&repo_root);
        apply_live_terminal_command_env(&mut command, viewport_size);
        match subcommand {
            DemoSubcommand::Run { demo_id } => {
                command.arg("demo").arg("run").arg(demo_id);
            }
            DemoSubcommand::Rerun { demo_id } => {
                command.arg("demo").arg("rerun").arg(demo_id);
            }
            _ => {
                return Err("live browser terminal sessions only support demo run/rerun".to_owned());
            }
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            format!("failed to launch live browser terminal session for `{demo_id}`: {error}")
        })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or_else(|| {
            format!("live browser terminal session for `{demo_id}` launched without stdout pipe")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            format!("live browser terminal session for `{demo_id}` launched without stderr pipe")
        })?;
        let (sender, receiver) = mpsc::channel();
        spawn_live_terminal_reader(stdout, sender.clone());
        spawn_live_terminal_reader(stderr, sender);
        Ok(Self {
            demo_id,
            terminal: LiveTerminalBuffer::new(),
            child,
            stdin,
            receiver,
            transcript: Vec::new(),
            exit_status: None,
        })
    }

    pub fn is_running(&self) -> bool {
        self.exit_status.is_none()
    }

    pub fn drain_output(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                LiveTerminalEvent::Output(bytes) => self.append_output(&bytes),
            }
        }
    }

    fn append_output(&mut self, bytes: &[u8]) {
        let sanitized = sanitize_live_terminal_bytes(bytes);
        self.terminal.push_chunk(&sanitized);
        self.transcript.extend_from_slice(&sanitized);
        if self.transcript.len() > DEMO_BROWSER_LIVE_TERMINAL_TRANSCRIPT_MAX_BYTES {
            let excess = self.transcript.len() - DEMO_BROWSER_LIVE_TERMINAL_TRANSCRIPT_MAX_BYTES;
            self.transcript.drain(..excess);
        }
    }

    pub fn poll_exit(&mut self) -> Result<Option<(String, bool)>, String> {
        if self.exit_status.is_some() {
            return Ok(None);
        }
        let Some(status) = self.child.try_wait().map_err(|error| {
            format!(
                "failed to poll live browser terminal session for `{}`: {error}",
                self.demo_id
            )
        })?
        else {
            return Ok(None);
        };
        self.exit_status = Some(status);
        self.stdin = None;
        self.drain_output();
        self.terminal.finalize();
        Ok(Some((self.demo_id.clone(), status.success())))
    }

    pub fn write_input(&mut self, bytes: &[u8]) -> Result<(), String> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(format!(
                "live terminal session for `{}` no longer accepts input",
                self.demo_id
            ));
        };
        stdin.write_all(bytes).map_err(|error| {
            format!(
                "failed to write live terminal input for `{}`: {error}",
                self.demo_id
            )
        })?;
        stdin.flush().map_err(|error| {
            format!(
                "failed to flush live terminal input for `{}`: {error}",
                self.demo_id
            )
        })
    }

    pub fn finish_after_stop_request(&mut self) -> Result<(), String> {
        for _ in 0..20 {
            self.drain_output();
            if self.poll_exit()?.is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stdin = None;
        self.exit_status = None;
        self.terminal.finalize();
        Ok(())
    }
}

fn spawn_live_terminal_reader<R>(mut reader: R, sender: mpsc::Sender<LiveTerminalEvent>)
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        while let Ok(read) = reader.read(&mut buffer) {
            if read == 0 {
                break;
            }
            if sender
                .send(LiveTerminalEvent::Output(buffer[..read].to_vec()))
                .is_err()
            {
                break;
            }
        }
    });
}

fn apply_live_terminal_command_env(
    command: &mut ProcessCommand,
    viewport_size: Option<(u16, u16)>,
) {
    command.env_remove("NO_COLOR").env("EFFIGY_COLOR", "always");
    if let Some((cols, rows)) = viewport_size {
        command
            .env(DEMO_BROWSER_TERMINAL_COLS_ENV, cols.to_string())
            .env(DEMO_BROWSER_TERMINAL_ROWS_ENV, rows.to_string());
    }
}

pub fn sanitize_live_terminal_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut sanitized = Vec::with_capacity(bytes.len());
    let mut cursor = 0usize;
    while let Some(relative) = bytes[cursor..]
        .windows(2)
        .position(|window| window == b"^D")
    {
        let absolute = cursor + relative;
        sanitized.extend_from_slice(&bytes[cursor..absolute]);
        cursor = absolute + 2;
        while cursor < bytes.len() && bytes[cursor] == 0x08 {
            cursor += 1;
        }
    }
    sanitized.extend_from_slice(&bytes[cursor..]);
    normalize_terminal_newlines(&sanitized)
}

fn normalize_terminal_newlines(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' && normalized.last().copied() != Some(b'\r') {
            normalized.push(b'\r');
            normalized.push(b'\n');
        } else {
            normalized.push(byte);
        }
        index += 1;
    }
    normalized
}

pub fn take_complete_terminal_bytes(carry: &mut Vec<u8>, bytes: &[u8]) -> Vec<u8> {
    crate::terminal_text::take_complete_terminal_bytes(carry, bytes)
}

pub fn browser_live_terminal_env(viewport_size: Option<(u16, u16)>) -> Vec<(String, String)> {
    let mut env = vec![("EFFIGY_COLOR".to_owned(), "always".to_owned())];
    if let Some((cols, rows)) = viewport_size {
        env.push((DEMO_BROWSER_TERMINAL_COLS_ENV.to_owned(), cols.to_string()));
        env.push((DEMO_BROWSER_TERMINAL_ROWS_ENV.to_owned(), rows.to_string()));
    }
    env
}
