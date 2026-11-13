use std::io::{self, Write};
use std::sync::{Mutex, OnceLock, PoisonError};

pub const OUTPUT_CAP_ENV: &str = "EFFIGY_OUTPUT_CAP_BYTES";
const MARKER: &str = "effigy: output cap {cap} bytes reached; remaining Effigy output discarded (output_truncated)\n";
const FOOTER: &str = "\neffigy: output complete (payload {written} bytes; cap {cap} bytes)\n";

#[derive(Clone, Copy)]
pub enum Stream {
    Stdout,
    Stderr,
}

struct State {
    cap: Option<usize>,
    written: usize,
    truncated: bool,
    incomplete: bool,
    footer_enabled: bool,
    finished: bool,
}
impl State {
    fn new(cap: Option<usize>) -> Self {
        Self {
            cap,
            written: 0,
            truncated: false,
            incomplete: false,
            footer_enabled: true,
            finished: false,
        }
    }
    fn write(&mut self, stream: Stream, bytes: &[u8]) -> io::Result<()> {
        self.write_with(stream, bytes, raw_write)
    }

    fn write_with(
        &mut self,
        stream: Stream,
        bytes: &[u8],
        mut emit: impl FnMut(Stream, &[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        if bytes.is_empty() || self.truncated || self.finished {
            return Ok(());
        }
        let Some(cap) = self.cap else {
            let result = emit(stream, bytes);
            if result.is_err() {
                self.incomplete = true;
            }
            return result;
        };
        let marker = marker(cap);
        let available = cap
            .saturating_sub(marker.len())
            .saturating_sub(footer_reserve())
            .saturating_sub(self.written);
        let accepted = complete_lines(bytes, available);
        self.written += accepted;
        if accepted > 0 {
            if let Err(error) = emit(stream, &bytes[..accepted]) {
                // An emitter may have written a prefix before returning an
                // error. Charge the whole attempt, reserve and attempt the
                // marker once, then latch so ignored errors cannot retry it.
                self.truncated = true;
                self.incomplete = true;
                self.written += marker.len();
                if emit(Stream::Stderr, &marker).is_err() {
                    self.incomplete = true;
                }
                return Err(error);
            }
        }
        if accepted != bytes.len() {
            self.written += marker.len();
            self.truncated = true;
            if let Err(error) = emit(Stream::Stderr, &marker) {
                self.incomplete = true;
                return Err(error);
            }
        }
        Ok(())
    }

    fn finish_with(
        &mut self,
        mut emit: impl FnMut(Stream, &[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        let Some(cap) = self.cap else {
            self.finished = true;
            return Ok(());
        };
        if self.finished || !self.footer_enabled || self.truncated || self.incomplete {
            self.finished = true;
            return Ok(());
        }
        let footer = footer(self.written, cap);
        self.finished = true;
        if let Err(error) = emit(Stream::Stderr, &footer) {
            self.incomplete = true;
            return Err(error);
        }
        self.written += footer.len();
        Ok(())
    }
}

fn marker(cap: usize) -> Vec<u8> {
    MARKER.replace("{cap}", &cap.to_string()).into_bytes()
}
fn footer(written: usize, cap: usize) -> Vec<u8> {
    FOOTER
        .replace("{written}", &written.to_string())
        .replace("{cap}", &cap.to_string())
        .into_bytes()
}
fn footer_reserve() -> usize {
    footer(usize::MAX, usize::MAX).len()
}
fn complete_lines(bytes: &[u8], limit: usize) -> usize {
    if bytes.len() <= limit {
        return bytes.len();
    }
    bytes[..limit]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |i| i + 1)
}
fn raw_write(stream: Stream, bytes: &[u8]) -> io::Result<()> {
    match stream {
        Stream::Stdout => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(bytes)?;
            stdout.flush()
        }
        Stream::Stderr => {
            let mut stderr = io::stderr().lock();
            stderr.write_all(bytes)?;
            stderr.flush()
        }
    }
}
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::new(None)))
}

pub fn marker_len(cap: usize) -> usize {
    marker(cap).len()
}
pub fn minimum_cap() -> usize {
    (1..)
        .find(|cap| *cap >= marker_len(*cap) + footer_reserve())
        .unwrap_or(usize::MAX)
}
fn configure_env_once() -> Result<(), String> {
    let cap = match std::env::var(OUTPUT_CAP_ENV) {
        Ok(value) => Some(
            value
                .parse::<usize>()
                .map_err(|_| format!("{OUTPUT_CAP_ENV} must be a positive byte count"))?,
        ),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => return Err(format!("could not read {OUTPUT_CAP_ENV}: {error}")),
    };
    validate_cap(cap)?;
    configure(cap);
    if cap.is_some() {
        install_capped_panic_hook();
    }
    Ok(())
}

