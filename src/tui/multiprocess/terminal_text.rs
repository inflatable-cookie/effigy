use std::collections::VecDeque;
use std::time::Duration;

use anstyle::Style as AnsiStyle;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use vt100::Parser as VtParser;

use super::config::MAX_LOG_LINES;
use crate::tui::core::{LogEntry, LogEntryKind};

pub(crate) fn vt_logs(
    parser: &mut VtParser,
    panel_rows: usize,
    panel_cols: usize,
    ui_scroll_offset: usize,
    follow: bool,
) -> (Vec<LogEntry>, usize, usize) {
    let safe_rows = panel_rows.max(1);
    parser.set_size(safe_rows as u16, panel_cols.max(1) as u16);
    // vt100 0.15.x can panic when scrollback offset exceeds visible row count.
    // Clamp to a safe range until we move to a parser version without this bug.
    let max_offset = vt_max_scrollback(parser).min(safe_rows.saturating_sub(1));
    let clamped = if follow {
        max_offset
    } else {
        ui_scroll_offset.min(max_offset)
    };
    let vt_scrollback = max_offset.saturating_sub(clamped);
    parser.set_scrollback(vt_scrollback);
    let rows = parser
        .screen()
        .rows_formatted(0, panel_cols.max(1) as u16)
        .map(|row| LogEntry {
            kind: LogEntryKind::Stdout,
            line: String::from_utf8_lossy(&row).into_owned(),
        })
        .collect::<Vec<LogEntry>>();
    (rows, clamped, max_offset)
}

fn vt_max_scrollback(parser: &mut VtParser) -> usize {
    let current = parser.screen().scrollback();
    parser.set_scrollback(usize::MAX);
    let max = parser.screen().scrollback();
    parser.set_scrollback(current);
    max
}

pub(crate) fn push_entry(buffer: &mut VecDeque<LogEntry>, entry: LogEntry) {
    buffer.push_back(entry);
    while buffer.len() > MAX_LOG_LINES {
        buffer.pop_front();
    }
}

pub(crate) fn ingest_log_payload(
    buffer: &mut VecDeque<LogEntry>,
    kind: LogEntryKind,
    payload: &str,
) {
    let (normalized, cursor_up) = normalize_terminal_payload(payload);
    let fragments = normalized
        .split('\r')
        .map(sanitize_log_text)
        .filter(|line| !line.is_empty())
        .collect::<Vec<String>>();
    if fragments.is_empty() {
        return;
    }

    if fragments.len() == 1 && !normalized.contains('\r') {
        if cursor_up > 0 {
            replace_last_renderable_line(buffer, kind, fragments[0].clone());
        } else {
            push_entry(
                buffer,
                LogEntry {
                    kind,
                    line: fragments[0].clone(),
                },
            );
        }
        return;
    }

    let mut append_on_first_rewrite = false;
    let mut first = true;
    for fragment in fragments {
        if first {
            if cursor_up > 0 {
                replace_last_renderable_line(buffer, kind.clone(), fragment);
            } else {
                push_entry(
                    buffer,
                    LogEntry {
                        kind: kind.clone(),
                        line: fragment,
                    },
                );
            }
            first = false;
            continue;
        }
        if append_on_first_rewrite {
            push_entry(
                buffer,
                LogEntry {
                    kind: kind.clone(),
                    line: fragment,
                },
            );
            append_on_first_rewrite = false;
        } else {
            replace_last_renderable_line(buffer, kind.clone(), fragment);
        }
    }
}

fn normalize_terminal_payload(raw: &str) -> (String, usize) {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    let mut cursor_up = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\u{1b}' && i + 1 < chars.len() {
            match chars[i + 1] {
                '[' => {
                    let start = i;
                    i += 2;
                    let mut params = String::new();
                    while i < chars.len() {
                        let final_byte = chars[i];
                        if ('@'..='~').contains(&final_byte) {
                            if final_byte == 'm' {
                                out.extend(chars[start..=i].iter());
                            } else if final_byte == 'A' {
                                let count = params
                                    .split(';')
                                    .next()
                                    .and_then(|value| {
                                        if value.is_empty() {
                                            Some(1usize)
                                        } else {
                                            value.parse::<usize>().ok()
                                        }
                                    })
                                    .unwrap_or(1usize);
                                cursor_up = cursor_up.saturating_add(count);
                            }
                            break;
                        }
                        params.push(final_byte);
                        i += 1;
                    }
                }
                ']' => {
                    i += 2;
                    while i < chars.len() {
                        if chars[i] == '\u{0007}' {
                            break;
                        }
                        if chars[i] == '\u{1b}' && i + 1 < chars.len() && chars[i + 1] == '\\' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {}
            }
        } else {
            out.push(ch);
        }
        i += 1;
    }
    (out, cursor_up)
}

fn replace_last_renderable_line(buffer: &mut VecDeque<LogEntry>, kind: LogEntryKind, line: String) {
    if let Some(last) = buffer.back_mut() {
        if matches!(last.kind, LogEntryKind::Stdout | LogEntryKind::Stderr) {
            last.kind = kind;
            last.line = line;
            return;
        }
    }
    push_entry(buffer, LogEntry { kind, line });
}

pub(crate) fn sanitize_log_text(raw: &str) -> String {
    raw.chars()
        .filter(|ch| {
            !matches!(
                ch,
                '\r'
                    | '\u{0000}'..='\u{0008}'
                    | '\u{000B}'
                    | '\u{000C}'
                    | '\u{000E}'..='\u{001A}'
                    | '\u{001C}'..='\u{001F}'
                    | '\u{007F}'
            )
        })
        .collect()
}

