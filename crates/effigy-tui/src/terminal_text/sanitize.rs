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