fn install_capped_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let thread_name = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_owned();
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_owned());
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown location".to_owned());
        let message = format!("thread '{thread_name}' panicked at {location}:\n{payload}\n");
        let _ = write(Stream::Stderr, message.as_bytes());
    }));
}
pub fn validate_cap(cap: Option<usize>) -> Result<(), String> {
    if let Some(cap) = cap {
        if cap == 0 {
            return Err(format!("{OUTPUT_CAP_ENV} must be greater than zero"));
        }
        if cap < minimum_cap() {
            return Err(format!("{OUTPUT_CAP_ENV}={cap} is too small for the output marker and completeness footer ({} bytes minimum)", minimum_cap()));
        }
    }
    Ok(())
}
static ENV_CONFIGURED: OnceLock<Result<(), String>> = OnceLock::new();
pub fn configure_from_env() -> Result<(), String> {
    ENV_CONFIGURED.get_or_init(configure_env_once).clone()
}
pub fn configure(cap: Option<usize>) {
    *state().lock().unwrap_or_else(PoisonError::into_inner) = State::new(cap);
}
pub fn write(stream: Stream, bytes: &[u8]) -> io::Result<()> {
    state()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .write(stream, bytes)
}
pub fn finish() -> io::Result<()> {
    state()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .finish_with(raw_write)
}
pub fn set_footer_enabled(enabled: bool) {
    state()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .footer_enabled = enabled;
}
pub fn truncated() -> bool {
    state()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .truncated
}

