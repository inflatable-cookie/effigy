//! The bounded terminal stream contract (§8.11).
//!
//! One `StreamBuffer` is one stream of one attachment: typed frames with a
//! monotone sequence, a flow-control window of unacknowledged frames and bytes,
//! a retained resume window and a cancel flag that stays serviceable under
//! backpressure. The two windows are separate: acknowledging reopens the flow
//! window, while retention — bounded in frames and bytes — is what a reconnect
//! resumes from. It holds no run authority — a caller that cannot push a frame
//! waits, and a caller that wants the run stopped asks the supervisor, which is
//! why backpressure never blocks cancellation.
//!
//! Terminal input is deliberately *not* buffered. It is delivered once to the
//! owning terminal and never replayed, so a reconnect cannot re-enter text an
//! operator already sent. A resize is latest-wins: only the newest geometry
//! matters.

use serde::{Deserialize, Serialize};

/// The largest decoded payload one frame may carry (32 KiB).
pub const MAX_FRAME_BYTES: usize = 32 * 1024;
/// At most this many data frames may be unacknowledged before the sender
/// backpressures.
pub const MAX_UNACKNOWLEDGED_FRAMES: usize = 16;
/// At most this much decoded data may be queued before the sender
/// backpressures.
pub const MAX_QUEUED_BYTES: usize = 512 * 1024;
/// The resume window: frames retained for a reconnecting reader.
pub const RETAINED_FRAMES: usize = 256;
/// The longest accepted stream name.
const STREAM_ID_MAX: usize = 64;

/// The frame kinds §8.11 defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Event,
    Stdout,
    Stderr,
    TerminalOutput,
    TerminalInput,
    Resize,
    Ack,
    End,
    Gap,
}
impl StreamKind {
    /// Data counts against the acknowledgement window. Control frames do not,
    /// so cancelling stays serviceable however full the window is.
    pub fn is_data(self) -> bool {
        matches!(
            self,
            Self::Event | Self::Stdout | Self::Stderr | Self::TerminalOutput
        )
    }
}

/// One stream's name: the run or terminal it belongs to plus a short label.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StreamId(String);
impl StreamId {
    pub fn parse(value: &str) -> Result<Self, StreamError> {
        if value.is_empty() || value.len() > STREAM_ID_MAX {
            return Err(StreamError::InvalidStream);
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
        {
            return Err(StreamError::InvalidStream);
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The next sequence a reader wants. Sequences start at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StreamCursor(u64);
impl StreamCursor {
    pub fn start() -> Self {
        Self(1)
    }
    pub fn at(sequence: u64) -> Self {
        Self(sequence.max(1))
    }
    pub fn sequence(self) -> u64 {
        self.0
    }
}

/// One framed stream message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    pub seq: u64,
    pub kind: StreamKind,
    pub payload: Vec<u8>,
}
/// The alias the dispatcher uses when it renders a frame.
pub type StreamFrame = Frame;

/// Why a push or an acknowledgement was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamError {
    /// The stream name is not a bounded stream name.
    InvalidStream,
    /// A resize needs positive rows and columns.
    InvalidResize,
    /// The cursor names a sequence this stream never issued.
    InvalidCursor,
    /// The stream is cancelled or closed.
    Closed,
    /// The frame is larger than a frame may be.
    TooLarge,
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStream => f.write_str("the stream name is not valid"),
            Self::InvalidResize => f.write_str("a resize needs positive rows and columns"),
            Self::InvalidCursor => f.write_str("the cursor is not a sequence this stream issued"),
            Self::Closed => f.write_str("the stream is closed"),
            Self::TooLarge => f.write_str("the frame is larger than a stream frame may be"),
        }
    }
}

/// The result of adding a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushOutcome {
    /// The frame is queued at this sequence.
    Queued(u64),
    /// Nothing was queued and nothing was dropped: the sender retries later,
    /// and an acknowledgement reopens the window. Cancellation and control stay
    /// serviceable meanwhile.
    Backpressured {
        unacknowledged: usize,
        queued_bytes: usize,
    },
    /// Terminal input is delivered once to the owner and never retained, so it
    /// has no sequence and no reader.
    NotBuffered,
}

