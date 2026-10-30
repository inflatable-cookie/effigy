//! Stateful plain projection of a terminal byte stream.
//!
//! Unknown prefixes cannot be interpreted safely; suppress them and report a gap.
//! Offsets remain the caller's raw byte offsets.

/// Maximum raw prefix replayed for a cold plain reader (64 MiB).
pub const PREFIX_SCAN_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default)]
enum State {
    #[default]
    Text,
    Escape,
    EscapeIntermediate,
    Csi,
    String,
    StringEscape,
    Unknown,
}

#[derive(Debug, Default)]
pub struct PlainStream {
    state: State,
    elided: u64,
    after_cr: bool,
    utf8: Vec<u8>,
    string_utf8_lead: u8,
    string_utf8_remaining: u8,
}

impl PlainStream {
    pub fn unknown() -> Self {
        Self {
            state: State::Unknown,
            ..Self::default()
        }
    }

    pub fn take_gap(&mut self) -> u64 {
        std::mem::take(&mut self.elided)
    }

    pub fn project(&mut self, bytes: &[u8]) -> Vec<u8> {
        let plain = self.push(bytes);
        let gap = self.take_gap();
        if gap == 0 {
            plain
        } else {
            let mut result =
                format!("[projection unavailable (unknown escape state): {gap} bytes elided]\n")
                    .into_bytes();
            result.extend(plain);
            result
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(input.len());
        for &byte in input {
            self.state = match self.state {
                State::Unknown => {
                    self.elided = self.elided.saturating_add(1);
                    State::Unknown
                }
                State::Text => self.text_byte(byte, &mut output),
                State::Escape => match byte {
                    b'[' => State::Csi,
                    b']' | b'P' | b'X' | b'^' | b'_' => State::String,
                    0x9b => State::Csi,
                    0x90 | 0x98 | 0x9d..=0x9f => State::String,
                    0x20..=0x2f => State::EscapeIntermediate,
                    0x1b => State::Escape,
                    _ => State::Text,
                },
                State::EscapeIntermediate => match byte {
                    0x9b => State::Csi,
                    0x90 | 0x98 | 0x9d..=0x9f => State::String,
                    0x20..=0x2f => State::EscapeIntermediate,
                    0x1b => State::Escape,
                    _ => State::Text,
                },
                State::Csi => match byte {
                    0x40..=0x7e => State::Text,
                    0x1b => State::Escape,
                    0x9b => State::Csi,
                    0x90 | 0x98 | 0x9d..=0x9f => State::String,
                    _ => State::Csi,
                },
                State::String => self.string_byte(byte),
                State::StringEscape => match byte {
                    b'\\' => State::Text,
                    0x1b => State::StringEscape,
                    _ => self.string_byte(byte),
                },
            };
        }
        output
    }

    fn text_byte(&mut self, byte: u8, output: &mut Vec<u8>) -> State {
        if !self.utf8.is_empty() {
            if (0x80..=0xbf).contains(&byte) {
                self.utf8.push(byte);
                match std::str::from_utf8(&self.utf8) {
                    Ok(text) => {
                        let ch = text.chars().next().expect("nonempty UTF-8");
                        let state = if ('\u{80}'..='\u{9f}').contains(&ch) {
                            c1_state(ch as u8)
                        } else {
                            output.extend_from_slice(&self.utf8);
                            self.after_cr = false;
                            State::Text
                        };
                        self.utf8.clear();
                        return state;
                    }
                    Err(error) if error.error_len().is_none() => return State::Text,
                    Err(_) => {
                        self.utf8.clear();
                        output.extend_from_slice("�".as_bytes());
                        return State::Text;
                    }
                }
            }
            self.utf8.clear();
            output.extend_from_slice("�".as_bytes());
        }
        match byte {
            0x1b => State::Escape,
            b'\r' => {
                output.push(b'\n');
                self.after_cr = true;
                State::Text
            }
            b'\n' if self.after_cr => {
                self.after_cr = false;
                State::Text
            }
            b'\n' | b'\t' | 0x20..=0x7e => {
                output.push(byte);
                self.after_cr = false;
                State::Text
            }
            0x80..=0x9f => c1_state(byte),
            0xc2..=0xf4 => {
                self.utf8.push(byte);
                State::Text
            }
            0xa0..=0xff => {
                output.extend_from_slice("�".as_bytes());
                State::Text
            }
            _ => State::Text,
        }
    }

    fn string_byte(&mut self, byte: u8) -> State {
        if self.string_utf8_remaining != 0 && (0x80..=0xbf).contains(&byte) {
            let encoded_st =
                self.string_utf8_lead == 0xc2 && self.string_utf8_remaining == 1 && byte == 0x9c;
            self.string_utf8_remaining -= 1;
            return if encoded_st {
                State::Text
            } else {
                State::String
            };
        }
        self.string_utf8_remaining = 0;
        match byte {
            0x07 | 0x9c => State::Text,
            0x1b => State::StringEscape,
            0xc2..=0xdf => {
                self.string_utf8_lead = byte;
                self.string_utf8_remaining = 1;
                State::String
            }
            0xe0..=0xef => {
                self.string_utf8_lead = byte;
                self.string_utf8_remaining = 2;
                State::String
            }
            0xf0..=0xf4 => {
                self.string_utf8_lead = byte;
                self.string_utf8_remaining = 3;
                State::String
            }
            _ => State::String,
        }
    }
}

/// Project independent captured streams. A full bounded queue might have lost
/// its prefix, so it is never interpreted as if it began at byte zero.
pub fn project_captured_streams(streams: (Vec<u8>, Vec<u8>), queue_limit: usize) -> Vec<u8> {
    let mut result = Vec::new();
    for stream in [streams.0, streams.1] {
        let mut projection = if stream.len() >= queue_limit {
            PlainStream::unknown()
        } else {
            PlainStream::default()
        };
        result.extend(projection.project(&stream));
    }
    result
}

fn c1_state(byte: u8) -> State {
    match byte {
        0x9b => State::Csi,
        0x90 | 0x98 | 0x9d | 0x9e | 0x9f => State::String,
        _ => State::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::PlainStream;

    #[test]
    fn split_controls_and_progress_are_plain() {
        let chunks: &[&[u8]] = &[
            b"build\r",
            b"\n\x1b[3",
            b"1merror\x1b[0m ",
            b"\x1b]8;;url",
            b"\x1b",
            b"\\link\x1b]8;;\x07\rdone\n",
        ];
        let mut stream = PlainStream::default();
        let output = chunks
            .iter()
            .flat_map(|chunk| stream.push(chunk))
            .collect::<Vec<_>>();
        assert_eq!(output, b"build\nerror link\ndone\n");
    }

    #[test]
    fn unknown_state_never_infers_ground_from_newline_or_terminator() {
        let mut stream = PlainStream::unknown();
        let input = b"osc payload\nsecret\x1b\\still unknown";
        assert!(stream.push(input).is_empty());
        assert_eq!(stream.take_gap(), input.len() as u64);
    }

    #[test]
    fn incremental_projection_matches_replayed_cold_state() {
        let bytes = b"start\x1b[31mred\x1b[0m\r\n\x1b]hidden\nprivate\x1b\\end\n";
        let mut full = PlainStream::default();
        let expected = full.project(bytes);
        for split in 0..bytes.len() {
            let mut stream = PlainStream::default();
            let mut output = stream.project(&bytes[..split]);
            output.extend(stream.project(&bytes[split..]));
            assert_eq!(output, expected, "split {split}");
        }
    }

    #[test]
    fn captured_streams_do_not_share_escape_state() {
        let stdout = b"\x1b]title\nsecret\x1b\\visible".to_vec();
        let stderr = b"\x1b\\other".to_vec();
        let output = super::project_captured_streams((stdout, stderr), 100);
        assert_eq!(output, b"visibleother");
        let unknown = super::project_captured_streams((b"private".to_vec(), Vec::new()), 7);
        assert!(String::from_utf8(unknown)
            .unwrap()
            .contains("projection unavailable"));
    }

    #[test]
    fn every_byte_projection_excludes_c0_controls() {
        for split in 0..=256 {
            let input = (0..=255).collect::<Vec<_>>();
            let mut stream = PlainStream::default();
            let mut output = stream.push(&input[..split]);
            output.extend(stream.push(&input[split..]));
            assert!(output
                .iter()
                .all(|b| *b >= 0x20 || *b == b'\n' || *b == b'\t'));
            assert!(!output.contains(&0x7f));
            assert!(std::str::from_utf8(&output)
                .unwrap()
                .chars()
                .all(|ch| !ch.is_control() || ch == '\n' || ch == '\t'));
        }
    }

    #[test]
    fn unknown_no_newline_flood_never_guesses_text() {
        let mut stream = PlainStream::unknown();
        let output = stream.project(&vec![b'x'; 8192]);
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "[projection unavailable (unknown escape state): 8192 bytes elided]\n"
        );
        assert!(stream.push(b"still unknown").is_empty());
    }

    #[test]
    fn split_utf8_is_preserved_and_c1_controls_are_removed() {
        let mut stream = PlainStream::default();
        assert_eq!(stream.push(b"caf\xc3"), b"caf");
        assert_eq!(
            stream.push(b"\xa9 \xc2\x9b31mred\x9b0m"),
            "é red".as_bytes()
        );
    }

    #[test]
    fn utf8_continuation_inside_osc_cannot_end_the_string() {
        let mut stream = PlainStream::default();
        assert_eq!(stream.push(b"before\x1b]title-\xc3"), b"before");
        assert!(stream.push(b"\x9c secret").is_empty());
        assert_eq!(stream.push(b"\x1b\\after"), b"after");

        let mut stream = PlainStream::default();
        assert!(stream
            .push(b"\x1b]hidden\x1b\xc3\x9cstill hidden")
            .is_empty());
        assert_eq!(stream.push(b"\xc2\x9cvisible"), b"visible");
    }

    #[test]
    fn c1_sequences_interrupting_escape_are_not_exposed() {
        let mut stream = PlainStream::default();
        assert_eq!(stream.push(b"\x1b(\x9b31mred"), b"red");
        assert_eq!(stream.push(b"\x1b\x9dhidden\x07ok"), b"ok");
    }
}
