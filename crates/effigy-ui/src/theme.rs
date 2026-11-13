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
    pub task_name: Style,
    pub task_signature: Style,
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
            task_name: Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::White)))
                .bold(),
            task_signature: Style::new()
                .fg_color(Some(Color::Ansi256(Ansi256Color(117))))
                .dimmed(),
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

/// The one colour-enable decision: `NO_COLOR` always wins, then the explicit
/// mode, then the terminal's answer for `Auto`.
///
/// Callers that already hold the three facts (a renderer constructor, a
/// recorded invocation) use this; [`resolve_color_enabled`] is the ambient
/// wrapper that reads `NO_COLOR` from the environment.
pub fn resolve_color_enabled_with(mode: OutputMode, is_tty: bool, no_color: bool) -> bool {
    if no_color {
        return false;
    }
    match mode {
        OutputMode::Always => true,
        OutputMode::Never => false,
        OutputMode::Auto => is_tty,
    }
}

/// Colour policy for this process's output stream: [`OutputMode::from_env`],
/// whether that stream is a terminal, and the ambient `NO_COLOR`.
pub fn resolve_color_enabled(mode: OutputMode, is_tty: bool) -> bool {
    resolve_color_enabled_with(mode, is_tty, std::env::var_os("NO_COLOR").is_some())
}

pub fn is_ci_environment() -> bool {
    std::env::var_os("CI").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_color_wins_over_every_mode_and_terminal() {
        for mode in [OutputMode::Auto, OutputMode::Always, OutputMode::Never] {
            for terminal in [false, true] {
                assert!(
                    !resolve_color_enabled_with(mode, terminal, true),
                    "NO_COLOR must disable styles: {mode:?}, terminal={terminal}"
                );
            }
        }
    }

    #[test]
    fn mode_and_terminal_decide_when_no_color_is_absent() {
        assert!(!resolve_color_enabled_with(OutputMode::Auto, false, false));
        assert!(resolve_color_enabled_with(OutputMode::Auto, true, false));
        assert!(resolve_color_enabled_with(OutputMode::Always, false, false));
        assert!(!resolve_color_enabled_with(OutputMode::Never, true, false));
    }
}