/// The result of reading from a cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOutcome {
    /// Retained frames from the cursor onwards, in order.
    Frames(Vec<Frame>),
    /// The requested frames are no longer retained. `from` and `to` name the
    /// missing range and `resumed` is the first sequence the next read returns.
    /// A gap is never presented as complete output.
    Gap { from: u64, to: u64, resumed: u64 },
    /// Nothing new yet.
    Empty,
    /// The stream ended and every retained frame has been delivered.
    End { seq: u64, outcome: Option<String> },
    /// The cursor is not a sequence this stream issued.
    Refused(StreamError),
}

/// One stream's bounded buffer.
#[derive(Debug)]
pub struct StreamBuffer {
    id: StreamId,
    next_seq: u64,
    retained_from: u64,
    frames: std::collections::VecDeque<Frame>,
    /// Bytes currently retained in `frames`, so the resume window is bounded in
    /// bytes as well as in frames.
    retained_bytes: usize,
    /// The data frames a reader has not acknowledged yet: sequence and bytes.
    /// This is the flow-control window, not the resume window.
    unacknowledged: std::collections::VecDeque<(u64, usize)>,
    /// Bytes in unacknowledged data frames: the 512 KiB window.
    unacknowledged_bytes: usize,
    acked: u64,
    pending_resize: Option<(u16, u16)>,
    input_frames: u64,
    ended: Option<(u64, Option<String>)>,
    cancelled: bool,
    closed: bool,
}

impl StreamBuffer {
    pub fn new(id: StreamId) -> Self {
        Self {
            id,
            next_seq: 1,
            retained_from: 1,
            frames: std::collections::VecDeque::new(),
            retained_bytes: 0,
            unacknowledged: std::collections::VecDeque::new(),
            unacknowledged_bytes: 0,
            acked: 0,
            pending_resize: None,
            input_frames: 0,
            ended: None,
            cancelled: false,
            closed: false,
        }
    }
    pub fn id(&self) -> &StreamId {
        &self.id
    }
    pub fn cursor(&self) -> StreamCursor {
        StreamCursor::at(self.next_seq)
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled
    }
    pub fn closed(&self) -> bool {
        self.closed
    }
    pub fn unacknowledged(&self) -> usize {
        self.unacknowledged.len()
    }
    /// The bytes in unacknowledged data frames: what the 512 KiB window counts.
    /// An acknowledgement releases them, so a slow reader throttles the sender
    /// without stalling the stream once it catches up.
    pub fn queued_bytes(&self) -> usize {
        self.unacknowledged_bytes
    }
    pub fn input_frames(&self) -> u64 {
        self.input_frames
    }
    pub fn pending_resize(&self) -> Option<(u16, u16)> {
        self.pending_resize
    }
    /// The retained window's first sequence, for diagnostics and tests.
    pub fn retained_from(&self) -> u64 {
        self.retained_from
    }

    /// Queues one data or control frame. A data frame that would exceed the
    /// acknowledgement window or the 512 KiB unacknowledged-byte window is not
    /// queued and not dropped: the caller retries, and this call never blocks.
    /// Both windows reopen when the reader acknowledges, so backpressure is a
    /// delay, never a permanent stall.
    pub fn push(&mut self, kind: StreamKind, payload: Vec<u8>) -> Result<PushOutcome, StreamError> {
        if kind == StreamKind::TerminalInput {
            // Terminal input is delivered once to the owner and never retained.
            return Ok(PushOutcome::NotBuffered);
        }
        if payload.len() > MAX_FRAME_BYTES {
            return Err(StreamError::TooLarge);
        }
        if self.cancelled || self.closed {
            return Err(StreamError::Closed);
        }
        if self.ended.is_some() && kind != StreamKind::End {
            return Err(StreamError::Closed);
        }
        if kind.is_data()
            && (self.unacknowledged.len() >= MAX_UNACKNOWLEDGED_FRAMES
                || self.unacknowledged_bytes.saturating_add(payload.len()) > MAX_QUEUED_BYTES)
        {
            return Ok(PushOutcome::Backpressured {
                unacknowledged: self.unacknowledged.len(),
                queued_bytes: self.unacknowledged_bytes,
            });
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.retained_bytes += payload.len();
        if kind.is_data() {
            self.unacknowledged.push_back((seq, payload.len()));
            self.unacknowledged_bytes += payload.len();
        }
        self.frames.push_back(Frame { seq, kind, payload });
        self.prune();
        Ok(PushOutcome::Queued(seq))
    }

    /// Delivers terminal input once. It is never retained, so no cursor can
    /// replay it; a reconnect starts at output only.
    pub fn input(&mut self, bytes: Vec<u8>) -> Result<u64, StreamError> {
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(StreamError::TooLarge);
        }
        if self.cancelled || self.closed {
            return Err(StreamError::Closed);
        }
        self.input_frames += 1;
        Ok(self.input_frames)
    }

