//! The one path from a secret-bearing process's merged raw output to every sink.
//!
//! Terminal controls are removed first, so the redactor sees exactly the
//! visible bytes a front end would show; redaction runs after that pass. The
//! pipeline and the redactor are one obligation (K12) and one owner: splitting
//! that order, or normalizing after redaction, reopens the split secret —
//! `super` CSI `secret` is one value to a terminal and two non-matching runs to
//! a redactor over the raw bytes.
use super::terminal;
/// A bounded live projection of canonical output, with explicit queue loss.
#[derive(Debug, Clone, Default)]
pub struct StyledOutput {
    pub chunks: Vec<effigy_core::terminal_text::StyledChunk>,
    pub truncated: bool,
}
/// The one path from a secret-bearing process's merged raw output to every
/// sink (K12): terminal controls are removed first, so the redactor sees
/// exactly the visible bytes a front end would show, then redaction runs.
/// Splitting that order, or normalizing after redaction, reopens the split
/// secret: `super` CSI `secret` is one value to a terminal and two
/// non-matching runs to a redactor over the raw bytes.
#[derive(Debug)]
pub(crate) struct OutputPipeline {
    /// `Some` only for a secret-bearing process. An ordinary process keeps its
    /// terminal colour, exactly as before this seam existed.
    pub(crate) sanitizer: Option<terminal::TerminalSanitizer>,
    pub(crate) redactor: Redactor<OutputTag>,
    pub(crate) plain: bool,
    /// One redactor per stream, present only for a secret-bearing process.
    ///
    /// Splitting the merged result by origin stream is a byte removal, and a
    /// removal can join two halves of a secret that the merged redaction
    /// correctly left alone, because the other stream's *visible* bytes sat
    /// between them (`super` on stdout, `noise` on stderr, `secret` on
    /// stdout). Each projection is therefore redacted again over its own byte
    /// stream before it reaches a stored log or a queue. Keeping both streams
    /// in arrival order is the only capture that cannot be reordered, so the
    /// merged output and the merged tail stay canonical, and a consumer that
    /// wants both streams reads them, never a concatenation of projections.
    pub(crate) projections: Option<[Redactor; 2]>,
}
impl OutputPipeline {
    pub(crate) fn new(secrets: Vec<Vec<u8>>) -> Self {
        Self::for_capture(secrets, false, true)
    }
    pub(crate) fn for_capture(mut secrets: Vec<Vec<u8>>, plain: bool, project: bool) -> Self {
        if plain {
            secrets.extend(
                secrets
                    .iter()
                    .map(|secret| {
                        terminal::visible(secret)
                            .into_iter()
                            .filter(|byte| *byte != b'\r')
                            .collect::<Vec<_>>()
                    })
                    .filter(|secret| !secret.is_empty())
                    .collect::<Vec<_>>(),
            );
        }
        let secret_bearing = !secrets.is_empty();
        Self {
            sanitizer: (secret_bearing || plain).then(terminal::TerminalSanitizer::new),
            plain,
            projections: (secret_bearing && project).then(|| {
                [
                    Redactor::new(secrets.clone()),
                    Redactor::new(secrets.clone()),
                ]
            }),
            redactor: Redactor::with_tags(secrets),
        }
    }
    /// Sanitize, then redact, one merged arrival-order slice. Both steps keep
    /// each byte's origin tag.
    pub(crate) fn push_tagged(&mut self, bytes: &[(u8, u8)], eof: bool) -> Vec<(OutputTag, u8)> {
        self.push_terminal(bytes, eof, eof)
    }
    pub(crate) fn push_terminal(
        &mut self,
        bytes: &[(u8, u8)],
        terminal_eof: bool,
        eof: bool,
    ) -> Vec<(OutputTag, u8)> {
        match self.sanitizer.as_mut() {
            Some(sanitizer) => {
                let mut visible = sanitizer.push_styled(bytes, terminal_eof);
                if self.plain {
                    visible.retain(|(_, byte)| *byte != b'\r');
                }
                // Empty is not a no-op: EOF still flushes the redactor.
                self.redactor.push_tagged(&visible, eof)
            }
            None => {
                let tagged: Vec<_> = bytes
                    .iter()
                    .map(|(stream, byte)| {
                        (
                            OutputTag {
                                stream: *stream,
                                ..OutputTag::default()
                            },
                            *byte,
                        )
                    })
                    .collect();
                self.redactor.push_tagged(&tagged, eof)
            }
        }
    }
    /// Each stream's own safe projection of one merged result. A stream sink
    /// is only ever written from this, never from a filtered merged slice.
    pub(crate) fn project(&mut self, merged: &[(OutputTag, u8)], eof: bool) -> [Vec<u8>; 2] {
        let mut raw: [Vec<u8>; 2] = Default::default();
        for &(tag, byte) in merged {
            if let Some(part) = raw.get_mut(usize::from(tag.stream)) {
                part.push(byte);
            }
        }
        let Some(projections) = self.projections.as_mut() else {
            return raw;
        };
        for (stream, redactor) in projections.iter_mut().enumerate() {
            // The projection is its own stream of bytes, so it is redacted as
            // one: a secret its own bytes form is caught even when arrival
            // order interleaved them with the other stream's.
            raw[stream] = redactor.push(&raw[stream], eof);
        }
        raw
    }
}
pub(crate) trait RedactionTag: Copy {
    fn masked(self) -> Self;
}
impl RedactionTag for u8 {
    fn masked(self) -> Self {
        self
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct OutputTag {
    pub(crate) stream: u8,
    pub(crate) style: effigy_core::terminal_text::Style,
}
impl RedactionTag for OutputTag {
    fn masked(self) -> Self {
        Self {
            stream: self.stream,
            ..Self::default()
        }
    }
}
pub(crate) struct Redactor<T: RedactionTag = u8> {
    /// `None` when no reader-safe byte is absent from every secret, so no
    /// replacement can be emitted that a reader cannot join across: nothing is
    /// redacted and the capture stops (`blocked`) rather than leak.
    display: Option<RedactionPass<T>>,
    final_check: Option<RedactionPass<T>>,
}
impl<T: RedactionTag> std::fmt::Debug for Redactor<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Redactor([REDACTED])")
    }
}
impl<T: RedactionTag> Redactor<T> {
    fn with_tags(mut secrets: Vec<Vec<u8>>) -> Self {
        // A PTY's output processing expands LF to CRLF. Match that display
        // representation too, before it reaches logs, tails or a front end.
        let terminal = secrets
            .iter()
            .filter(|secret| secret.contains(&b'\n'))
            .map(|secret| {
                let mut displayed = Vec::with_capacity(secret.len());
                for &byte in secret {
                    if byte == b'\n' {
                        displayed.push(b'\r');
                    }
                    displayed.push(byte);
                }
                displayed
            })
            .collect::<Vec<_>>();
        secrets.extend(terminal);
        // A secret can itself carry a terminal control. The sanitizer removes
        // it, so the bytes a reader shows are that value's visible form; match
        // it too, or stripping a control out of a secret would leave a visible
        // remainder no pattern covers (K12).
        secrets.extend(
            secrets
                .iter()
                .map(|secret| terminal::visible(secret))
                .filter(|visible| !visible.is_empty())
                .collect::<Vec<_>>(),
        );
        secrets.sort();
        secrets.dedup();
        // A secret can itself contain the display marker, or a replacement can
        // join adjacent bytes into another secret. A second streaming pass
        // replaces such an occurrence with one byte: a byte *no reader drops*
        // and no secret contains. Both halves matter. A control byte (a NUL,
        // say) is dropped by a reader, which joins the bytes either side of it
        // and can rebuild the very secret this pass removed; a byte a secret
        // contains could be part of one. With a byte in neither class, the
        // replacement can neither hold a secret nor be spanned by one.
        let replacement = final_replacement(&secrets);
        Self {
            display: replacement
                .map(|_| RedactionPass::new(secrets.clone(), b"[REDACTED]".to_vec())),
            final_check: replacement.map(|byte| RedactionPass::new(secrets, vec![byte])),
        }
    }
    pub(crate) fn has_pending(&self) -> bool {
        self.display
            .as_ref()
            .is_some_and(|pass| !pass.pending.is_empty())
            || self
                .final_check
                .as_ref()
                .is_some_and(|pass| !pass.pending.is_empty())
    }
    /// Whether no reader-safe replacement byte exists for this secret set, so
    /// nothing can be emitted at all.
    pub(crate) fn blocked(&self) -> bool {
        self.display.is_none()
    }
    /// Redacts `(stream, byte)` pairs in arrival order. A replacement takes
    /// the stream of the secret's first byte, so a secret split across
    /// stdout and stderr is redacted whole and attributed to one stream.
    pub(crate) fn push_tagged(&mut self, bytes: &[(T, u8)], eof: bool) -> Vec<(T, u8)> {
        let (Some(display), Some(final_check)) = (self.display.as_mut(), self.final_check.as_mut())
        else {
            return Vec::new();
        };
        let shown = display.push(bytes, eof);
        final_check.push(&shown, eof)
    }
}

