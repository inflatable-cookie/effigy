//! The one length-prefixed frame codec the supervisor helpers speak.
//!
//! A frame is a four-byte big-endian body length followed by that body. The
//! helper pipes, the event pipe and the owning-client stream all use this
//! codec, so this module alone owns the wire bound, a short write, a
//! `WouldBlock`/`Interrupted` retry, and a buffered peer that has delivered
//! part of a frame.
use crate::ctx::OperationContext;
use crate::id::ResolvedInvocation;
use serde::Serialize;
use std::io::{self, Read, Write};
use std::time::Duration;

/// The largest body the wire admits, in both directions.
pub(super) const MAX_BODY: usize = ResolvedInvocation::MAX_WIRE_BYTES;
const PREFIX: usize = 4;
const RETRY: Duration = Duration::from_millis(2);

/// A frame the codec will not put on the wire or read from it.
#[derive(Debug)]
pub(super) enum FrameError {
    /// The body is outside [`MAX_BODY`]. This is Effigy's own encoding, never a
    /// peer's answer, so the caller reports it as a broken invariant.
    TooLarge(usize),
    /// The body could not be serialized from its typed value.
    Encode(serde_json::Error),
    /// The peer's frame could not be read or its pipe failed.
    Io(io::Error),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge(size) => write!(
                f,
                "frame body of {size} bytes exceeds the {MAX_BODY} byte wire bound"
            ),
            Self::Encode(error) => write!(f, "frame body is not encodable ({error})"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<io::Error> for FrameError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Serialize `value` into one bounded, length-prefixed frame.
pub(super) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, FrameError> {
    let body = serde_json::to_vec(value).map_err(FrameError::Encode)?;
    framed_body(&body)
}

/// Length-prefix an already-encoded body, refusing one outside the bound.
pub(super) fn framed_body(body: &[u8]) -> Result<Vec<u8>, FrameError> {
    if body.len() > MAX_BODY {
        return Err(FrameError::TooLarge(body.len()));
    }
    let mut frame = Vec::with_capacity(body.len() + PREFIX);
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(body);
    Ok(frame)
}

/// Write one whole frame, retrying a short write or a nonblocking pipe.
pub(super) fn write(
    writer: &mut impl Write,
    frame: &[u8],
    op: &OperationContext,
) -> Result<(), FrameError> {
    let mut offset = 0;
    while offset < frame.len() {
        // The interrupt's own sentence already names the phase and the cause;
        // a person reads it through `Display`, never a `Debug` rendering.
        op.check("the frame write")
            .map_err(|error| io::Error::other(error.to_string()))?;
        match writer.write(&frame[offset..]) {
            Ok(0) => {
                return Err(io::Error::new(io::ErrorKind::WriteZero, "frame pipe closed").into())
            }
            Ok(count) => offset += count,
            Err(error) if retryable(&error) => std::thread::sleep(RETRY),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Read exactly `bytes`, retrying a nonblocking pipe.
pub(super) fn read_exact(
    reader: &mut impl Read,
    bytes: &mut [u8],
    op: &OperationContext,
) -> Result<(), FrameError> {
    let mut offset = 0;
    while offset < bytes.len() {
        // The interrupt's own sentence already names the phase and the cause;
        // a person reads it through `Display`, never a `Debug` rendering.
        op.check("the frame read")
            .map_err(|error| io::Error::other(error.to_string()))?;
        match reader.read(&mut bytes[offset..]) {
            Ok(0) => {
                return Err(
                    io::Error::new(io::ErrorKind::UnexpectedEof, "frame pipe closed").into(),
                )
            }
            Ok(count) => offset += count,
            Err(error) if retryable(&error) => std::thread::sleep(RETRY),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Read one frame body, refusing one outside the bound before allocating it.
pub(super) fn read(reader: &mut impl Read, op: &OperationContext) -> Result<Vec<u8>, FrameError> {
    let mut prefix = [0u8; PREFIX];
    read_exact(reader, &mut prefix, op)?;
    read_body(reader, prefix, op)
}

/// Read one frame body whose first prefix byte an open wait already consumed.
pub(super) fn read_after_first(
    reader: &mut impl Read,
    first: u8,
    op: &OperationContext,
) -> Result<Vec<u8>, FrameError> {
    let mut prefix = [0u8; PREFIX];
    prefix[0] = first;
    read_exact(reader, &mut prefix[1..], op)?;
    read_body(reader, prefix, op)
}

fn read_body(
    reader: &mut impl Read,
    prefix: [u8; PREFIX],
    op: &OperationContext,
) -> Result<Vec<u8>, FrameError> {
    let size = u32::from_be_bytes(prefix) as usize;
    if size > MAX_BODY {
        return Err(FrameError::TooLarge(size));
    }
    let mut body = vec![0u8; size];
    read_exact(reader, &mut body, op)?;
    Ok(body)
}

/// Take one whole frame from a buffer a peer fills in arbitrary pieces.
///
/// `Ok(None)` means the buffer holds no complete frame yet. A prefix that
/// announces an impossible body is refused, so a peer cannot make the buffer
/// wait for bytes the wire bound forbids.
pub(super) fn take(buffer: &mut Vec<u8>) -> Result<Option<Vec<u8>>, FrameError> {
    if buffer.len() < PREFIX {
        return Ok(None);
    }
    let size = u32::from_be_bytes(buffer[..PREFIX].try_into().map_err(io::Error::other)?) as usize;
    if size > MAX_BODY {
        return Err(FrameError::TooLarge(size));
    }
    if buffer.len() < size + PREFIX {
        return Ok(None);
    }
    let body = buffer[PREFIX..size + PREFIX].to_vec();
    buffer.drain(..size + PREFIX);
    Ok(Some(body))
}

fn retryable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::CancelToken;

    fn op() -> OperationContext {
        super::super::context(CancelToken::default(), Duration::from_secs(5))
    }

    /// A pipe that accepts two bytes per write, then reports nonblocking.
    struct Trickle {
        written: Vec<u8>,
        blocked: bool,
    }

    impl Write for Trickle {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.blocked {
                self.blocked = false;
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            self.blocked = true;
            let take = bytes.len().min(2);
            self.written.extend_from_slice(&bytes[..take]);
            Ok(take)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A pipe that delivers at most one byte per read.
    struct Drip {
        bytes: Vec<u8>,
    }

    impl Read for Drip {
        fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
            let take = into.len().min(1).min(self.bytes.len());
            into[..take].copy_from_slice(&self.bytes[..take]);
            self.bytes.drain(..take);
            Ok(take)
        }
    }

    #[test]
    fn a_short_nonblocking_write_delivers_the_whole_frame() {
        let mut sink = Trickle {
            written: Vec::new(),
            blocked: false,
        };
        let frame = framed_body(b"payload").expect("bounded body");
        write(&mut sink, &frame, &op()).expect("write completes");
        assert_eq!(sink.written, frame);
    }

    #[test]
    fn a_dripping_peer_delivers_the_whole_frame() {
        let frame = framed_body(b"{\"a\":1}").expect("bounded body");
        let mut source = Drip {
            bytes: frame.clone(),
        };
        assert_eq!(read(&mut source, &op()).expect("read"), b"{\"a\":1}");
    }

    #[test]
    fn a_frame_whose_prefix_announces_an_impossible_body_is_refused() {
        let mut source = Drip {
            bytes: ((MAX_BODY as u32 + 1).to_be_bytes()).to_vec(),
        };
        assert!(matches!(
            read(&mut source, &op()),
            Err(FrameError::TooLarge(_))
        ));
        assert!(matches!(
            framed_body(&vec![0u8; MAX_BODY + 1]),
            Err(FrameError::TooLarge(_))
        ));
    }

    #[test]
    fn a_buffered_peer_waits_for_a_whole_frame() {
        let frame = framed_body(b"abc").expect("bounded body");
        let mut buffer = frame[..frame.len() - 1].to_vec();
        assert!(take(&mut buffer).expect("partial").is_none());
        buffer.push(*frame.last().expect("body"));
        assert_eq!(
            take(&mut buffer).expect("whole").as_deref(),
            Some(&b"abc"[..])
        );
        assert!(take(&mut buffer).expect("empty").is_none());
    }

    #[test]
    fn a_buffered_peer_cannot_wait_for_a_forbidden_body() {
        let mut buffer = (MAX_BODY as u32 + 1).to_be_bytes().to_vec();
        assert!(matches!(take(&mut buffer), Err(FrameError::TooLarge(_))));
    }

    #[test]
    fn a_closed_pipe_is_an_end_of_stream_not_an_empty_frame() {
        let mut closed = Drip { bytes: Vec::new() };
        assert!(matches!(
            read(&mut closed, &op()),
            Err(FrameError::Io(error)) if error.kind() == io::ErrorKind::UnexpectedEof
        ));
    }
}
