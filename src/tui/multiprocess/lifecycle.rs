use std::collections::HashMap;
use std::io;
use std::io::IsTerminal;
use std::time::Instant;

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::Terminal;

use crate::process_manager::{ProcessSupervisor, ShutdownProgress};
use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{KeyValue, OutputMode, PlainRenderer, Renderer};

use super::config::SHUTDOWN_GRACE_TIMEOUT;
use super::diagnostics::RuntimeDiagnostics;
use super::terminal_text::{format_elapsed, is_expected_shutdown_diagnostic, styled_text};
use super::MultiProcessTuiError;

pub(super) type TuiTerminal = Terminal<CrosstermBackend<std::io::Stdout>>;

pub(super) fn init_terminal() -> Result<TuiTerminal, io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

pub(super) fn shutdown_and_render_summary(
    terminal: &mut TuiTerminal,
    supervisor: &ProcessSupervisor,
    observed_non_zero: HashMap<String, String>,
    process_started_at: &HashMap<String, Instant>,
    diagnostics: &RuntimeDiagnostics,
) -> Result<Vec<(String, String)>, MultiProcessTuiError> {
    supervisor.terminate_all_graceful_with_progress(SHUTDOWN_GRACE_TIMEOUT, |progress| {
        let label = match progress {
            ShutdownProgress::SendingTerm => "Shutdown: sending SIGTERM to managed processes...",
            ShutdownProgress::Waiting => "Shutdown: waiting for managed processes to exit...",
            ShutdownProgress::ForceKilling => {
                "Shutdown: forcing remaining managed processes to stop..."
            }
            ShutdownProgress::Complete { .. } => "Shutdown: complete.",
        };
        let _ = draw_shutdown_status(terminal, label);
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
    let process_diagnostics = supervisor.exit_diagnostics();
    let now = Instant::now();
    for (name, diagnostic) in process_diagnostics {
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

    if diagnostics.enabled() {
        renderer.section("TUI Diagnostics")?;
        renderer.key_values(&[
            KeyValue::new("elapsed-ms", diagnostics.elapsed_ms().to_string()),
            KeyValue::new("frames", diagnostics.frame_count().to_string()),
            KeyValue::new("keypresses", diagnostics.keypress_count().to_string()),
            KeyValue::new("stdout-chunks", diagnostics.stdout_chunks().to_string()),
            KeyValue::new("stderr-chunks", diagnostics.stderr_chunks().to_string()),
            KeyValue::new("stdout-lines", diagnostics.stdout_lines().to_string()),
            KeyValue::new("stderr-lines", diagnostics.stderr_lines().to_string()),
            KeyValue::new("exit-events", diagnostics.exit_events().to_string()),
            KeyValue::new("vt-resets", diagnostics.vt_resets().to_string()),
        ])?;
        renderer.text("")?;
        let traces = diagnostics.traces();
        if !traces.is_empty() {
            renderer.bullet_list("trace", &traces)?;
            renderer.text("")?;
        }
    }

    Ok(non_zero_exits)
}

fn draw_shutdown_status(terminal: &mut TuiTerminal, status: &str) -> Result<(), io::Error> {
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
