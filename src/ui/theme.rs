use anstyle::{Ansi256Color, AnsiColor, Color, Style};

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
    pub accent_soft: Style,
    pub muted: Style,
    pub inline_code: Style,
    pub success: Style,
    pub warning: Style,
    pub error: Style,
    pub label: Style,
    pub value: Style,
}

impl Default for Theme {
    fn default() -> Self {
        // Gum-inspired palette: restrained neutral base with a vivid accent.
        Self {
            accent: Style::new()
                .fg_color(Some(Color::Ansi256(Ansi256Color(212))))
                .bold(),
            accent_soft: Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(218)))),
            muted: Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(244)))),
            inline_code: Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(117)))),
            success: Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(42)))),
            warning: Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(214)))),
            error: Style::new()
                .fg_color(Some(Color::Ansi256(Ansi256Color(203))))
                .bold(),
            label: Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(111)))),
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
