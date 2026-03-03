use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

#[derive(Debug, Default)]
pub(super) struct NormalizedPayload {
    pub(super) text: String,
    pub(super) cursor_up: usize,
}

pub(super) fn normalize_terminal_payload(raw: &str) -> NormalizedPayload {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    let mut cursor_up = 0usize;
    while i < chars.len() {
        if let Some(sequence) = parse_escape_sequence(&chars, i) {
            match sequence {
                EscapeSequence::Csi {
                    params,
                    final_byte,
                    end,
                    start,
                } => {
                    if final_byte == 'm' {
                        out.extend(chars[start..=end].iter());
                    } else if final_byte == 'A' {
                        cursor_up = cursor_up.saturating_add(parse_cursor_up_count(&params));
                    }
                    i = end + 1;
                    continue;
                }
                EscapeSequence::Osc { end } => {
                    i = end + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    NormalizedPayload {
        text: out,
        cursor_up,
    }
}

pub(crate) fn ansi_line(raw: &str, base: Style) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut style = base;
    let mut buf = String::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if let Some(sequence) = parse_escape_sequence(&chars, i) {
            match sequence {
                EscapeSequence::Csi {
                    params,
                    final_byte,
                    end,
                    ..
                } => {
                    if !buf.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut buf), style));
                    }
                    if final_byte == 'm' {
                        style = apply_sgr(style, &params, base);
                    }
                    i = end + 1;
                    continue;
                }
                EscapeSequence::Osc { end } => {
                    i = end + 1;
                    continue;
                }
            }
        }
        buf.push(chars[i]);
        i += 1;
    }
    if !buf.is_empty() {
        spans.push(Span::styled(buf, style));
    }
    if spans.is_empty() {
        return Line::from("");
    }
    Line::from(spans)
}

enum EscapeSequence {
    Csi {
        start: usize,
        end: usize,
        params: String,
        final_byte: char,
    },
    Osc {
        end: usize,
    },
}

fn parse_escape_sequence(chars: &[char], start: usize) -> Option<EscapeSequence> {
    if chars.get(start).copied()? != '\u{1b}' {
        return None;
    }
    let next = chars.get(start + 1).copied()?;
    match next {
        '[' => parse_csi_sequence(chars, start),
        ']' => parse_osc_sequence(chars, start),
        _ => None,
    }
}

fn parse_csi_sequence(chars: &[char], start: usize) -> Option<EscapeSequence> {
    let mut i = start + 2;
    let mut params = String::new();
    while i < chars.len() {
        let final_byte = chars[i];
        if ('@'..='~').contains(&final_byte) {
            return Some(EscapeSequence::Csi {
                start,
                end: i,
                params,
                final_byte,
            });
        }
        params.push(final_byte);
        i += 1;
    }
    None
}

fn parse_osc_sequence(chars: &[char], start: usize) -> Option<EscapeSequence> {
    let mut i = start + 2;
    while i < chars.len() {
        if chars[i] == '\u{0007}' {
            return Some(EscapeSequence::Osc { end: i });
        }
        if chars[i] == '\u{1b}' && i + 1 < chars.len() && chars[i + 1] == '\\' {
            return Some(EscapeSequence::Osc { end: i + 1 });
        }
        i += 1;
    }
    None
}

fn parse_cursor_up_count(params: &str) -> usize {
    params
        .split(';')
        .next()
        .and_then(|value| {
            if value.is_empty() {
                Some(1usize)
            } else {
                value.parse::<usize>().ok()
            }
        })
        .unwrap_or(1usize)
}

fn apply_sgr(current: Style, sgr: &str, base: Style) -> Style {
    let mut style = current;
    let parts = if sgr.is_empty() {
        vec!["0"]
    } else {
        sgr.split(';').collect::<Vec<&str>>()
    };
    for part in parts {
        match part.parse::<u8>() {
            Ok(0) => style = base,
            Ok(1) => style = style.add_modifier(Modifier::BOLD),
            Ok(2) => style = style.add_modifier(Modifier::DIM),
            Ok(3) => style = style.add_modifier(Modifier::ITALIC),
            Ok(4) => style = style.add_modifier(Modifier::UNDERLINED),
            Ok(22) => style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            Ok(23) => style = style.remove_modifier(Modifier::ITALIC),
            Ok(24) => style = style.remove_modifier(Modifier::UNDERLINED),
            Ok(30) => style = style.fg(Color::Black),
            Ok(31) => style = style.fg(Color::Red),
            Ok(32) => style = style.fg(Color::Green),
            Ok(33) => style = style.fg(Color::Yellow),
            Ok(34) => style = style.fg(Color::Blue),
            Ok(35) => style = style.fg(Color::Magenta),
            Ok(36) => style = style.fg(Color::Cyan),
            Ok(37) => style = style.fg(Color::Gray),
            Ok(39) => style = style.fg(base.fg.unwrap_or(Color::Reset)),
            Ok(90) => style = style.fg(Color::DarkGray),
            Ok(91) => style = style.fg(Color::LightRed),
            Ok(92) => style = style.fg(Color::LightGreen),
            Ok(93) => style = style.fg(Color::LightYellow),
            Ok(94) => style = style.fg(Color::LightBlue),
            Ok(95) => style = style.fg(Color::LightMagenta),
            Ok(96) => style = style.fg(Color::LightCyan),
            Ok(97) => style = style.fg(Color::White),
            _ => {}
        }
    }
    style
}
