use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};

#[cfg(test)]
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Command,
    Insert,
}

#[derive(Debug, Clone)]
pub enum LogEntryKind {
    Stdout,
    Stderr,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessExitState {
    /// The entry reported a zero exit status.
    Success,
    /// The entry reported a non-zero exit status.
    Failure,
    /// The entry was stopped by a signal and never reported an exit status.
    Stopped,
}

/// The TUI's ratatui colours are a projection of [`effigy_ui::theme::Theme`],
/// the one palette every Effigy surface draws with: a palette change moves the
/// CLI and the TUI together instead of leaving two hand-kept ANSI tables.
struct Palette {
    accent: Color,
    accent_soft: Color,
    muted: Color,
}

fn palette() -> &'static Palette {
    static PALETTE: std::sync::OnceLock<Palette> = std::sync::OnceLock::new();
    PALETTE.get_or_init(|| {
        let theme = effigy_ui::theme::Theme::default();
        Palette {
            accent: ratatui_colour(theme.accent),
            accent_soft: ratatui_colour(theme.accent_soft),
            muted: ratatui_colour(theme.muted),
        }
    })
}

/// Project an `anstyle` foreground colour into the ratatui palette.
fn ratatui_colour(style: anstyle::Style) -> Color {
    match style.get_fg_color() {
        Some(anstyle::Color::Ansi256(index)) => Color::Indexed(index.0),
        Some(anstyle::Color::Ansi(colour)) => match colour {
            anstyle::AnsiColor::Black => Color::Black,
            anstyle::AnsiColor::Red => Color::Red,
            anstyle::AnsiColor::Green => Color::Green,
            anstyle::AnsiColor::Yellow => Color::Yellow,
            anstyle::AnsiColor::Blue => Color::Blue,
            anstyle::AnsiColor::Magenta => Color::Magenta,
            anstyle::AnsiColor::Cyan => Color::Cyan,
            anstyle::AnsiColor::White => Color::White,
            anstyle::AnsiColor::BrightBlack => Color::DarkGray,
            anstyle::AnsiColor::BrightRed => Color::LightRed,
            anstyle::AnsiColor::BrightGreen => Color::LightGreen,
            anstyle::AnsiColor::BrightYellow => Color::LightYellow,
            anstyle::AnsiColor::BrightBlue => Color::LightBlue,
            anstyle::AnsiColor::BrightMagenta => Color::LightMagenta,
            anstyle::AnsiColor::BrightCyan => Color::LightCyan,
            anstyle::AnsiColor::BrightWhite => Color::White,
        },
        Some(anstyle::Color::Rgb(rgb)) => Color::Rgb(rgb.0, rgb.1, rgb.2),
        None => Color::Reset,
        // `anstyle::Color` is exhaustive here; a future variant lands as the
        // terminal's default rather than a silently wrong hue.
        #[allow(unreachable_patterns)]
        Some(_) => Color::Reset,
    }
}

/// The accent the TUI's borders and active tab draw with.
pub fn accent() -> Color {
    palette().accent
}

/// The softer accent used for secondary marks such as the version badge.
pub fn accent_soft() -> Color {
    palette().accent_soft
}

/// The muted colour inactive tabs draw with.
pub fn muted() -> Color {
    palette().muted
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Writer-parsed, redacted styles. Plain transcript text remains in `line`.
    pub styles: Option<Vec<effigy_core::terminal_text::StyledSpan>>,
    pub kind: LogEntryKind,
    pub line: String,
}

pub fn next_index(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (current + 1) % len
    }
}

pub fn prev_index(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else if current == 0 {
        len - 1
    } else {
        current - 1
    }
}

pub fn effigy_panel_block<'a>(
    title: Option<&'a str>,
    show_version: bool,
    border_color: Color,
) -> Block<'a> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(border_color));
    if let Some(title) = title {
        block = block.title_top(
            Line::from(Span::styled(
                title.to_owned(),
                Style::default().fg(accent()).add_modifier(Modifier::BOLD),
            ))
            .left_aligned(),
        );
    }
    if show_version {
        let version = format!(" {} ", effigy_core::build_info::display_version());
        block = block.title_bottom(
            Line::from(Span::styled(version, Style::default().fg(accent_soft()))).right_aligned(),
        );
    }
    block
}

#[cfg(test)]
pub fn toggle_follow_for_active(
    follow_mode: &mut HashMap<String, bool>,
    scroll_offsets: &mut HashMap<String, usize>,
    active: &str,
    max_offset: usize,
) {
    if let Some(follow) = follow_mode.get_mut(active) {
        *follow = !*follow;
        if *follow {
            if let Some(offset) = scroll_offsets.get_mut(active) {
                *offset = max_offset;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_projects_the_shared_theme() {
        let theme = effigy_ui::theme::Theme::default();
        assert_eq!(ratatui_colour(theme.accent), accent());
        assert_eq!(ratatui_colour(theme.accent_soft), accent_soft());
        assert_eq!(ratatui_colour(theme.muted), muted());
        assert_eq!(accent(), Color::Indexed(212));
        assert_eq!(accent_soft(), Color::Indexed(218));
        assert_eq!(muted(), Color::Indexed(244));
    }

    #[test]
    fn ansi_and_rgb_colours_map_to_their_ratatui_counterparts() {
        let ansi = |colour| anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(colour)));
        assert_eq!(
            ratatui_colour(ansi(anstyle::AnsiColor::White)),
            Color::White
        );
        assert_eq!(
            ratatui_colour(ansi(anstyle::AnsiColor::BrightBlack)),
            Color::DarkGray
        );
        assert_eq!(
            ratatui_colour(
                anstyle::Style::new()
                    .fg_color(Some(anstyle::Color::Rgb(anstyle::RgbColor(1, 2, 3))))
            ),
            Color::Rgb(1, 2, 3)
        );
        assert_eq!(ratatui_colour(anstyle::Style::new()), Color::Reset);
    }
}
