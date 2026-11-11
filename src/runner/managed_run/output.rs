//! Incremental consumption of verified canonical bytes. Presentation never
//! advances a second offset or decodes producer terminal controls.
use effigy_core::terminal_text::{StyledChunk, StyledSpan};
use effigy_kernel::custody::{FileIdentity, MergedLog};
use effigy_kernel::run::LogRead;
use std::path::Path;

pub(super) const READ_MAX: usize = 64 * 1024;

#[derive(Debug, Default)]
pub(super) struct Cursor {
    pub(super) offset: u64,
    identity: Option<FileIdentity>,
    partial: String,
    pub(super) failed: bool,
    warned: bool,
}

#[derive(Default)]
pub(super) struct Output {
    pub(super) from: u64,
    pub(super) text: String,
    pub(super) gap: bool,
    pub(super) replaced: bool,
    pub(super) truncated: bool,
    pub(super) failed: bool,
    pub(super) caught_up: bool,
}

impl Output {
    /// Style readers see exactly the validated scalar-aligned byte range.
    pub(super) fn styled(&self, read: impl FnOnce(u64, &[u8]) -> Vec<StyledSpan>) -> StyledChunk {
        StyledChunk {
            offset: self.from,
            spans: read(self.from, self.text.as_bytes()),
        }
    }
}

impl Cursor {
    pub(super) fn consume(&mut self, read: &LogRead, replaced: bool) -> Output {
        let mut output = Output {
            from: read.from,
            gap: read.from != self.offset && !replaced,
            replaced,
            truncated: read.truncated && !self.warned,
            ..Output::default()
        };
        self.warned |= read.truncated;
        if replaced || read.from < self.offset {
            self.failed = false;
        }
        if output.gap || replaced {
            self.partial.clear();
        }
        self.offset = read.from;
        if self.failed {
            self.offset += read.bytes.len() as u64;
            return output;
        }
        let valid = match std::str::from_utf8(&read.bytes) {
            Ok(text) => Some(text),
            Err(error) if error.error_len().is_none() => {
                std::str::from_utf8(&read.bytes[..error.valid_up_to()]).ok()
            }
            Err(_) => None,
        };
        if let Some(text) =
            valid.filter(|text| !text.chars().any(|ch| ch.is_control() && ch != '\n'))
        {
            self.offset += text.len() as u64;
            output.text.push_str(text);
        } else {
            self.offset += read.bytes.len() as u64;
            self.fail(&mut output);
        }
        output.caught_up = self.offset == read.end;
        output
    }

    fn fail(&mut self, output: &mut Output) {
        output.failed = !self.failed;
        self.failed = true;
        self.partial.clear();
        output.text.clear();
    }

    /// Line adapters share framing and the bounded partial buffer. Chunk
    /// adapters can deliver text immediately without imposing a line limit.
    pub(super) fn lines(&mut self, output: &mut Output, finish: bool) -> Vec<String> {
        let mut lines = Vec::new();
        for piece in output.text.split_inclusive('\n') {
            if self.partial.len() + piece.len() > READ_MAX {
                self.fail(output);
                return Vec::new();
            }
            self.partial.push_str(piece);
            if self.partial.ends_with('\n') {
                self.partial.pop();
                lines.push(std::mem::take(&mut self.partial));
            }
        }
        if finish && !self.partial.is_empty() {
            lines.push(std::mem::take(&mut self.partial));
        }
        lines
    }

