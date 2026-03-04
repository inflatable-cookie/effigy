use super::normalize::normalize_terminal_payload;
use super::parse::{parse_cursor_up_count, parse_escape_sequence, EscapeSequence};

#[test]
fn parse_escape_sequence_supports_csi_and_osc_forms() {
    let csi_chars = "\u{1b}[31m".chars().collect::<Vec<char>>();
    let osc_bel_chars = "\u{1b}]0;title\u{7}".chars().collect::<Vec<char>>();
    let osc_st_chars = "\u{1b}]0;title\u{1b}\\".chars().collect::<Vec<char>>();

    assert!(matches!(
        parse_escape_sequence(&csi_chars, 0),
        Some(EscapeSequence::Csi {
            final_byte: 'm',
            ..
        })
    ));
    assert!(matches!(
        parse_escape_sequence(&osc_bel_chars, 0),
        Some(EscapeSequence::Osc { .. })
    ));
    assert!(matches!(
        parse_escape_sequence(&osc_st_chars, 0),
        Some(EscapeSequence::Osc { .. })
    ));
}

#[test]
fn parse_cursor_up_count_handles_empty_invalid_and_explicit_params() {
    assert_eq!(parse_cursor_up_count(""), 1);
    assert_eq!(parse_cursor_up_count("3"), 3);
    assert_eq!(parse_cursor_up_count("2;5"), 2);
    assert_eq!(parse_cursor_up_count("x"), 1);
}

#[test]
fn normalize_terminal_payload_tracks_cursor_up_and_strips_non_sgr_sequences() {
    let payload = "line1\u{1b}[2A\u{1b}[2K\r\u{1b}[31mline2\u{1b}[0m";
    let normalized = normalize_terminal_payload(payload);

    assert_eq!(normalized.cursor_up, 2);
    assert_eq!(normalized.text, "line1\r\u{1b}[31mline2\u{1b}[0m");
}

#[test]
fn normalize_terminal_payload_ignores_osc_sequences() {
    let payload = "pre\u{1b}]0;title\u{7}post";
    let normalized = normalize_terminal_payload(payload);
    assert_eq!(normalized.text, "prepost");
}
