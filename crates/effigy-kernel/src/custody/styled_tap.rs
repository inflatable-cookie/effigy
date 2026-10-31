//! Bounded, best-effort presentation data published only after plain append.
//! Canonical bytes remain authoritative; readers use styles only for an exact
//! identity/offset/text match. No terminal parsing, replay or redaction lives here.
use crate::fs::{DirFd, FileName, Trust};
use effigy_core::terminal_text::{Style, StyledChunk, StyledSpan};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{self, Read};
const MAX_WIRE: usize = 1024 * 1024;
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Window {
    version: u8,
    identity: [u64; 2],
    chunks: VecDeque<StyledChunk>,
}
#[derive(Debug)]
pub(super) struct Tap {
    dir: DirFd,
    name: FileName,
    window: Window,
    limit: usize,
    bytes: usize,
}
impl Tap {
    pub(super) fn new(dir: DirFd, name: FileName, identity: [u64; 2], limit: usize) -> Self {
        Self {
            dir,
            name,
            window: Window {
                version: 1,
                identity,
                chunks: VecDeque::new(),
            },
            limit: limit.min(64 * 1024),
            bytes: 0,
        }
    }
    pub(super) fn append(&mut self, chunk: StyledChunk) -> io::Result<()> {
        self.bytes += chunk
            .spans
            .iter()
            .map(|span| span.text.len())
            .sum::<usize>();
        self.window.chunks.push_back(chunk);
        while self.bytes > self.limit || self.window.chunks.len() > 256 {
            if let Some(old) = self.window.chunks.pop_front() {
                self.bytes -= old.spans.iter().map(|span| span.text.len()).sum::<usize>();
            }
        }
        let mut wire = Vec::new();
        // Bound serialized metadata too: SGR changes may make spans much larger
        // than their text. Dropping old styles never changes the plain log.
        while !self.window.chunks.is_empty() {
            wire = serde_json::to_vec(&self.window).map_err(io::Error::other)?;
            if wire.len() <= MAX_WIRE {
                break;
            }
            if let Some(old) = self.window.chunks.pop_front() {
                self.bytes -= old.spans.iter().map(|span| span.text.len()).sum::<usize>();
            }
        }
        if wire.is_empty() || wire.len() > MAX_WIRE {
            wire = serde_json::to_vec(&self.window).map_err(io::Error::other)?;
        }
        let expected = if let Some((file, stat)) = self
            .dir
            .open_file(&self.name, libc::O_RDONLY, None, Trust::Private)
            .map_err(io::Error::other)?
        {
            if stat.st_size < 0 || stat.st_size as usize > MAX_WIRE || stat.st_nlink != 1 {
                return Err(io::Error::other(
                    "untrusted styled tap; operator data retained",
                ));
            }
            let previous: Window = serde_json::from_reader(file.take((MAX_WIRE + 1) as u64))
                .map_err(|_| io::Error::other("unrecognized styled tap; operator data retained"))?;
            if previous.version != 1 {
                return Err(io::Error::other(
                    "unknown styled tap version; operator data retained",
                ));
            }
            Some(super::FileIdentity::of(&stat))
        } else {
            None
        };
        if wire.len() > MAX_WIRE {
            return Err(io::Error::other("styled tap exceeds its wire bound"));
        }
        super::merged::atomic::publish(&self.dir, &self.name, &self.window, "styles", expected)
            .map(|_| ())
    }
}
/// Apply style metadata only where its text exactly matches this contiguous
/// canonical read. Stale, absent or oversized tap data yields ordinary text.
pub(super) fn read(
    dir: &DirFd,
    name: &FileName,
    identity: [u64; 2],
    from: u64,
    bytes: &[u8],
) -> Vec<StyledSpan> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Vec::new();
    };
    let fallback = || {
        vec![StyledSpan {
            text: text.to_owned(),
            style: Style::default(),
        }]
    };
    let window = (|| {
        let (file, stat) = dir
            .open_file(name, libc::O_RDONLY, None, Trust::Private)
            .ok()??;
        if stat.st_size < 0 || stat.st_size as usize > MAX_WIRE || stat.st_nlink != 1 {
            return None;
        }
        let mut wire = Vec::new();
        file.take((MAX_WIRE + 1) as u64)
            .read_to_end(&mut wire)
            .ok()?;
        if wire.is_empty() || wire.len() > MAX_WIRE {
            return None;
        }
        serde_json::from_slice::<Window>(&wire).ok()
    })();
    let Some(window) = window.filter(|window| window.version == 1 && window.identity == identity)
    else {
        return fallback();
    };
    let mut spans = Vec::new();
    let mut cursor = 0usize;
    for chunk in window.chunks {
        let mut position = chunk.offset;
        for span in chunk.spans {
            let end = position.saturating_add(span.text.len() as u64);
            let start = position.max(from);
            let stop = end.min(from.saturating_add(bytes.len() as u64));
            if start < stop {
                let at = (start - from) as usize;
                let until = (stop - from) as usize;
                let local = (start - position) as usize;
                let local_end = (stop - position) as usize;
                if at < cursor
                    || !text.is_char_boundary(at)
                    || !text.is_char_boundary(until)
                    || !span.text.is_char_boundary(local)
                    || !span.text.is_char_boundary(local_end)
                    || text[at..until] != span.text[local..local_end]
                {
                    return fallback();
                }
                if cursor < at {
                    spans.push(StyledSpan {
                        text: text[cursor..at].to_owned(),
                        style: Style::default(),
                    });
                }
                spans.push(StyledSpan {
                    text: text[at..until].to_owned(),
                    style: span.style,
                });
                cursor = until;
            }
            position = end;
        }
    }
    if cursor < text.len() {
        spans.push(StyledSpan {
            text: text[cursor..].to_owned(),
            style: Style::default(),
        });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn styles_require_identity_offset_and_exact_canonical_text() {
        let home = effigy_test_support::short_tempdir("st-").unwrap();
        let dir = DirFd::walk(home.path(), Trust::Operator).unwrap();
        let name = FileName::new("typed-tap").unwrap();
        let mut tap = Tap::new(dir.clone(), name.clone(), [7, 8], 32);
        let green =
            effigy_core::terminal_text::style::apply_sgr(Style::default(), "32", Style::default());
        tap.append(StyledChunk {
            offset: 5,
            spans: vec![StyledSpan {
                text: "safe é\n".into(),
                style: green,
            }],
        })
        .unwrap();
        let exact = read(&dir, &name, [7, 8], 5, "safe é\n".as_bytes());
        assert_eq!(exact[0].style, green);
        let mismatched = read(&dir, &name, [7, 9], 5, "safe é\n".as_bytes());
        assert!(mismatched.iter().all(|span| span.style == Style::default()));
        let stale = read(&dir, &name, [7, 8], 5, b"other text\n");
        assert_eq!(stale[0].text, "other text\n");
        assert_eq!(stale[0].style, Style::default());
        assert_eq!(
            read(&dir, &name, [7, 8], 10, "é".as_bytes())[0].style,
            green
        );
        for n in 0..100 {
            tap.append(StyledChunk {
                offset: 20 + n * 5,
                spans: vec![StyledSpan {
                    text: "next\n".into(),
                    style: green,
                }],
            })
            .unwrap();
        }
        assert!(tap.bytes <= 32);
        assert!(tap.window.chunks.len() <= 256);
        assert_eq!(
            read(&dir, &name, [7, 8], 5, b"safe\n")[0].style,
            Style::default()
        );
    }
    #[test]
    fn an_unrecognized_style_file_is_operator_data_and_is_retained() {
        let home = effigy_test_support::short_tempdir("so-").unwrap();
        let dir = DirFd::walk(home.path(), Trust::Operator).unwrap();
        let name = FileName::new("styled-tap").unwrap();
        let (mut file, _) = dir
            .open_file(
                &name,
                libc::O_WRONLY | libc::O_EXCL,
                Some(0o600),
                Trust::Private,
            )
            .unwrap()
            .unwrap();
        file.write_all(b"operator bytes").unwrap();
        let mut tap = Tap::new(dir, name, [7, 8], 4096);
        assert!(tap
            .append(StyledChunk {
                offset: 0,
                spans: vec![StyledSpan {
                    text: "safe\n".into(),
                    style: Style::default()
                }]
            })
            .is_err());
        assert_eq!(
            std::fs::read(home.path().join("styled-tap")).unwrap(),
            b"operator bytes"
        );
    }
}