pub(crate) fn ansi_line(raw: &str, base: Style) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut style = base;
    let mut buf = String::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '\u{1b}' && i + 1 < chars.len() && chars[i + 1] == '[' {
            if !buf.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut buf), style));
            }
            i += 2;
            let mut code = String::new();
            while i < chars.len() {
                let final_byte = chars[i];
                if ('@'..='~').contains(&final_byte) {
                    if final_byte == 'm' {
                        style = apply_sgr(style, &code, base);
                    }
                    break;
                }
                code.push(chars[i]);
                i += 1;
            }
        } else {
            buf.push(chars[i]);
        }
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

pub(crate) fn is_expected_shutdown_diagnostic(diagnostic: &str) -> bool {
    matches!(diagnostic, "signal=15" | "signal=9")
}

pub(crate) fn format_elapsed(elapsed: Duration) -> String {
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

pub(crate) fn runtime_meta_line(elapsed: Duration, restart_count: usize) -> Line<'static> {
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

pub(crate) fn styled_text(style: AnsiStyle, text: &str) -> String {
    format!("{}{}{}", style.render(), text, style.render_reset())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_line_parses_basic_colour_sequence() {
        let line = ansi_line("\u{1b}[31merror\u{1b}[0m ok", Style::default());
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content.as_ref(), "error");
        assert_eq!(line.spans[1].content.as_ref(), " ok");
    }

    #[test]
    fn expected_shutdown_diagnostics_are_ignored() {
        assert!(is_expected_shutdown_diagnostic("signal=15"));
        assert!(is_expected_shutdown_diagnostic("signal=9"));
        assert!(!is_expected_shutdown_diagnostic("exit=1"));
        assert!(!is_expected_shutdown_diagnostic("signal=11"));
    }

    #[test]
    fn format_elapsed_uses_compact_human_time() {
        assert_eq!(format_elapsed(Duration::from_secs(9)), "9s");
        assert_eq!(format_elapsed(Duration::from_secs(65)), "1m05s");
        assert_eq!(format_elapsed(Duration::from_secs(3665)), "1h01m05s");
    }

    #[test]
    fn runtime_meta_line_marks_restart_state() {
        let started = runtime_meta_line(Duration::from_secs(9), 0);
        assert_eq!(started.spans[0].content.as_ref(), "started: ");
        let restarted = runtime_meta_line(Duration::from_secs(9), 1);
        assert_eq!(restarted.spans[0].content.as_ref(), "restarted: ");
    }

    #[test]
    fn sanitize_log_text_removes_control_bytes_but_keeps_ansi() {
        let raw = "a\u{0008}b\r\u{001b}[31merr\u{001b}[0m";
        let sanitized = sanitize_log_text(raw);
        assert_eq!(sanitized, "ab\u{001b}[31merr\u{001b}[0m");
    }

    #[test]
    fn ingest_log_payload_carriage_return_overwrites_last_line() {
        let mut buffer = VecDeque::new();
        ingest_log_payload(
            &mut buffer,
            LogEntryKind::Stdout,
            "building\rfinished\rdone",
        );
        assert_eq!(buffer.len(), 1);
        let line = buffer.back().expect("line");
        assert!(matches!(line.kind, LogEntryKind::Stdout));
        assert_eq!(line.line, "done");
    }

    #[test]
    fn ingest_log_payload_cursor_up_replaces_prior_line() {
        let mut buffer = VecDeque::new();
        ingest_log_payload(&mut buffer, LogEntryKind::Stdout, "line 1");
        ingest_log_payload(&mut buffer, LogEntryKind::Stdout, "line 2");
        ingest_log_payload(
            &mut buffer,
            LogEntryKind::Stdout,
            "\u{1b}[1A\u{1b}[2K\rline 2 updated",
        );
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer[0].line, "line 1");
        assert_eq!(buffer[1].line, "line 2 updated");
    }

    #[test]
    fn ingest_log_payload_cursor_up_without_replacement_does_not_drop_lines() {
        let mut buffer = VecDeque::new();
        ingest_log_payload(&mut buffer, LogEntryKind::Stdout, "line 1");
        ingest_log_payload(&mut buffer, LogEntryKind::Stdout, "line 2");
        ingest_log_payload(&mut buffer, LogEntryKind::Stdout, "\u{1b}[1A\u{1b}[2K");
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer[0].line, "line 1");
        assert_eq!(buffer[1].line, "line 2");
    }

    #[test]
    fn ansi_line_ignores_non_sgr_escape_sequences() {
        let line = ansi_line(
            "\u{1b}[2K\u{1b}[1Ahello \u{1b}[31mred\u{1b}[0m",
            Style::default(),
        );
        let rendered = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert_eq!(rendered, "hello red");
    }

    #[test]
    fn vt_logs_trims_empty_padding_lines() {
        let mut parser = VtParser::new(8, 40, 100);
        parser.process(b"\n\nhello\nworld\n\n");
        let (rows, _, _) = vt_logs(&mut parser, 8, 40, 0, true);
        assert!(rows.iter().any(|line| line.line.contains("hello")));
        assert!(rows.iter().any(|line| line.line.contains("world")));
    }

    #[test]
    fn vt_logs_clamps_overscroll_without_panicking() {
        let mut parser = VtParser::new(8, 40, 200);
        for i in 0..200 {
            parser.process(format!("line-{i}\n").as_bytes());
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            vt_logs(&mut parser, 8, 40, usize::MAX / 2, false)
        }));
        assert!(result.is_ok(), "overscroll should be clamped safely");
    }
}
