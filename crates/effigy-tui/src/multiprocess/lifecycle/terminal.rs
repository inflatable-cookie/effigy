use std::io;

use crossterm::cursor::Show;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub(in super::super) type TuiTerminal = Terminal<CrosstermBackend<std::io::Stdout>>;

pub(in super::super) fn init_terminal() -> Result<TuiTerminal, io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout, LeaveAlternateScreen, EnableLineWrap, Show);
        return Err(error);
    }
    let backend = CrosstermBackend::new(stdout);
    match Terminal::new(backend) {
        Ok(mut terminal) => match terminal.clear() {
            Ok(()) => Ok(terminal),
            Err(error) => {
                let _ = restore_terminal(&mut terminal);
                Err(error)
            }
        },
        Err(error) => {
            let _ = disable_raw_mode();
            let mut stdout = io::stdout();
            let _ = execute!(stdout, LeaveAlternateScreen, EnableLineWrap, Show);
            Err(error)
        }
    }
}

pub(super) fn restore_terminal(terminal: &mut TuiTerminal) -> Result<(), io::Error> {
    let raw_result = disable_raw_mode();
    let screen_result = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        EnableLineWrap,
        Show
    );
    raw_result.and(screen_result)
}
