#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum EscapeSequence {
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

pub(super) fn parse_escape_sequence(chars: &[char], start: usize) -> Option<EscapeSequence> {
    if chars.get(start).copied()? != '\u{1b}' {
        return None;
    }
    match chars.get(start + 1).copied()? {
        '[' => parse_csi_sequence(chars, start),
        ']' => parse_osc_sequence(chars, start),
        _ => None,
    }
}

pub(super) fn parse_cursor_up_count(params: &str) -> usize {
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
