//! Bounded redacted rotating logs. I/O failure changes capture metadata only.
//!
//! Each process stream has its own log, as segments named by the absolute
//! byte offset they start at: `<seq>.<stream>.<offset>.log`. A reader keeps
//! an exact position across rotation, and a position older than the oldest
//! kept segment is reported as expired, never silently skipped (§7.3 family 6).
use super::*;
use crate::fs::{DirFd, FileName, Trust};
use crate::root::{Family, StateRoot};

/// One process output stream, logged and read separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stream {
    Stdout,
    Stderr,
}
impl Stream {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

pub(crate) fn segment_name(sequence: i64, stream: Stream, base: u64) -> String {
    format!("{sequence}.{}.{base}.log", stream.name())
}

/// The marker a failed log write leaves, so readers report truncation.
pub(crate) fn truncated_name(sequence: i64, stream: Stream) -> String {
    format!("{sequence}.{}.truncated", stream.name())
}

#[derive(Debug)]
pub(crate) struct Log {
    dir: DirFd,
    sequence: i64,
    stream: Stream,
    file: std::fs::File,
    /// Start offsets of the kept segments, oldest first; the last is open.
    bases: VecDeque<u64>,
    written: u64,
    rotate: u64,
    keep: u8,
    tap: super::styled_tap::Tap,
    durable: bool,
}
impl Log {
    pub fn open(
        root: &StateRoot,
        run: crate::id::RunId,
        sequence: i64,
        stream: Stream,
        limits: &crate::ctx::Limits,
    ) -> io::Result<Self> {
        let parent = root
            .family(Family::Runs)
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other("runs directory missing"))?;
        let dir = parent
            .child(&run.to_string(), Trust::Private, true)
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other("run directory missing"))?;
        let file = create(&dir, sequence, stream, 0)?;
        let tap = super::styled_tap::Tap::new(
            dir.clone(),
            style_name(sequence)?,
            [sequence as u64, 0],
            limits.queue_bytes,
        );
        Ok(Self {
            tap,
            durable: false,
            dir,
            sequence,
            stream,
            file,
            bases: VecDeque::from([0]),
            written: 0,
            rotate: limits.log_rotate.max(1),
            keep: limits.log_keep,
        })
    }
    pub fn committed_offset(&self) -> u64 {
        self.bases.back().copied().unwrap_or(0) + self.written
    }
    pub fn enable_plain(&mut self) -> io::Result<()> {
        self.durable = true;
        let name = plain_name(self.sequence)?;
        let (mut file, _) = self
            .dir
            .open_file(
                &name,
                libc::O_WRONLY | libc::O_EXCL,
                Some(0o600),
                Trust::Private,
            )
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other("plain format record missing"))?;
        crate::fs::fchmod(&file, 0o600)?;
        file.write_all(b"effigy.plain.v1\n")?;
        file.sync_all()?;
        self.dir.sync().map_err(io::Error::other)
    }
    pub fn publish_styles(
        &mut self,
        chunk: effigy_core::terminal_text::StyledChunk,
    ) -> io::Result<()> {
        self.tap.append(chunk)
    }
    pub fn append(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        crate::fault_point!(CaptureWrite)?;
        while !bytes.is_empty() {
            if self.written == self.rotate {
                if self.durable {
                    self.file.sync_all()?;
                }
                let base = self.bases.back().copied().unwrap_or(0) + self.written;
                self.file = create(&self.dir, self.sequence, self.stream, base)?;
                self.bases.push_back(base);
                self.written = 0;
                while self.bases.len() > usize::from(self.keep) + 1 {
                    let oldest = self.bases.pop_front().expect("kept segment");
                    let name = FileName::new(&segment_name(self.sequence, self.stream, oldest))
                        .map_err(io::Error::other)?;
                    self.dir.remove(&name, false).map_err(io::Error::other)?;
                }
            }
            let count = bytes
                .len()
                .min((self.rotate - self.written).min(usize::MAX as u64) as usize);
            self.file.write_all(&bytes[..count])?;
            self.written += count as u64;
            bytes = &bytes[count..];
        }
        Ok(())
    }
    /// Managed plain capture is acknowledged only after its bytes and any
    /// new segment entry are durable. Other capture modes retain their contract.
    pub fn append_durable(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.append(bytes)?;
        crate::fault_point!(CaptureSync)?;
        self.file.sync_all()?;
        self.dir.sync().map_err(io::Error::other)
    }
    /// Best effort: a failed write leaves a marker for readers. Failing to
    /// write the marker too loses only that report (K10).
    pub fn mark_truncated(&self) {
        if let Ok(name) = FileName::new(&truncated_name(self.sequence, self.stream)) {
            let _ = self
                .dir
                .open_file(&name, libc::O_WRONLY, Some(0o600), Trust::Private);
        }
    }
}

fn create(dir: &DirFd, sequence: i64, stream: Stream, base: u64) -> io::Result<std::fs::File> {
    let name = FileName::new(&segment_name(sequence, stream, base)).map_err(io::Error::other)?;
    Ok(dir
        .open_file(
            &name,
            libc::O_WRONLY | libc::O_EXCL,
            Some(0o600),
            Trust::Private,
        )
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::other("log missing"))?
        .0)
}

fn style_name(sequence: i64) -> io::Result<FileName> {
    FileName::new(&format!("{sequence}.stdout.styled-tap")).map_err(io::Error::other)
}
/// Optional presentation styles for an already read contiguous merged log.
/// A missing/expired tap always yields plain spans matching those exact bytes.
pub fn read_styles(
    root: &StateRoot,
    run: crate::id::RunId,
    sequence: i64,
    from: u64,
    bytes: &[u8],
) -> Vec<effigy_core::terminal_text::StyledSpan> {
    let fallback = || {
        vec![effigy_core::terminal_text::StyledSpan {
            text: String::from_utf8(bytes.to_vec()).unwrap_or_default(),
            style: Default::default(),
        }]
    };
    let Ok(Some(parent)) = root.read_family(Family::Runs) else {
        return fallback();
    };
    let Ok(Some(dir)) = parent.child(&run.to_string(), Trust::Private, false) else {
        return fallback();
    };
    let Ok(name) = style_name(sequence) else {
        return fallback();
    };
    super::styled_tap::read(&dir, &name, [sequence as u64, 0], from, bytes)
}

pub(crate) fn plain_name(sequence: i64) -> io::Result<FileName> {
    FileName::new(&format!("{sequence}.stdout.plain")).map_err(io::Error::other)
}