    /// The canonical reader owns identity/range verification and replacement;
    /// no raw descriptor is opened by a follower.
    pub(super) fn host(&mut self, path: &Path) -> std::io::Result<(Output, StyledChunk)> {
        let read = MergedLog::read_plain(path, self.identity, self.offset, READ_MAX)?;
        self.identity = Some(read.log.identity);
        let spans = MergedLog::styles(path, &read);
        let output = self.consume(
            &LogRead {
                from: read.from,
                bytes: read.bytes,
                end: read.log.length,
                truncated: false,
            },
            read.cutover,
        );
        let chunk = output.styled(|_, _| spans);
        Ok((output, chunk))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn read(from: u64, bytes: &[u8]) -> LogRead {
        LogRead {
            from,
            bytes: bytes.to_vec(),
            end: from + bytes.len() as u64,
            truncated: false,
        }
    }

    #[test]
    fn cursor_boundary_table() {
        let mut cursor = Cursor::default();
        // A split scalar is retried from its first byte, just like canonical
        // readers do at their bounded read boundary. Replacement never joins
        // the old partial, even when the new file is larger.
        for (from, bytes, replaced, expected, offset) in [
            (0, &b"one\ntwo\npart"[..], false, "one|two", 12),
            (
                12,
                &b"ial line\nthree\n"[..],
                false,
                "partial line|three",
                27,
            ),
            (27, &b"caf\xc3"[..], false, "", 30),
            (30, &b"\xc3\xa9\nlast"[..], false, "café", 37),
            (100, &b"secret\n"[..], false, "secret", 107),
            (0, &b"old\npartial"[..], true, "old", 11),
            (
                0,
                &b"replacement grows\n"[..],
                true,
                "replacement grows",
                18,
            ),
            (0, "  café\n\nlast".as_bytes(), true, "  café|", 13),
        ] {
            let mut output = cursor.consume(&read(from, bytes), replaced);
            assert_eq!(
                cursor.lines(&mut output, false).join("|"),
                expected,
                "from={from}"
            );
            assert_eq!(
                (cursor.offset, output.gap, output.replaced),
                (offset, from == 100, replaced)
            );
            assert!(!output.failed && !cursor.failed && cursor.partial.len() <= READ_MAX);
        }
        assert_eq!(cursor.lines(&mut Output::default(), true), ["last"]);
        assert!(cursor.lines(&mut Output::default(), true).is_empty());
    }

    #[test]
    fn interleaved_streams_warn_once_and_keep_redacted_style_ranges() {
        let mut streams = [Cursor::default(), Cursor::default()];
        for (slot, from, bytes, truncated, expected) in [
            (0, 0, &b"[REDACTED]\xc3"[..], true, "[REDACTED]"),
            (1, 0, &b"err\n"[..], false, "err\n"),
            (0, 10, &b"\xc3\xa9\n"[..], true, "é\n"),
        ] {
            let mut output = streams[slot].consume(
                &LogRead {
                    truncated,
                    ..read(from, bytes)
                },
                false,
            );
            assert_eq!(output.text, expected);
            assert_eq!(output.truncated, slot == 0 && from == 0);
            let style = effigy_core::terminal_text::Style::default().fg(ratatui::style::Color::Red);
            let chunk = output.styled(|offset, bytes| {
                assert_eq!((offset, bytes), (from, expected.as_bytes()));
                vec![StyledSpan {
                    text: expected.to_owned(),
                    style,
                }]
            });
            assert_eq!(chunk.offset, from);
            assert_eq!(chunk.spans[0].text, expected);
            assert_eq!(chunk.spans[0].style, style);
            let lines = streams[slot].lines(&mut output, false);
            assert_eq!(
                lines,
                match (slot, from) {
                    (0, 0) => vec![],
                    (0, _) => vec!["[REDACTED]é"],
                    _ => vec!["err"],
                }
            );
        }
        assert_eq!((streams[0].offset, streams[1].offset), (13, 4));
    }

    #[test]
    fn oversized_line_stops_once_and_replacement_recovers() {
        let mut cursor = Cursor::default();
        for (from, bytes, failed) in [
            (0, vec![b'x'; READ_MAX], false),
            (READ_MAX as u64, vec![b'x'], true),
        ] {
            let mut output = cursor.consume(&read(from, &bytes), false);
            assert!(cursor.lines(&mut output, false).is_empty());
            assert_eq!(output.failed, failed);
            assert!(cursor.partial.len() <= READ_MAX);
        }
        assert!(cursor.failed && cursor.partial.is_empty());
        let mut output = cursor.consume(&read(0, b"fresh\n"), true);
        assert_eq!(cursor.lines(&mut output, false), ["fresh"]);
        assert!(!cursor.failed);
        let mut output = cursor.consume(&read(6, &[0xff]), false);
        assert!(output.failed && cursor.lines(&mut output, true).is_empty());
        output = cursor.consume(&read(7, b"late\n"), false);
        assert!(!output.failed && output.text.is_empty());
    }
}
