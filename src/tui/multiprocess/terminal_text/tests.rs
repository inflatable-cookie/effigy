use std::collections::VecDeque;
use std::time::Duration;

use ratatui::style::{Color, Style};
use vt100::Parser as VtParser;

use crate::tui::core::LogEntryKind;

use super::{
    ansi_line, format_elapsed, ingest_log_payload, is_expected_shutdown_diagnostic,
    runtime_meta_line, sanitize_log_text, vt_logs,
};

#[test]
fn ansi_line_parses_basic_colour_sequence() {
    let line = ansi_line("\u{1b}[31merror\u{1b}[0m ok", Style::default());
    assert_eq!(line.spans.len(), 2);
    assert_eq!(line.spans[0].content.as_ref(), "error");
    assert_eq!(line.spans[1].content.as_ref(), " ok");
}

#[test]
fn ansi_line_parses_ansi256_colour_sequence() {
    let line = ansi_line("\u{1b}[38;5;212mEFFIGY\u{1b}[0m path", Style::default());
    assert_eq!(line.spans.len(), 2);
    assert_eq!(line.spans[0].content.as_ref(), "EFFIGY");
    assert_eq!(line.spans[0].style.fg, Some(Color::Indexed(212)));
    assert_eq!(line.spans[1].content.as_ref(), " path");
}

#[test]
fn ansi_line_parses_truecolor_sequence() {
    let line = ansi_line("\u{1b}[38;2;12;34;56mshade\u{1b}[0m", Style::default());
    assert_eq!(line.spans.len(), 1);
    assert_eq!(line.spans[0].content.as_ref(), "shade");
    assert_eq!(line.spans[0].style.fg, Some(Color::Rgb(12, 34, 56)));
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
