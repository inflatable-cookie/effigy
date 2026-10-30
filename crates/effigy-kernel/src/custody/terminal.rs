//! Terminal-control sanitizing for secret-bearing custody output (K12).
//!
//! A reader that interprets terminal controls can reassemble a value a child
//! split with one: `super ESC [ 1 m secret` is one secret in the terminal's
//! frame and two non-matching byte runs to a redactor over the raw stream, and
//! a C1 control (`U+009B` CSI, say) or an invalid byte is *dropped* by the
//! renderer, which joins the halves either side of it. Secret-bearing output
//! therefore leaves only well-formed, control-free text, so no reader drops a
//! byte — and the redactor runs *after* that pass, so any bytes this machine
//! removes are joined in the frame the redactor sees, which is the frame a
//! display could show. Ordinary processes keep their colour: a process whose
//! spawn spec carries no secret never enters this path.
//!
//! State is one enum plus at most one partial UTF-8 sequence. Every ambiguity
//! is fail-closed: a byte or sequence the machine cannot classify is dropped
//! rather than emitted, an unterminated control or sequence drops its bytes at
//! EOF, and no path ever emits a byte it held back.
//!
//! Line structure is kept, but a carriage return survives only as part of a
//! line ending: `CR LF` passes, every other CR is dropped. A lone CR is a
//! horizontal control — a display erases what it covers, and a reader that
//! strips it instead joins the runs on either side — so a secret split by one
//! is a split a redactor over the retained bytes never sees. Dropping it joins
//! the halves before redaction, which is the frame a display could show. The
//! `LF` of a line ending stays a separator no reader joins across, and a
//! secret that itself contains a newline stays matchable.

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Visible text: only bytes a terminal prints pass.
    #[default]
    Ground,
    /// After a CR: kept only when the next byte is LF.
    CarriageReturn,
    /// Inside a multi-byte UTF-8 sequence, with `need` bytes still to come.
    Utf8 { need: u8 },
    /// After ESC; the next byte says which sequence follows.
    Escape,
    /// ESC plus a 0x20..=0x2F intermediate (a charset designator, say).
    EscapeIntermediate,
    /// `ESC [`: parameters and intermediates, ended by one final byte.
    Csi,
    /// `ESC ]`: an OSC string, ended by BEL or ST.
    Osc,
    /// `ESC P`/`X`/`^`/`_`: a DCS, SOS, PM or APC string, ended by ST.
    String,
    /// An ESC inside an OSC string: `\` closes it, anything else resumes it.
    OscAfterEsc,
    /// An ESC inside a DCS/SOS/PM/APC string, on the same rule.
    StringAfterEsc,
}

#[derive(Debug, Default)]
pub(crate) struct TerminalSanitizer {
    state: State,
    /// The bytes of the sequence being validated, and the tag they are emitted
    /// with: a sequence is one visible character, so it belongs to the stream
    /// that began it even when the other stream supplied its tail.
    utf8: Vec<u8>,
    tag: u8,
    scalar: u32,
}