pub struct ProcessStdout;
pub struct ProcessStderr;
impl Write for ProcessStdout {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        write(Stream::Stdout, bytes)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush().inspect_err(|_| {
            state()
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .incomplete = true;
        })
    }
}
impl Write for ProcessStderr {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        write(Stream::Stderr, bytes)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush().inspect_err(|_| {
            state()
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .incomplete = true;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_counts_one_marker_and_drops_later_output() {
        let cap = minimum_cap() + 40;
        let mut state = State::new(Some(cap));
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut emit = |stream, bytes: &[u8]| {
            match stream {
                Stream::Stdout => out.extend_from_slice(bytes),
                Stream::Stderr => err.extend_from_slice(bytes),
            }
            Ok(())
        };
        state
            .write_with(
                Stream::Stdout,
                b"first line\nsecond line is too long to fit\n",
                &mut emit,
            )
            .unwrap();
        state
            .write_with(Stream::Stdout, b"later\n", &mut emit)
            .unwrap();
        assert_eq!(out, b"first line\n");
        assert_eq!(err, marker(cap));
        assert!(out.len() + err.len() <= cap);
        assert!(state.truncated);
    }

    #[test]
    fn uncapped_output_is_unchanged() {
        let mut state = State::new(None);
        let mut output = Vec::new();
        state
            .write_with(Stream::Stderr, b"same bytes\0\n", |_, bytes| {
                output.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        assert_eq!(output, b"same bytes\0\n");
    }

    #[test]
    fn stdout_and_hook_stderr_share_the_budget() {
        let mut state = State::new(Some(minimum_cap() + 100));
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut emit = |stream, bytes: &[u8]| {
            match stream {
                Stream::Stdout => out.extend_from_slice(bytes),
                Stream::Stderr => err.extend_from_slice(bytes),
            }
            Ok(())
        };
        state
            .write_with(Stream::Stdout, b"stdout\n", &mut emit)
            .unwrap();
        state
            .write_with(Stream::Stderr, b"hook stderr\n", &mut emit)
            .unwrap();
        assert_eq!(out, b"stdout\n");
        assert_eq!(err, b"hook stderr\n");
        assert_eq!(state.written, out.len() + err.len());
    }

    #[test]
    fn an_early_stderr_notice_consumes_the_same_budget() {
        let cap = minimum_cap() + 20;
        let mut state = State::new(Some(cap));
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut emit = |stream, bytes: &[u8]| {
            match stream {
                Stream::Stdout => out.extend_from_slice(bytes),
                Stream::Stderr => err.extend_from_slice(bytes),
            }
            Ok(())
        };
        state
            .write_with(Stream::Stderr, b"early warning\n", &mut emit)
            .unwrap();
        state
            .write_with(Stream::Stdout, &[b'x'; 64], &mut emit)
            .unwrap();
        assert_eq!(out, b"");
        assert!(err.starts_with(b"early warning\n"));
        assert!(err.ends_with(&marker(cap)));
        assert!(out.len() + err.len() <= cap);
    }

    #[test]
    fn marker_length_is_the_minimum_accepted_cap() {
        let cap = minimum_cap();
        assert!(validate_cap(Some(cap)).is_ok());
        assert!(validate_cap(Some(cap - 1)).is_err());
        assert!(validate_cap(None).is_ok());
    }

    #[test]
    fn a_partial_payload_write_error_charges_and_latches_before_retry() {
        let cap = minimum_cap() + 20;
        let mut state = State::new(Some(cap));
        let mut calls = 0;
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut emit = |stream, bytes: &[u8]| {
            calls += 1;
            match (calls, stream) {
                (1, Stream::Stdout) => {
                    out.extend_from_slice(&bytes[..4]);
                    Err(io::Error::other("payload write failed after a prefix"))
                }
                (2, Stream::Stderr) => {
                    err.extend_from_slice(bytes);
                    Ok(())
                }
                _ => panic!("unexpected emit attempt {calls}"),
            }
        };

        assert!(state
            .write_with(
                Stream::Stdout,
                b"first line\nsecond line is too long to fit\n",
                &mut emit,
            )
            .is_err());
        state
            .write_with(Stream::Stdout, b"later\n", &mut emit)
            .unwrap();

        assert_eq!(calls, 2);
        assert_eq!(out, b"firs");
        assert_eq!(err, marker(cap));
        assert_eq!(state.written, b"first line\n".len() + marker(cap).len());
        assert!(state.written <= cap);
        assert!(state.truncated);
    }

    #[test]
    fn a_partial_marker_write_error_is_not_retried() {
        let cap = minimum_cap() + 20;
        let mut state = State::new(Some(cap));
        let mut calls = 0;
        let mut output = Vec::new();
        let mut emit = |_, bytes: &[u8]| {
            calls += 1;
            output.extend_from_slice(&bytes[..5]);
            Err(io::Error::other("marker write failed after a prefix"))
        };

        assert!(state
            .write_with(Stream::Stdout, &[b'x'; 300], &mut emit)
            .is_err());
        state
            .write_with(Stream::Stdout, b"later\n", &mut emit)
            .unwrap();

        assert_eq!(calls, 1);
        assert_eq!(output, marker(cap)[..5]);
        assert_eq!(state.written, marker(cap).len());
        assert!(state.written <= cap);
        assert!(state.truncated);
    }

    #[test]
    fn clean_capped_run_writes_a_reserved_completeness_footer() {
        let cap = minimum_cap() + 100;
        let mut state = State::new(Some(cap));
        let mut output = Vec::new();
        state
            .write_with(Stream::Stdout, b"task output\n", |_, bytes| {
                output.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        state
            .finish_with(|stream, bytes| {
                assert!(matches!(stream, Stream::Stderr));
                output.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        assert!(output.ends_with(footer(b"task output\n".len(), cap).as_slice()));
        assert!(output.len() <= cap);
    }

    #[test]
    fn overflow_marker_suppresses_the_completeness_footer() {
        let cap = minimum_cap() + 20;
        let mut state = State::new(Some(cap));
        let mut output = Vec::new();
        state
            .write_with(Stream::Stdout, &[b'x'; 200], |_, bytes| {
                output.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        state
            .finish_with(|_, bytes| {
                output.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        assert!(output.ends_with(&marker(cap)));
        assert!(!output
            .windows(b"output complete".len())
            .any(|w| w == b"output complete"));
        assert!(output.len() <= cap);
    }

    #[test]
    fn failed_admission_output_suppresses_the_footer() {
        let cap = minimum_cap() + 100;
        let mut state = State::new(Some(cap));
        let mut output = Vec::new();
        let result = state.write_with(Stream::Stderr, b"admission progress\n", |_, bytes| {
            output.extend_from_slice(&bytes[..5]);
            Err(io::Error::other("admission output failed"))
        });
        assert!(result.is_err());
        state
            .finish_with(|_, bytes| {
                output.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        assert!(!output
            .windows(b"output complete".len())
            .any(|w| w == b"output complete"));
    }

    #[test]
    fn payload_and_marker_failures_leave_no_footer_or_retry() {
        let cap = minimum_cap() + 40;
        let mut state = State::new(Some(cap));
        let mut calls = 0;
        let mut output = Vec::new();
        assert!(state
            .write_with(
                Stream::Stdout,
                b"a complete payload line\n",
                |stream, bytes| {
                    calls += 1;
                    output.extend_from_slice(&bytes[..bytes.len().min(3)]);
                    if matches!(stream, Stream::Stdout) {
                        Err(io::Error::other("payload failed"))
                    } else {
                        Err(io::Error::other("marker failed"))
                    }
                }
            )
            .is_err());
        state
            .finish_with(|_, bytes| {
                calls += 1;
                output.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        assert_eq!(calls, 2);
        assert!(!output
            .windows(b"output complete".len())
            .any(|w| w == b"output complete"));
    }

    #[test]
    fn footer_failure_is_not_followed_by_more_output() {
        let cap = minimum_cap() + 100;
        let mut state = State::new(Some(cap));
        let mut calls = 0;
        let mut output = Vec::new();
        state
            .write_with(Stream::Stdout, b"payload\n", |_, bytes| {
                calls += 1;
                output.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        assert!(state
            .finish_with(|_, bytes| {
                calls += 1;
                output.extend_from_slice(&bytes[..4]);
                Err(io::Error::other("footer failed"))
            })
            .is_err());
        state
            .write_with(Stream::Stderr, b"later\n", |_, bytes| {
                calls += 1;
                output.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        assert_eq!(calls, 2);
        assert!(!output
            .windows(b"output complete".len())
            .any(|w| w == b"output complete"));
    }

    #[test]
    fn uncapped_finish_adds_no_bytes() {
        let mut state = State::new(None);
        let mut output = Vec::new();
        state
            .write_with(Stream::Stdout, b"unchanged\n", |_, bytes| {
                output.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        state
            .finish_with(|_, bytes| {
                output.extend_from_slice(bytes);
                Ok(())
            })
            .unwrap();
        assert_eq!(output, b"unchanged\n");
    }
}
