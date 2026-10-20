use super::*;
const PREFIX: &[u8] = b"\x1eEFFIGY-CUSTODY ";
const MAX_FRAME: usize = 1024;

/// Runs on stderr before any log, redactor, queue or tail sink. The pending
/// prefix/frame is bounded even when a malicious client never sends a newline.
#[derive(Debug)]
pub(crate) struct RegistrationStream {
    intent: Intent,
    pending: Vec<u8>,
    result: Option<Result<RemoteRegistration, UnknownReason>>,
    discard: bool,
    /// Framed on a backend terminal: stdout, with the terminal's `\r\n`.
    tty: bool,
}
impl RegistrationStream {
    pub(super) fn new(intent: Intent, tty: bool) -> Self {
        Self {
            intent,
            pending: Vec::new(),
            result: None,
            discard: false,
            tty,
        }
    }
    /// The client stream that carries the frame: stderr, or stdout on a tty.
    pub(crate) fn stream(&self) -> usize {
        if self.tty {
            0
        } else {
            1
        }
    }
    pub(super) fn registration(&self) -> Option<Result<RemoteRegistration, UnknownReason>> {
        self.result.clone()
    }
    pub(crate) fn push(&mut self, bytes: &[u8], eof: bool) -> Vec<u8> {
        let mut out = Vec::new();
        for &byte in bytes {
            if self.discard {
                if byte == b'\n' {
                    self.discard = false;
                }
                continue;
            }
            self.pending.push(byte);
            if self.pending.starts_with(PREFIX) {
                if byte == b'\n' {
                    let result = parse(&self.pending, &self.intent, self.tty);
                    self.result = Some(if self.result.is_some() {
                        Err(inconsistent("duplicate remote acknowledgement"))
                    } else {
                        result
                    });
                    self.pending.clear();
                } else if self.pending.len() > MAX_FRAME {
                    self.result = Some(Err(inconsistent("oversize remote acknowledgement")));
                    self.pending.clear();
                    self.discard = true;
                }
            } else {
                while !self.pending.is_empty() && !PREFIX.starts_with(&self.pending) {
                    out.push(self.pending.remove(0));
                }
            }
        }
        if eof && !self.pending.is_empty() {
            // A truncated registration is control data, never user output.
            if self.pending[0] == PREFIX[0] {
                self.result = Some(Err(inconsistent("truncated remote acknowledgement")));
            } else {
                out.append(&mut self.pending);
            }
            self.pending.clear();
        }
        out
    }
}
fn parse(line: &[u8], intent: &Intent, tty: bool) -> Result<RemoteRegistration, UnknownReason> {
    let text = std::str::from_utf8(line).map_err(|_| inconsistent("non-UTF8 acknowledgement"))?;
    let mut text = text.strip_suffix('\n').unwrap_or(text);
    if tty {
        text = text.strip_suffix('\r').unwrap_or(text);
    }
    let fields: Vec<_> = text.split(' ').collect();
    if fields.len() != 8
        || fields[0] != "\x1eEFFIGY-CUSTODY"
        || fields[1] != "v1"
        || fields[2] != intent.nonce.as_str()
    {
        return Err(inconsistent("invalid acknowledgement frame"));
    }
    let number = |i: usize| {
        fields[i]
            .parse::<u64>()
            .map_err(|_| inconsistent("invalid acknowledgement number"))
    };
    let pid = u32::try_from(number(3)?).map_err(|_| inconsistent("pid overflow"))?;
    let sid = u32::try_from(number(4)?).map_err(|_| inconsistent("sid overflow"))?;
    let mut container = intent.container.clone();
    container.vm_boot_id = fields[6].into();
    let reg = RemoteRegistration {
        env: intent.env.clone(),
        container,
        pid_ns: number(7)?,
        sid,
        leader_pid: pid,
        leader_start_ticks: number(5)?,
        nonce: intent.nonce.clone(),
        session_isolated: sid == pid,
    };
    validate(&reg, intent)?;
    Ok(reg)
}
pub(super) fn validate(reg: &RemoteRegistration, intent: &Intent) -> Result<(), UnknownReason> {
    if reg.env != intent.env
        || reg.container != intent.container
        || reg.nonce != intent.nonce
        || !valid_shape(reg)
    {
        return Err(inconsistent("remote registration identity mismatch"));
    }
    Ok(())
}

pub(super) fn valid_shape(reg: &RemoteRegistration) -> bool {
    valid_identity(&reg.container)
        && reg.pid_ns > 0
        && reg.leader_pid > 0
        && reg.leader_pid <= i32::MAX as u32
        && reg.sid > 0
        && reg.sid <= i32::MAX as u32
        && reg.leader_start_ticks > 0
        && reg.session_isolated == (reg.sid == reg.leader_pid)
        && reg.nonce.0.len() == 64
        && reg.nonce.0.bytes().all(|b| b.is_ascii_hexdigit())
}
