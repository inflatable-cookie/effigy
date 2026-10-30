//! Terminal-control sanitizing for secret-bearing custody output (K12).
//!
//! A reader that interprets CSI or OSC can reassemble a value a child split
//! with a control sequence: `super ESC [ 1 m secret` is one secret in the
//! terminal's frame and two non-matching byte runs to a redactor over the raw
//! stream. Secret-bearing output therefore drops terminal controls before
//! redaction, so the redactor sees exactly the visible bytes a front end would
//! show. Ordinary processes keep their colour: a process whose spawn spec
//! carries no secret never enters this path.
//!
//! State is one enum. A control sequence's bytes are discarded as they arrive,
//! so nothing is buffered and a read boundary or a stream boundary cannot
//! split a decision. Every ambiguity is fail-closed: a byte the machine cannot
//! classify is dropped rather than emitted, an unterminated control drops its
//! bytes at EOF, and no path ever emits a byte it held back.
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
}

impl TerminalSanitizer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Emits the visible bytes of `bytes` in order, each with the tag of the
    /// stream that produced it. `eof` finishes the process's merged output: a
    /// sequence or a CR still open is dropped, never completed by later
    /// output, and the machine starts from ground if the core is fed again.
    pub(crate) fn push_tagged(&mut self, bytes: &[(u8, u8)], eof: bool) -> Vec<(u8, u8)> {
        let mut out = Vec::with_capacity(bytes.len());
        for &(tag, byte) in bytes {
            // A dropped lone CR says nothing about the byte after it, so that
            // byte is handled as if the CR had never been there.
            let mut pending = Some(byte);
            while let Some(byte) = pending.take() {
                self.state = match self.state {
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
                    State::Ground => match byte {
                        0x1b => State::Escape,
                        0x0d => State::CarriageReturn,
                        // TAB and LF keep the output's line structure.
                        0x09 | 0x0a => {
                            out.push((tag, byte));
                            State::Ground
                        }
                        // Every other C0 control and DEL is dropped.
                        0x00..=0x1f | 0x7f => State::Ground,
                        _ => {
                            out.push((tag, byte));
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
            self.state = State::Ground;
        }
        out
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
