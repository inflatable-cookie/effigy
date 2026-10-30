pub fn sanitize_log_text(raw: &str) -> String {
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

pub fn is_expected_shutdown_diagnostic(diagnostic: &str) -> bool {
    matches!(diagnostic, "signal=15" | "signal=9")
}
