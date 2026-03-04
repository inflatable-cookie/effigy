use super::parse::{parse_cursor_up_count, parse_escape_sequence, EscapeSequence};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(in super::super) struct NormalizedPayload {
    pub(in super::super) text: String,
    pub(in super::super) cursor_up: usize,
}

pub(in super::super) fn normalize_terminal_payload(raw: &str) -> NormalizedPayload {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    let mut cursor_up = 0usize;
    while i < chars.len() {
        if let Some(sequence) = parse_escape_sequence(&chars, i) {
            match sequence {
                EscapeSequence::Csi {
                    start,
                    end,
                    params,
                    final_byte,
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