    /// Records the newest geometry. A later resize replaces an unread one:
    /// latest-wins, never a queue of stale geometries.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<(), StreamError> {
        if rows == 0 || cols == 0 {
            return Err(StreamError::InvalidResize);
        }
        if self.cancelled || self.closed {
            return Err(StreamError::Closed);
        }
        self.pending_resize = Some((rows, cols));
        Ok(())
    }

    /// Acknowledges everything consumed up to `seq`. Acknowledgements are
    /// monotone: an older one changes nothing.
    pub fn ack(&mut self, seq: u64) -> Result<(), StreamError> {
        if seq > self.next_seq {
            return Err(StreamError::InvalidCursor);
        }
        if seq > self.acked {
            self.acked = seq;
            while let Some((front, bytes)) = self.unacknowledged.front().copied() {
                if front <= seq {
                    self.unacknowledged.pop_front();
                    self.unacknowledged_bytes = self.unacknowledged_bytes.saturating_sub(bytes);
                } else {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Retained frames from `cursor`. Frames before the retained window are a
    /// gap; the next read resumes at the oldest retained frame.
    pub fn read(&self, cursor: StreamCursor) -> ReadOutcome {
        let seq = cursor.sequence();
        if seq > self.next_seq {
            return ReadOutcome::Refused(StreamError::InvalidCursor);
        }
        if seq < self.retained_from {
            return ReadOutcome::Gap {
                from: seq,
                to: self.retained_from,
                resumed: self.retained_from,
            };
        }
        if seq == self.next_seq {
            return match &self.ended {
                Some((end, outcome)) => ReadOutcome::End {
                    seq: *end,
                    outcome: outcome.clone(),
                },
                None => ReadOutcome::Empty,
            };
        }
        let frames: Vec<Frame> = self
            .frames
            .iter()
            .filter(|frame| frame.seq >= seq)
            .cloned()
            .collect();
        if frames.is_empty() {
            ReadOutcome::Empty
        } else {
            ReadOutcome::Frames(frames)
        }
    }

    /// Ends the stream at its next sequence with the outcome, when known.
    pub fn end(&mut self, outcome: Option<String>) -> Result<u64, StreamError> {
        if self.cancelled || self.closed {
            return Err(StreamError::Closed);
        }
        if let Some((seq, _)) = self.ended {
            return Ok(seq);
        }
        let seq = self.next_seq;
        let payload = outcome.clone().unwrap_or_default().into_bytes();
        self.retained_bytes += payload.len();
        self.frames.push_back(Frame {
            seq,
            kind: StreamKind::End,
            payload,
        });
        self.next_seq += 1;
        self.ended = Some((seq, outcome));
        self.closed = true;
        self.prune();
        Ok(seq)
    }

    /// Drops the oldest retained frames once the resume window is full. The
    /// window is bounded in frames and in bytes, so one stream cannot hold
    /// unbounded memory; a reader whose cursor fell out of it is told the range
    /// is a gap rather than receiving output that claims to be complete.
    fn prune(&mut self) {
        while self.frames.len() > RETAINED_FRAMES
            || (self.retained_bytes > MAX_QUEUED_BYTES && self.frames.len() > 1)
        {
            let Some(dropped) = self.frames.pop_front() else {
                break;
            };
            self.retained_bytes = self.retained_bytes.saturating_sub(dropped.payload.len());
            self.retained_from = dropped.seq + 1;
        }
    }

    /// Cancels the stream. Control stays serviceable under backpressure because
    /// this takes no window and never waits for a reader.
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.closed = true;
    }
}