impl Redactor<u8> {
    pub(crate) fn new(secrets: Vec<Vec<u8>>) -> Self {
        Self::with_tags(secrets)
    }
    pub(crate) fn push(&mut self, bytes: &[u8], eof: bool) -> Vec<u8> {
        let tagged: Vec<(u8, u8)> = bytes.iter().map(|byte| (0, *byte)).collect();
        self.push_tagged(&tagged, eof)
            .into_iter()
            .map(|(_, byte)| byte)
            .collect()
    }
}

/// The byte a re-formed secret is replaced with: the first value a reader keeps
/// and no secret contains, preferred in an order that keeps output readable.
/// Every control is either dropped by a reader or moved by a renderer, so only
/// printable ASCII is a candidate. `None` when the secrets cover them all: no
/// replacement could then be both safe and readable.
pub(crate) fn final_replacement(secrets: &[Vec<u8>]) -> Option<u8> {
    const PREFERRED: &[u8] = b"~`^|\\{}<>$&*?!;:+=@%#/\"',()[]";
    let absent = |byte: u8| !secrets.iter().any(|secret| secret.contains(&byte));
    PREFERRED
        .iter()
        .copied()
        .chain((0x20..=0x7e).filter(|byte| !PREFERRED.contains(byte)))
        .find(|byte| absent(*byte))
}
pub(crate) struct RedactionPass<T: RedactionTag> {
    secrets: Vec<Vec<u8>>,
    pending: Vec<(T, u8)>,
    replacement: Vec<u8>,
}
impl<T: RedactionTag> RedactionPass<T> {
    fn new(mut secrets: Vec<Vec<u8>>, replacement: Vec<u8>) -> Self {
        secrets.sort_by_key(|v| std::cmp::Reverse(v.len()));
        Self {
            secrets,
            pending: Vec::new(),
            replacement,
        }
    }
    fn push(&mut self, bytes: &[(T, u8)], eof: bool) -> Vec<(T, u8)> {
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        let mut pos = 0;
        while pos < self.pending.len() {
            let rest = &self.pending[pos..];
            // Only a suffix that could still complete a known value needs
            // lookahead. Long inherited values must not stall unrelated live
            // lines until max-secret-length bytes or process EOF arrive.
            if !eof
                && self.secrets.iter().any(|secret| {
                    rest.len() < secret.len()
                        && rest
                            .iter()
                            .zip(secret)
                            .all(|((_, byte), expected)| byte == expected)
                })
            {
                break;
            }
            if let Some(secret) = self.secrets.iter().find(|s| {
                rest.len() >= s.len() && rest.iter().zip(s.iter()).all(|((_, b), c)| b == c)
            }) {
                let stream = rest[0].0.masked();
                out.extend(self.replacement.iter().map(|byte| (stream, *byte)));
                pos += secret.len();
            } else {
                out.push(rest[0]);
                pos += 1;
            }
        }
        self.pending.drain(..pos);
        out
    }
}
