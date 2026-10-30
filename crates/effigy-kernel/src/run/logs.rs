//! Pure reads of a process stream's log by absolute byte offset (§7.3
//! family 6). A reader resumes exactly where it stopped: segments carry their
//! start offset, so rotation never shifts a position. Bytes older than the
//! oldest kept segment are reported as expired, never skipped silently.
use super::*;
use crate::ctx::ObserveContext;
use crate::custody::capture::{segment_name, truncated_name};
use crate::custody::Stream;
use crate::fs::{DirFd, FileName, Trust};
use crate::observe::{AbsenceProof, Evidence, Observed};
use crate::root::Family;
use std::io::{Read, Seek, SeekFrom};

/// Segments one stream can have at most; anything beyond is untrusted.
const SEGMENTS_MAX: usize = 4096;

/// One read of a stream log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRead {
    /// Offset of the first returned byte. Greater than the requested offset
    /// when the bytes in between expired with rotation or retention.
    pub from: u64,
    pub bytes: Vec<u8>,
    /// The log's end offset when read: the next byte written lands here.
    pub end: u64,
    /// A write failed, so output after `end` was not logged.
    pub truncated: bool,
}

/// Reads at most `max` bytes of process `sequence`'s `stream` log from
/// `offset`. `Absent` when the run kept no log for it (never captured, or
/// removed by retention).
pub fn read_log(
    cx: &ObserveContext,
    root: &StateRoot,
    run: RunId,
    sequence: i64,
    stream: Stream,
    offset: u64,
    max: usize,
) -> Observed<LogRead> {
    let result = (|| -> Result<Option<LogRead>, crate::fs::FsError> {
        cx.check("log read")
            .map_err(|e| crate::fs::FsError::untrusted(root.path(), format!("{e:?}")))?;
        crate::fault_point!(LogRead).map_err(|e| {
            crate::fs::FsError::Io(crate::fs::IoError::new("log read", root.path(), &e))
        })?;
        let Some(runs) = root.read_family(Family::Runs)? else {
            return Ok(None);
        };
        let Some(dir) = runs.child(&run.to_string(), Trust::Private, false)? else {
            return Ok(None);
        };
        // A rotation can remove the segment between listing and opening;
        // the next listing sees its successor.
        for _ in 0..3 {
            if let Some(read) = read_segments(&dir, sequence, stream, offset, max)? {
                return Ok(read);
            }
        }
        Err(crate::fs::FsError::untrusted(
            dir.path(),
            "log segments kept rotating during the read",
        ))
    })();
    match result {
        Ok(Some(read)) => Observed::Verified(
            read,
            Evidence::new("kernel run log".into(), None).expect("fixed evidence"),
        ),
        Ok(None) => Observed::Absent(AbsenceProof::NoRow {
            table: "logs".into(),
        }),
        Err(error) => Observed::Unknown(crate::root::unknown(&error)),
    }
}

/// `Ok(None)` asks for a fresh listing; `Ok(Some(None))` is absence.
#[allow(clippy::option_option)]
fn read_segments(
    dir: &DirFd,
    sequence: i64,
    stream: Stream,
    offset: u64,
    max: usize,
) -> Result<Option<Option<LogRead>>, crate::fs::FsError> {
    let prefix = format!("{sequence}.{}.", stream.name());
    let mut bases: Vec<u64> = dir
        .list(SEGMENTS_MAX)?
        .iter()
        .filter_map(|name| name.to_str())
        .filter_map(|name| name.strip_prefix(&prefix)?.strip_suffix(".log"))
        .filter_map(|base| base.parse().ok())
        .collect();
    if bases.is_empty() {
        return Ok(Some(None));
    }
    bases.sort_unstable();
    let truncated = dir
        .entry(&FileName::new(&truncated_name(sequence, stream))?)?
        .is_some();
    let last = *bases.last().expect("non-empty");
    let Some(last_len) = segment_len(dir, sequence, stream, last)? else {
        return Ok(None);
    };
    let end = last + last_len;
    let from = offset.max(bases[0]).min(end);
    let mut bytes = Vec::new();
    let mut position = from;
    while bytes.len() < max && position < end {
        let index = bases.partition_point(|base| *base <= position) - 1;
        let base = bases[index];
        let name = FileName::new(&segment_name(sequence, stream, base))?;
        let Some((mut file, stat)) = dir.open_file(&name, libc::O_RDONLY, None, Trust::Private)?
        else {
            return Ok(None);
        };
        let within = position - base;
        let available = (stat.st_size as u64).saturating_sub(within);
        if available == 0 {
            break;
        }
        let take = available.min((max - bytes.len()) as u64);
        file.seek(SeekFrom::Start(within))
            .map_err(|e| crate::fs::FsError::io("log seek", dir.path(), &e))?;
        let before = bytes.len();
        file.take(take)
            .read_to_end(&mut bytes)
            .map_err(|e| crate::fs::FsError::io("log read", dir.path(), &e))?;
        let read = (bytes.len() - before) as u64;
        if read == 0 {
            break;
        }
        position += read;
    }
    let mut from = from;
    let format_name = crate::custody::capture::plain_name(sequence)
        .map_err(|error| crate::fs::FsError::io("plain format name", dir.path(), &error))?;
    if let Some((mut file, stat)) =
        dir.open_file(&format_name, libc::O_RDONLY, None, Trust::Private)?
    {
        let mut format = [0; 16];
        let count = file
            .read(&mut format)
            .map_err(|error| crate::fs::FsError::io("plain format read", dir.path(), &error))?;
        if stat.st_size != 16 || &format[..count] != b"effigy.plain.v1\n" {
            return Err(crate::fs::FsError::untrusted(
                dir.path(),
                "invalid plain capture format",
            ));
        }
        if from > offset {
            let skip = bytes
                .iter()
                .take_while(|byte| (0x80..=0xbf).contains(*byte))
                .count();
            bytes.drain(..skip);
            from += skip as u64;
        }
        match std::str::from_utf8(&bytes) {
            Ok(text) if text.chars().any(|ch| ch.is_control() && ch != '\n') => {
                return Err(crate::fs::FsError::untrusted(
                    dir.path(),
                    "control in plain capture",
                ))
            }
            Ok(_) => {}
            Err(error) if error.error_len().is_none() => bytes.truncate(error.valid_up_to()),
            Err(_) => {
                return Err(crate::fs::FsError::untrusted(
                    dir.path(),
                    "invalid plain capture text",
                ))
            }
        }
    }
    Ok(Some(Some(LogRead {
        from,
        bytes,
        end: end.max(position),
        truncated,
    })))
}

fn segment_len(
    dir: &DirFd,
    sequence: i64,
    stream: Stream,
    base: u64,
) -> Result<Option<u64>, crate::fs::FsError> {
    let name = FileName::new(&segment_name(sequence, stream, base))?;
    Ok(dir.entry(&name)?.map(|stat| stat.st_size as u64))
}
