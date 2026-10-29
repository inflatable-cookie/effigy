use std::io;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;

use effigy_process::ShutdownProgress;

use super::super::SessionProcesses;

use super::super::config::SHUTDOWN_GRACE_TIMEOUT;
use super::terminal::TuiTerminal;

pub(super) fn run_shutdown_with_progress(
    terminal: &mut TuiTerminal,
    supervisor: &dyn SessionProcesses,
) {
    supervisor.terminate_all_graceful_with_progress(SHUTDOWN_GRACE_TIMEOUT, &mut |progress| {
        let _ = draw_shutdown_status(terminal, shutdown_progress_label(&progress));
    });
}

fn shutdown_progress_label(progress: &ShutdownProgress) -> &'static str {
    match progress {
        ShutdownProgress::SendingTerm => "Shutdown: sending SIGTERM to managed processes...",
        ShutdownProgress::Waiting => "Shutdown: waiting for managed processes to exit...",
        ShutdownProgress::ForceKilling => {
            "Shutdown: forcing remaining managed processes to stop..."
        }
        ShutdownProgress::Complete { .. } => "Shutdown: complete.",
    }
}

fn draw_shutdown_status(terminal: &mut TuiTerminal, status: &str) -> Result<(), io::Error> {
    terminal.draw(|frame| render_shutdown_status(frame, status))?;
    Ok(())
}

fn render_shutdown_status(frame: &mut ratatui::Frame<'_>, status: &str) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    let footer = Paragraph::new(status.to_owned()).style(Style::default().fg(Color::Yellow));
    frame.render_widget(footer, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn shutdown_frames_have_one_current_label() {
        let mut terminal = Terminal::new(TestBackend::new(100, 4)).unwrap();
        let stages = [
            ShutdownProgress::SendingTerm,
            ShutdownProgress::Waiting,
            ShutdownProgress::ForceKilling,
            ShutdownProgress::Complete {
                total: 1,
                forced: 0,
            },
        ];
        for stage in stages {
            let label = shutdown_progress_label(&stage);
            terminal
                .draw(|frame| render_shutdown_status(frame, label))
                .unwrap();
            let frame = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert_eq!(frame.matches("Shutdown:").count(), 1, "{frame}");
            assert!(frame.contains(label), "{frame}");
            assert!(!frame.contains("running dev"), "{frame}");
        }
    }
}
