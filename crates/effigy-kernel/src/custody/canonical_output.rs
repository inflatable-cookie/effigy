//! One writer for parsed, redacted and durably appended managed output.
//! Host generations and owner messages share it; Standard processes and the
//! managed supervisor use the same pipeline with their journal destination.
use super::{
    CaptureState, EnvValue, MergedLog, OutputPipeline, OutputTag, ProcessSpec, StyledOutput,
};
use effigy_core::terminal_text::{StyledChunk, StyledSpan};
use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::io;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct CanonicalOutput(Arc<Mutex<Writer>>);
enum Destination {
    Host(MergedLog),
    Run(super::capture::Log),
}
impl Destination {
    fn committed_offset(&self) -> u64 {
        match self {
            Self::Host(log) => log.committed_offset(),
            Self::Run(log) => log.committed_offset(),
        }
    }
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        match self {
            Self::Host(log) => log.append(bytes),
            Self::Run(log) => {
                let result = log.append_durable(bytes);
                if result.is_err() {
                    log.mark_truncated();
                }
                result
            }
        }
    }
    fn publish_styles(&mut self, chunk: StyledChunk) -> io::Result<()> {
        match self {
            Self::Host(log) => log.publish_styles(chunk),
            Self::Run(log) => log.publish_styles(chunk),
        }
    }
    fn mark_failed(&self) {
        if let Self::Run(log) = self {
            log.mark_truncated();
        }
    }
}
struct Writer {
    log: Destination,
    pipeline: OutputPipeline,
    secrets: Vec<Vec<u8>>,
    pending: Vec<(OutputTag, u8)>,
    output: VecDeque<u8>,
    tail: VecDeque<u8>,
    styled: VecDeque<StyledChunk>,
    styled_bytes: usize,
    line_start: bool,
    queue_max: usize,
    tail_max: usize,
    truncated: bool,
    failed: bool,
    finished: bool,
}
impl std::fmt::Debug for Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanonicalOutput")
            .field("failed", &self.failed)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}
fn lock(writer: &Mutex<Writer>) -> std::sync::MutexGuard<'_, Writer> {
    writer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