impl TerminalSanitizer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Emits the visible bytes of `bytes` in order, each with the tag of the
    /// stream that produced it. `eof` finishes the process's merged output: a
    /// sequence, control or partial character still open is dropped, never
    /// completed by later output, and the machine starts from ground if the
    /// core is fed again.
    pub(crate) fn push_tagged(&mut self, bytes: &[(u8, u8)], eof: bool) -> Vec<(u8, u8)> {
        let mut out = Vec::with_capacity(bytes.len());
        for &(tag, byte) in bytes {
            // A dropped lone CR says nothing about the byte after it, so that
            // byte is handled as if the CR had never been there.
            let mut pending = Some(byte);
            while let Some(byte) = pending.take() {
                self.state = match self.state {
                    State::Ground => self.ground(tag, byte, &mut out),
                    State::Utf8 { need } => match byte {
                        0x80..=0xbf => {
                            self.utf8.push(byte);
                            self.scalar = (self.scalar << 6) | u32::from(byte & 0x3f);
                            if need > 1 {
                                State::Utf8 { need: need - 1 }
                            } else {
                                self.finish_utf8(&mut out);
                                State::Ground
                            }
                        }
                        // Not a continuation: the sequence is malformed, and
                        // this byte starts whatever comes next.
                        _ => {
                            self.drop_utf8();
                            pending = Some(byte);
                            State::Ground
                        }
                    },
                    State::CarriageReturn => match byte {
                        b'\n' => {
                            out.push((tag, b'\r'));
                            out.push((tag, b'\n'));
                            State::Ground
                        }
                        // Fail closed: the CR is gone and the byte is re-read.
                        _ => {
                            pending = Some(byte);
                            State::Ground
                        }
                    },
                    State::Escape => match byte {
                        b'[' => State::Csi,
                        b']' => State::Osc,
                        b'P' | b'X' | b'^' | b'_' => State::String,
                        0x20..=0x2f => State::EscapeIntermediate,
                        0x1b => State::Escape,
                        // A complete two-byte escape, a cancel or a malformed
                        // introducer all end at ground without emitting.
                        _ => State::Ground,
                    },
                    State::EscapeIntermediate => match byte {
                        0x20..=0x2f => State::EscapeIntermediate,
                        _ => State::Ground,
                    },
                    State::Csi => match byte {
                        // Parameter and intermediate bytes continue it.
                        0x20..=0x3f => State::Csi,
                        // One final byte ends it, whether well formed or not.
                        0x40..=0x7e => State::Ground,
                        // CAN and SUB abort it; anything else is malformed.
                        // The byte is dropped either way, so a split secret
                        // rejoins in the redactor's frame.
                        _ => State::Ground,
                    },
                    State::Osc => match byte {
                        0x07 | 0x18 | 0x1a => State::Ground,
                        0x1b => State::OscAfterEsc,
                        _ => State::Osc,
                    },
                    State::OscAfterEsc => match byte {
                        b'\\' => State::Ground,
                        _ => State::Osc,
                    },
                    State::String => match byte {
                        0x18 | 0x1a => State::Ground,
                        0x1b => State::StringAfterEsc,
                        _ => State::String,
                    },
                    State::StringAfterEsc => match byte {
                        b'\\' => State::Ground,
                        _ => State::String,
                    },
                };
            }
        }
        if eof {
            // Fail closed: a character or sequence still open is dropped, so
            // no later read can complete it into visible text.
            self.drop_utf8();
            self.state = State::Ground;
        }
        out
    }

    /// One byte of ordinary text: printable ASCII passes, a control is dropped,
    /// and a multi-byte character is validated before any of it is emitted.
    fn ground(&mut self, tag: u8, byte: u8, out: &mut Vec<(u8, u8)>) -> State {
        match byte {
            0x1b => State::Escape,
            0x0d => State::CarriageReturn,
            // TAB and LF keep the output's line structure.
            0x09 | 0x0a => {
                out.push((tag, byte));
                State::Ground
            }
            // Every other C0 control and DEL is dropped.
            0x00..=0x1f | 0x7f => State::Ground,
            // A multi-byte sequence starts: hold it until it validates, so an
            // invalid or C1 character never reaches a reader that would drop
            // it and join the runs either side.
            0xc2..=0xdf => self.start_utf8(tag, byte, 1),
            0xe0..=0xef => self.start_utf8(tag, byte, 2),
            0xf0..=0xf4 => self.start_utf8(tag, byte, 3),
            // A stray continuation byte, an overlong lead (C0, C1) or a lead
            // beyond U+10FFFF is not text.
            0x80..=0xc1 | 0xf5..=0xff => State::Ground,
            _ => {
                out.push((tag, byte));
                State::Ground
            }
        }
    }

    fn start_utf8(&mut self, tag: u8, byte: u8, need: u8) -> State {
        let scalar = match need {
            1 => u32::from(byte & 0x1f),
            2 => u32::from(byte & 0x0f),
            _ => u32::from(byte & 0x07),
        };
        self.utf8.clear();
        self.utf8.push(byte);
        self.tag = tag;
        self.scalar = scalar;
        State::Utf8 { need }
    }

    /// Emit a valid sequence, or drop it when it is malformed or a C1 control.
    fn finish_utf8(&mut self, out: &mut Vec<(u8, u8)>) {
        let scalar = self.scalar;
        let complete = match self.utf8.len() {
            2 => scalar >= 0x80,
            3 => scalar >= 0x800 && !(0xd800..=0xdfff).contains(&scalar),
            4 => (0x10000..=0x10ffff).contains(&scalar),
            _ => false,
        };
        // U+0080..=U+009F are the C1 controls: a renderer drops them, so they
        // are dropped here, before redaction, where the join is still visible.
        let text = complete && !(0x80..=0x9f).contains(&scalar);
        if text {
            let tag = self.tag;
            out.extend(self.utf8.iter().map(|byte| (tag, *byte)));
        }
        self.drop_utf8();
    }

    fn drop_utf8(&mut self) {
        self.utf8.clear();
        self.scalar = 0;
    }
}

/// The visible form of `bytes`, one shot: the same rules with nothing open
/// before or after. Used for a secret's own display form, which a reader would
/// show with the controls removed.
pub(crate) fn visible(bytes: &[u8]) -> Vec<u8> {
    let tagged: Vec<(u8, u8)> = bytes.iter().map(|byte| (0, *byte)).collect();
    TerminalSanitizer::new()
        .push_tagged(&tagged, true)
        .into_iter()
        .map(|(_, byte)| byte)
        .collect()
}
