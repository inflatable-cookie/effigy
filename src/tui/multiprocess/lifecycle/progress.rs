use std::io;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;

use crate::process_manager::{ProcessSupervisor, ShutdownProgress};

use super::super::config::SHUTDOWN_GRACE_TIMEOUT;
use super::terminal::TuiTerminal;

pub(super) fn run_shutdown_with_progress(terminal: &mut TuiTerminal, supervisor: &ProcessSupervisor) {
    supervisor.terminate_all_graceful_with_progress(SHUTDOWN_GRACE_TIMEOUT, |progress| {
        let _ = draw_shutdown_status(terminal, shutdown_progress_label(&progress));
    });
}

fn shutdown_progress_label(progress: &ShutdownProgress) -> &'static str {
    match progress {
        ShutdownProgress::SendingTerm => "Shutdown: sending SIGTERM to managed processes...",
        ShutdownProgress::Waiting => "Shutdown: waiting for managed processes to exit...",
        ShutdownProgress::ForceKilling => "Shutdown: forcing remaining managed processes to stop...",
        ShutdownProgress::Complete { .. } => "Shutdown: complete.",
    }
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
