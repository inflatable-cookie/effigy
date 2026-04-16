use std::time::Duration;

use anstyle::Style as AnsiStyle;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

pub fn format_elapsed(elapsed: Duration) -> String {
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

pub fn runtime_meta_line(elapsed: Duration, restart_count: usize) -> Line<'static> {
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

pub fn styled_text(style: AnsiStyle, text: &str) -> String {
    format!("{}{}{}", style.render(), text, style.render_reset())
}