impl CanonicalOutput {
    /// The complete classified producer environment establishes one secret
    /// boundary before any child or supervisor message reaches the file.
    pub fn new(
        log: MergedLog,
        env: &BTreeMap<OsString, EnvValue>,
        limits: &crate::ctx::Limits,
    ) -> io::Result<Self> {
        let secrets = env
            .values()
            .filter_map(|value| match value {
                EnvValue::Secret(value) => Some(value.bytes().to_vec()),
                _ => None,
            })
            .collect();
        Self::with_secrets(log, secrets, limits.queue_bytes, limits.capture_tail)
    }
    pub(super) fn with_secrets(
        log: MergedLog,
        secrets: Vec<Vec<u8>>,
        queue_max: usize,
        tail_max: usize,
    ) -> io::Result<Self> {
        Self::with_destination(Destination::Host(log), secrets, queue_max, tail_max)
    }
    pub(crate) fn for_run(
        mut log: super::capture::Log,
        secrets: Vec<Vec<u8>>,
        limits: &crate::ctx::Limits,
    ) -> io::Result<Self> {
        log.enable_plain()?;
        Self::with_destination(
            Destination::Run(log),
            secrets,
            limits.queue_bytes,
            limits.capture_tail,
        )
    }
    fn with_destination(
        log: Destination,
        secrets: Vec<Vec<u8>>,
        queue_max: usize,
        tail_max: usize,
    ) -> io::Result<Self> {
        if secrets
            .iter()
            .any(|value| std::str::from_utf8(value).is_err())
        {
            return Err(io::Error::other(
                "canonical output requires classified UTF-8 secrets",
            ));
        }
        let pipeline = OutputPipeline::for_capture(secrets.clone(), true);
        if pipeline.redactor.blocked() {
            return Err(io::Error::other(
                "canonical output has no reader-safe redaction replacement",
            ));
        }
        let line_start = log.committed_offset() == 0;
        Ok(Self(Arc::new(Mutex::new(Writer {
            line_start,
            log,
            pipeline,
            secrets,
            pending: Vec::new(),
            output: VecDeque::new(),
            tail: VecDeque::new(),
            styled: VecDeque::new(),
            styled_bytes: 0,
            queue_max,
            tail_max,
            truncated: false,
            failed: false,
            finished: false,
        }))))
    }
    pub(super) fn validate(&self, spec: &ProcessSpec) -> io::Result<()> {
        let writer = lock(&self.0);
        if writer.failed || writer.finished {
            return Err(io::Error::other(
                "host canonical capture is closed or failed",
            ));
        }
        if spec
            .validated_secrets()?
            .iter()
            .any(|value| !writer.secrets.contains(value))
        {
            return Err(io::Error::other(
                "generation secret provenance changed; restart the host owner before spawning",
            ));
        }
        Ok(())
    }
    pub(crate) fn feed(&self, stream: usize, bytes: &[u8], terminal_eof: bool) -> io::Result<()> {
        let mut writer = lock(&self.0);
        if writer.finished && bytes.is_empty() {
            return Ok(());
        }
        writer.check()?;
        let tagged: Vec<_> = bytes.iter().map(|byte| (stream as u8, *byte)).collect();
        let final_eof = terminal_eof && matches!(&writer.log, Destination::Run(_));
        let clean = writer
            .pipeline
            .push_terminal(&tagged, terminal_eof, final_eof);
        writer.commit(clean, final_eof)?;
        if final_eof {
            writer.finished = true;
        }
        Ok(())
    }
    /// Messages have their own terminal-control framing, so a child's partial
    /// OSC cannot swallow an owner message. They enter the SAME merged redactor
    /// and append path; there is no direct descriptor or independent redaction.
    pub fn message(&self, message: &str) -> io::Result<()> {
        let mut writer = lock(&self.0);
        writer.check()?;
        let mut bytes = Vec::new();
        // Frame owner messages before redaction, including a held child
        // prefix. Readers never have to regroup or repair those boundaries.
        if !writer.line_start || writer.pipeline.redactor.has_pending() {
            bytes.push((2, b'\n'));
        }
        bytes.extend(message.as_bytes().iter().map(|byte| (2, *byte)));
        bytes.push((2, b'\n'));
        let mut parser = super::terminal::TerminalSanitizer::new();
        let visible = parser
            .push_styled(&bytes, true)
            .into_iter()
            .filter(|(_, byte)| *byte != b'\r')
            .collect::<Vec<_>>();
        let clean = writer.pipeline.redactor.push_tagged(&visible, false);
        writer.commit(clean, false)
    }
    /// Flush only when the supervisor has closed every producer. Child EOF
    /// resets terminal framing, but does not end the supervisor's redactor.
    pub fn finish(&self) -> io::Result<()> {
        let mut writer = lock(&self.0);
        if writer.finished {
            return writer.check_finished();
        }
        writer.check()?;
        let clean = writer.pipeline.push_terminal(&[], true, true);
        writer.commit(clean, true)?;
        writer.finished = true;
        Ok(())
    }
    pub(crate) fn fail(&self) {
        let mut writer = lock(&self.0);
        writer.failed = true;
        writer.log.mark_failed();
    }
    pub fn capture_state(&self) -> CaptureState {
        if lock(&self.0).failed {
            CaptureState::Truncated("canonical capture failed".into())
        } else {
            CaptureState::Complete
        }
    }
    pub fn take_styled_output(&self) -> StyledOutput {
        let mut writer = lock(&self.0);
        writer.styled_bytes = 0;
        StyledOutput {
            chunks: writer.styled.drain(..).collect(),
            truncated: std::mem::take(&mut writer.truncated) || writer.failed,
        }
    }
    pub(super) fn take_output(&self) -> Vec<u8> {
        lock(&self.0).output.drain(..).collect()
    }
    pub(crate) fn tail(&self) -> Vec<u8> {
        lock(&self.0).tail.iter().copied().collect()
    }
}
impl Writer {
    fn check_finished(&self) -> io::Result<()> {
        if self.failed {
            Err(io::Error::other(
                "canonical output failed; inspect capture state",
            ))
        } else {
            Ok(())
        }
    }
    fn check(&mut self) -> io::Result<()> {
        self.check_finished()?;
        if self.finished {
            self.failed = true;
            return Err(io::Error::other("canonical output already finished"));
        }
        Ok(())
    }
    fn commit(&mut self, mut clean: Vec<(OutputTag, u8)>, eof: bool) -> io::Result<()> {
        let result = self.commit_inner(&mut clean, eof);
        if result.is_err() {
            self.log.mark_failed();
        }
        result
    }
    fn commit_inner(&mut self, clean: &mut Vec<(OutputTag, u8)>, eof: bool) -> io::Result<()> {
        self.failed = true;
        self.pending.append(clean);
        let bytes: Vec<_> = self.pending.iter().map(|(_, byte)| *byte).collect();
        let complete = match std::str::from_utf8(&bytes) {
            Ok(_) => bytes.len(),
            Err(error) if error.error_len().is_none() && !eof => error.valid_up_to(),
            Err(_) => return Err(io::Error::other("invalid canonical text; capture stopped")),
        };
        let remainder = self.pending.split_off(complete);
        let clean = std::mem::replace(&mut self.pending, remainder);
        let bytes = &bytes[..complete];
        let offset = self.log.committed_offset();
        self.log.append(bytes)?;
        // Only successful durable appends reach any live/tail projection.
        if !bytes.is_empty() {
            self.line_start = bytes.last() == Some(&b'\n');
            let spans = spans(&clean);
            self.styled_bytes = self.styled_bytes.saturating_add(bytes.len());
            let chunk = StyledChunk { offset, spans };
            if self.log.publish_styles(chunk.clone()).is_err() {
                self.truncated = true;
            }
            self.styled.push_back(chunk);
            while self.styled_bytes > self.queue_max {
                let Some(old) = self.styled.pop_front() else {
                    break;
                };
                self.styled_bytes = self
                    .styled_bytes
                    .saturating_sub(old.spans.iter().map(|span| span.text.len()).sum());
                self.truncated = true;
            }
            self.output.extend(bytes);
            self.tail.extend(bytes);
            bound(&mut self.output, self.queue_max);
            bound(&mut self.tail, self.tail_max);
        }
        self.failed = false;
        Ok(())
    }
}
fn bound(bytes: &mut VecDeque<u8>, max: usize) {
    while bytes.len() > max {
        bytes.pop_front();
    }
    while bytes
        .front()
        .is_some_and(|byte| (0x80..=0xbf).contains(byte))
    {
        bytes.pop_front();
    }
}
pub(super) fn spans(clean: &[(OutputTag, u8)]) -> Vec<StyledSpan> {
    let mut result = Vec::new();
    let mut begin = 0;
    while begin < clean.len() {
        let style = clean[begin].0.style;
        let mut end = begin + 1;
        while end < clean.len() && clean[end].0.style == style {
            end += 1;
        }
        let bytes: Vec<_> = clean[begin..end].iter().map(|(_, byte)| *byte).collect();
        // The parser assigns all bytes of a validated scalar the same style.
        let text = std::str::from_utf8(&bytes)
            .expect("validated scalar style boundaries")
            .to_owned();
        result.push(StyledSpan { text, style });
        begin = end;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_core::terminal_text::Style;
    #[test]
    fn generations_messages_and_tap_share_one_redaction_and_durability_boundary() {
        let home = effigy_test_support::short_tempdir("ho-").unwrap();
        let path = home.path().join("app.log");
        std::fs::write(&path, b"\x1b[31mlegacy-secret\n").unwrap();
        let output = CanonicalOutput::with_secrets(
            MergedLog::prepare_test(&path).unwrap(),
            vec![b"supersecret".to_vec()],
            4096,
            4096,
        )
        .unwrap();
        output
            .feed(0, b"\x1b[32msafe\x1b[0m\nsuper\x1b]unterminated", false)
            .unwrap();
        output.feed(0, b"", true).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"safe\n");
        output.feed(1, b"secret\n", false).unwrap();
        output.message("owner super\x1b[31msecret").unwrap();
        output.finish().unwrap();
        let read = MergedLog::read_plain(&path, None, 0, 64 * 1024).unwrap();
        assert_eq!(read.bytes, b"safe\n[REDACTED]\nowner [REDACTED]\n");
        if std::env::var_os("EFFIGY_TEST_CANONICAL_TRIPWIRE").is_some() {
            assert!(
                read.bytes.contains(&0x1b),
                "deliberately inverted canonical safety assertion"
            );
        }
        let styles = MergedLog::styles(&path, &read);
        assert_eq!(
            styles
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>()
                .as_bytes(),
            read.bytes
        );
        assert_ne!(styles[0].style, Style::default());
        for span in &styles {
            if span.text.contains("REDACTED") {
                assert_eq!(span.style, Style::default());
            }
        }
        assert_eq!(output.take_output(), read.bytes);
        assert_eq!(output.tail(), read.bytes);
        assert_eq!(output.capture_state(), CaptureState::Complete);
        assert!(output.feed(0, b"late", false).is_err());
        assert!(matches!(output.capture_state(), CaptureState::Truncated(_)));
    }

    #[test]
    fn an_owner_message_starts_a_line_after_a_held_child_prefix() {
        let home = effigy_test_support::short_tempdir("hm-").unwrap();
        let path = home.path().join("app.log");
        let output = CanonicalOutput::with_secrets(
            MergedLog::prepare_test(&path).unwrap(),
            vec![b"partialtail".to_vec()],
            4096,
            4096,
        )
        .unwrap();
        output.feed(0, b"partial", false).unwrap();
        output
            .message("[effigy host-process] started `app`")
            .unwrap();
        output.feed(1, b"tail\n", false).unwrap();
        output.finish().unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"partial\n[effigy host-process] started `app`\ntail\n"
        );
    }

    #[cfg(feature = "fault-injection")]
    #[test]
    fn partial_append_and_sync_failures_never_acknowledge_a_live_byte() {
        for sync in [false, true] {
            let home = effigy_test_support::short_tempdir("hf-").unwrap();
            let path = home.path().join("app.log");
            let output = CanonicalOutput::with_secrets(
                MergedLog::prepare_test(&path).unwrap(),
                vec![],
                4096,
                4096,
            )
            .unwrap();
            if sync {
                crate::fault::arm_point(
                    crate::fault::Point::CaptureSync,
                    crate::fault::Fault::Enospc,
                );
            } else {
                crate::fault::arm_nth(
                    crate::fault::Point::CaptureWrite,
                    1,
                    crate::fault::Fault::Enospc,
                );
            }
            assert!(output.feed(0, &vec![b'x'; 8192], false).is_err());
            assert!(crate::fault::fired());
            crate::fault::arm(None);
            assert_eq!(
                std::fs::read(&path).unwrap().len(),
                if sync { 8192 } else { 4096 }
            );
            assert_eq!(lock(&output.0).log.committed_offset(), 0);
            assert!(output.take_output().is_empty());
            assert!(output.tail().is_empty());
            let styles = output.take_styled_output();
            assert!(styles.truncated && styles.chunks.is_empty());
            assert!(output.finish().is_err());
        }
    }
}
