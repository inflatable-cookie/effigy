use ratatui::style::Style;
use ratatui::text::Line;
use vt100::Parser as VtParser;

use super::super::config::{VT_PARSER_COLS, VT_PARSER_ROWS, VT_PARSER_SCROLLBACK};
use super::{ansi_line, vt_logs};

pub(crate) struct LiveTerminalBuffer {
    parser: VtParser,
    utf8_carry: Vec<u8>,
}

impl LiveTerminalBuffer {
    pub(crate) fn new() -> Self {
        Self {
            parser: VtParser::new(VT_PARSER_ROWS, VT_PARSER_COLS, VT_PARSER_SCROLLBACK),
            utf8_carry: Vec::new(),
        }
    }

    pub(crate) fn push_chunk(&mut self, bytes: &[u8]) {
        let complete = take_complete_terminal_bytes(&mut self.utf8_carry, bytes);
        if !complete.is_empty() {
            self.parser.process(&complete);
        }
    }

    pub(crate) fn finalize(&mut self) {
        if self.utf8_carry.is_empty() {
            return;
        }
        let flushed = String::from_utf8_lossy(&self.utf8_carry).into_owned();
        self.parser.process(flushed.as_bytes());
        self.utf8_carry.clear();
    }

    pub(crate) fn render_lines(
        &mut self,
        width: usize,
        height: usize,
        scroll_offset: usize,
    ) -> (Vec<Line<'static>>, usize) {
        render_vt_lines(&mut self.parser, width, height, scroll_offset)
    }
}

pub(crate) fn render_vt_lines(
    parser: &mut VtParser,
    width: usize,
    height: usize,
    scroll_offset: usize,
) -> (Vec<Line<'static>>, usize) {
    let (logs, clamped_scroll, _) =
        vt_logs(parser, height.max(1), width.max(1), scroll_offset, false);
    let lines = logs
        .into_iter()
        .map(|entry| ansi_line(&entry.line, Style::default()))
        .collect::<Vec<_>>();
    (lines, clamped_scroll)
}

pub(crate) fn take_complete_terminal_bytes(carry: &mut Vec<u8>, bytes: &[u8]) -> Vec<u8> {
    if carry.is_empty() {
        return trim_incomplete_utf8_suffix(bytes, carry);
    }

    let mut combined = Vec::with_capacity(carry.len() + bytes.len());
    combined.extend_from_slice(carry);
    combined.extend_from_slice(bytes);
    carry.clear();
    trim_incomplete_utf8_suffix(&combined, carry)
}

fn trim_incomplete_utf8_suffix(bytes: &[u8], carry: &mut Vec<u8>) -> Vec<u8> {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.to_vec(),
        Err(error) if error.error_len().is_none() => {
            let valid = error.valid_up_to();
            carry.extend_from_slice(&bytes[valid..]);
            bytes[..valid].to_vec()
        }
        Err(_) => bytes.to_vec(),
    }
}
