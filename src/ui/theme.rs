use anstyle::{AnsiColor, Color, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Auto,
    Always,
    Never,
}

impl OutputMode {
    pub fn from_env() -> Self {
        match std::env::var("EFFIGY_COLOR").ok().as_deref() {
            Some("always") => OutputMode::Always,
            Some("never") => OutputMode::Never,
            _ => OutputMode::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub accent: Style,
    pub muted: Style,
    pub success: Style,
    pub warning: Style,
    pub error: Style,
    pub label: Style,
    pub value: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Cyan)))
                .bold(),
            muted: Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack))),
            success: Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Green)))
                .bold(),
            warning: Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Yellow)))
                .bold(),
            error: Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Red)))
                .bold(),
            label: Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Blue)))
                .bold(),
            value: Style::new().fg_color(Some(Color::Ansi(AnsiColor::White))),
        }
    }
}

pub fn resolve_color_enabled(mode: OutputMode, is_tty: bool) -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    match mode {
        OutputMode::Always => true,
        OutputMode::Never => false,
        OutputMode::Auto => is_tty,
    }
}

pub fn is_ci_environment() -> bool {
    std::env::var_os("CI").is_some()
}
