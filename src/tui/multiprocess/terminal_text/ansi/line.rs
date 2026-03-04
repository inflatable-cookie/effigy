use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::parse::{parse_escape_sequence, EscapeSequence};
use super::style::apply_sgr;

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
