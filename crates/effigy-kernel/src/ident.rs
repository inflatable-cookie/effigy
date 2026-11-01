//! Read-only OS facts. Failure is uncertainty, never a guessed identity.
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BootIdentity(String);
impl BootIdentity {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProcessStartIdentity {
    LinuxTicks(u64),
    MacOs { seconds: u64, microseconds: u64 },
}
impl ProcessStartIdentity {
    /// Stable text form for diagnostic bodies; never parsed back as authority.
    pub fn text(&self) -> String {
        match self {
            Self::LinuxTicks(ticks) => format!("linux:{ticks}"),
            Self::MacOs {
                seconds,
                microseconds,
            } => format!("macos:{seconds}.{microseconds:06}"),
        }
    }
}
/// Scheduler state, for diagnostics such as a stopped gate holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ProcessState {
    Active,
    Stopped,
    Zombie,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProcessIdentity {
    pub boot: BootIdentity,
    pub pid: u32,
    pub start: ProcessStartIdentity,
}

/// Prove the process started no later than a filesystem timestamp.
pub fn started_before(identity: &ProcessIdentity, time: std::time::SystemTime) -> bool {
    let Some(limit) = time.duration_since(std::time::UNIX_EPOCH).ok() else {
        return false;
    };
    match identity.start {
        ProcessStartIdentity::MacOs {
            seconds,
            microseconds,
        } => {
            microseconds < 1_000_000
                && Duration::new(seconds, (microseconds * 1_000) as u32) <= limit
        }
        ProcessStartIdentity::LinuxTicks(ticks) => {
            #[cfg(target_os = "linux")]
            {
                let Ok(stat) = std::fs::read_to_string("/proc/stat") else {
                    return false;
                };
                let Some(boot) = stat
                    .lines()
                    .find_map(|line| line.strip_prefix("btime ")?.parse::<u64>().ok())
                else {
                    return false;
                };
                // SAFETY: sysconf reads a constant process-wide clock property.
                let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
                let Ok(hz) = u64::try_from(hz) else {
                    return false;
                };
                if hz == 0 {
                    return false;
                }
                // btime is rounded to a second; use an upper bound, not a guess.
                let Some(secs) = boot
                    .checked_add(1)
                    .and_then(|v| v.checked_add(ticks / hz))
                    .and_then(|v| v.checked_add(1))
                else {
                    return false;
                };
                Duration::from_secs(secs) <= limit
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = ticks;
                false
            }
        }
    }
}

/// The executable a process was started from, read from the operating system
/// rather than from its argument vector: `execve` lets a caller choose
/// `argv[0]`, so only this fact names the program. Unreadable is uncertainty.
pub fn process_executable(pid: u32) -> std::io::Result<std::path::PathBuf> {
    if pid == 0 || pid > i32::MAX as u32 {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    #[cfg(target_os = "macos")]
    {
        let mut bytes = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: the buffer is writable for exactly the length passed, lives
        // for the call and is disjoint from the arguments; a positive return is
        // the number of bytes written, including the terminating NUL.
        let written = unsafe {
            libc::proc_pidpath(pid as i32, bytes.as_mut_ptr().cast(), bytes.len() as u32)
        };
        if written <= 0 {
            return Err(std::io::Error::last_os_error());
        }
        let written = usize::try_from(written).map_err(|_| std::io::ErrorKind::InvalidData)?;
        let bytes = bytes
            .get(..written)
            .ok_or(std::io::ErrorKind::InvalidData)?;
        let value = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        let path = std::str::from_utf8(value)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
        Ok(std::path::PathBuf::from(path))
    }
    #[cfg(target_os = "linux")]
    {
        // `/proc/<pid>/exe` resolves to the file that was executed; a process we
        // may not inspect is unreadable rather than unknown-but-assumed.
        std::fs::read_link(format!("/proc/{pid}/exe"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}

/// Read exact argument boundaries without using a process-table pattern scan.
pub fn process_arguments(pid: u32) -> std::io::Result<Vec<std::ffi::OsString>> {
    use std::os::unix::ffi::OsStringExt;
    if pid == 0 || pid > i32::MAX as u32 {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    #[cfg(target_os = "macos")]
    {
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
        let mut bytes = vec![0u8; 1 << 20];
        let mut size = bytes.len();
        // SAFETY: the three-element MIB, size and byte buffer are initialized,
        // writable, live and disjoint for the call; null newp makes it read-only.
        if unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                3,
                bytes.as_mut_ptr().cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        if size > bytes.len() {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        bytes.truncate(size);
        let argc = i32::from_ne_bytes(
            bytes
                .get(..4)
                .ok_or(std::io::ErrorKind::InvalidData)?
                .try_into()
                .map_err(|_| std::io::ErrorKind::InvalidData)?,
        );
        if !(1..=4096).contains(&argc) {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        let mut offset = 4;
        while bytes
            .get(offset)
            .copied()
            .ok_or(std::io::ErrorKind::InvalidData)?
            != 0
        {
            offset += 1;
        }
        while bytes.get(offset) == Some(&0) {
            offset += 1;
        }
        let mut args = Vec::new();
        for _ in 0..argc {
            let end = bytes
                .get(offset..)
                .ok_or(std::io::ErrorKind::InvalidData)?
                .iter()
                .position(|b| *b == 0)
                .ok_or(std::io::ErrorKind::InvalidData)?
                + offset;
            args.push(std::ffi::OsString::from_vec(bytes[offset..end].to_vec()));
            offset = end + 1;
        }
        return Ok(args);
    };
    #[cfg(target_os = "linux")]
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline"))?;
    #[cfg(target_os = "linux")]
    {
        if bytes.is_empty() || bytes.last() != Some(&0) || bytes.len() > 1 << 20 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        Ok(bytes[..bytes.len() - 1]
            .split(|b| *b == 0)
            .map(|part| std::ffi::OsString::from_vec(part.to_vec()))
            .collect())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}
/// A process's place in its session, for bounded caller attribution. The
/// parent and session are read-only facts; an unreadable one is uncertainty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessLineage {
    pub parent: u32,
    pub session: Option<u32>,
}

/// Read the parent and session of one process.
pub fn process_lineage(pid: u32) -> std::io::Result<ProcessLineage> {
    if pid == 0 || pid > i32::MAX as u32 {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    platform::lineage(pid)
}

/// The invoking process's controlling terminal, when it has one. The terminal is
/// a property of the session, not of a descriptor: a standard descriptor can
/// belong to another terminal, and a process can hold a terminal descriptor
/// without a controlling terminal at all, so the session's own terminal is
/// resolved by its device rather than from whatever file descriptor is a tty.
pub fn controlling_terminal() -> Option<String> {
    platform::controlling_terminal()
}

/// The generic alias a controlling-terminal descriptor can report, which names
/// no terminal identity of its own.
#[cfg(test)]
const GENERIC_TERMINAL: &str = "/dev/tty";

/// A tty name that identifies a device, never the generic alias.
#[cfg(test)]
fn tty_name(fd: libc::c_int) -> Option<String> {
    let mut buffer = [0 as libc::c_char; 128];
    // SAFETY: fd is live, and ttyname_r writes at most buffer.len() bytes into
    // the live, writable and correctly typed buffer.
    if unsafe { libc::ttyname_r(fd, buffer.as_mut_ptr(), buffer.len()) } != 0 {
        return None;
    }
    // SAFETY: a successful ttyname_r NUL-terminates the buffer.
    let path = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    let real = path.starts_with("/dev/") && path != GENERIC_TERMINAL && path.len() <= 96;
    real.then_some(path)
}

/// The device number a character-device descriptor refers to.
///
/// `st_rdev` is `u64` on Linux and `u32` on macOS, so a single `as u64` is
/// necessary on one platform and `clippy::unnecessary_cast` on the other
/// (effigy#520). Neither arm casts: the narrow platform widens with `u64::from`
/// and the wide one needs nothing. The value is identical on both.
#[cfg(all(test, target_os = "linux"))]
fn device_number(device: libc::dev_t) -> u64 {
    device
}

#[cfg(all(test, not(target_os = "linux")))]
fn device_number(device: libc::dev_t) -> u64 {
    // `dev_t` is a signed 32-bit value on macOS, so this widening is required
    // there; the Linux arm above would make the same cast redundant.
    device as u64
}

#[cfg(test)]
fn device_of(fd: libc::c_int) -> Option<u64> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: fd is live and stat points to aligned writable full storage.
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: a successful fstat initialized the full output structure.
    let stat = unsafe { stat.assume_init() };
    (stat.st_mode & libc::S_IFMT == libc::S_IFCHR).then_some(device_number(stat.st_rdev))
}

/// Darwin names a pty slave `/dev/ttysNNN` from device major 16 and the minor
/// number. Another kind of terminal cannot be named from its device.
#[cfg(target_os = "macos")]
fn pty_device_path(device: u32) -> Option<String> {
    let device = u64::from(device);
    let major = (device >> 24) & 0xff;
    let minor = device & 0x00ff_ffff;
    (major == 16 && minor <= 999).then(|| format!("/dev/ttys{minor:03}"))
}

/// Linux names a terminal from its device number: pty slaves are major 136-143
/// and live under `/dev/pts`, and virtual or serial consoles are major 4.
#[cfg(any(target_os = "linux", test))]
fn linux_device_path(device: u64) -> Option<String> {
    let major = ((device >> 8) & 0xfff) | ((device >> 32) & !0xfff);
    let minor = (device & 0xff) | ((device >> 12) & !0xff);
    match major {
        136..=143 => Some(format!("/dev/pts/{}", (major - 136) * 256 + minor)),
        4 if minor < 64 => Some(format!("/dev/tty{minor}")),
        4 => Some(format!("/dev/ttyS{}", minor - 64)),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootInstant {
    pub boot: BootIdentity,
    pub elapsed: Duration,
}
impl BootInstant {
    /// A reboot invalidates a persisted deadline, rather than comparing clocks
    /// from different boots. Wall time is never involved.
    pub fn elapsed_since(&self, earlier: &Self) -> Option<Duration> {
        (self.boot == earlier.boot)
            .then(|| self.elapsed.checked_sub(earlier.elapsed))
            .flatten()
    }
}

fn evidence(source: &str) -> Evidence {
    // These are fixed short, nonempty kernel source names, below the bound.
    Evidence::new(source.into(), None).expect("fixed evidence source")
}
fn failure(error: std::io::Error) -> UnknownReason {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        UnknownReason::PermissionDenied
    } else {
        UnknownReason::Inconsistent(error.to_string())
    }
}
pub fn boot_identity() -> Observed<BootIdentity> {
    #[cfg(all(test, feature = "fault-injection"))]
    if let Some(injected) = crate::fault::injected() {
        return injected;
    }
    match platform::boot() {
        Ok(value) => match parse_boot_identity(&value) {
            Some(identity) => Observed::Verified(identity, evidence("kernel boot identity")),
            None => Observed::Unknown(UnknownReason::Unparsable("boot UUID".into())),
        },
        Err(error) => Observed::Unknown(failure(error)),
    }
}
fn parse_boot_identity(value: &str) -> Option<BootIdentity> {
    let value = value.trim();
    if value.len() != 36
        || !value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return None;
    }
    Some(BootIdentity(value.to_ascii_lowercase()))
}
pub fn process_identity(pid: u32) -> Observed<ProcessIdentity> {
    #[cfg(all(test, feature = "fault-injection"))]
    if let Some(injected) = crate::fault::injected() {
        return injected;
    }
    if pid == 0 || pid > i32::MAX as u32 {
        return Observed::Unknown(UnknownReason::Unparsable(
            "pid outside process range".into(),
        ));
    }
    let boot = match boot_identity() {
        Observed::Verified(boot, _) => boot,
        Observed::Unknown(reason) => return Observed::Unknown(reason),
        Observed::Absent(_) => {
            return Observed::Unknown(UnknownReason::Inconsistent("missing boot identity".into()))
        }
    };
    match platform::start(pid) {
        Ok(start) => Observed::Verified(
            ProcessIdentity { boot, pid, start },
            evidence("kernel process start identity"),
        ),
        Err(error) if matches!(error.raw_os_error(), Some(libc::ESRCH)) => {
            Observed::Absent(AbsenceProof::NoSuchProcess { pid })
        }
        Err(error) => Observed::Unknown(failure(error)),
    }
}
pub fn process_state(pid: u32) -> Observed<ProcessState> {
    #[cfg(all(test, feature = "fault-injection"))]
    if let Some(injected) = crate::fault::injected() {
        return injected;
    }
    if pid == 0 || pid > i32::MAX as u32 {
        return Observed::Unknown(UnknownReason::Unparsable(
            "pid outside process range".into(),
        ));
    }
    match platform::state(pid) {
        Ok(state) => Observed::Verified(state, evidence("kernel process state")),
        Err(error) if matches!(error.raw_os_error(), Some(libc::ESRCH)) => {
            Observed::Absent(AbsenceProof::NoSuchProcess { pid })
        }
        Err(error) => Observed::Unknown(failure(error)),
    }
}
pub fn boot_clock() -> Observed<BootInstant> {
    #[cfg(all(test, feature = "fault-injection"))]
    if let Some(injected) = crate::fault::injected() {
        return injected;
    }
    let boot = match boot_identity() {
        Observed::Verified(boot, _) => boot,
        Observed::Unknown(reason) => return Observed::Unknown(reason),
        Observed::Absent(_) => {
            return Observed::Unknown(UnknownReason::Inconsistent("missing boot identity".into()))
        }
    };
    match platform::clock() {
        Ok(elapsed) => Observed::Verified(
            BootInstant { boot, elapsed },
            evidence("kernel continuous boot clock"),
        ),
        Err(error) => Observed::Unknown(failure(error)),
    }
}

// Keep the parser built on both platforms for malformed /proc fixture proofs.
#[cfg(any(target_os = "linux", test))]
fn parse_linux_start(stat: &str, expected_pid: u32) -> Option<ProcessStartIdentity> {
    let (pid, _) = stat.split_once(" (")?;
    if pid.parse::<u32>().ok()? != expected_pid {
        return None;
    }
    // comm may contain spaces, parentheses and newlines. The last ')' is the
    // only delimiter before field 3; field 22 is index 19 in the remainder.
    let (_, rest) = stat.rsplit_once(')')?;
    let start = rest.split_whitespace().nth(19)?.parse::<u64>().ok()?;
    Some(ProcessStartIdentity::LinuxTicks(start))
}

#[cfg(any(target_os = "linux", test))]
fn parse_linux_lineage(stat: &str, expected_pid: u32) -> Option<ProcessLineage> {
    let (pid, _) = stat.split_once(" (")?;
    if pid.parse::<u32>().ok()? != expected_pid {
        return None;
    }
    let (_, rest) = stat.rsplit_once(')')?;
    let mut fields = rest.split_whitespace();
    // Field 3 is state, so parent is index 1 and session index 3.
    fields.next()?;
    let parent = fields.next()?.parse::<u32>().ok()?;
    fields.next()?;
    Some(ProcessLineage {
        parent,
        session: fields.next()?.parse::<u32>().ok(),
    })
}

/// Field 7 of `/proc/<pid>/stat` is the session's controlling terminal device;
/// zero means the process has none.
#[cfg(any(target_os = "linux", test))]
fn parse_linux_tty(stat: &str, expected_pid: u32) -> Option<u64> {
    let (pid, _) = stat.split_once(" (")?;
    if pid.parse::<u32>().ok()? != expected_pid {
        return None;
    }
    let (_, rest) = stat.rsplit_once(')')?;
    let mut fields = rest.split_whitespace();
    // Field 3 is state, so ppid, pgrp and session follow it, then tty_nr.
    fields.next()?;
    let device = fields.nth(3)?.parse::<i64>().ok()?;
    (device > 0).then_some(device as u64)
}

#[cfg(any(target_os = "linux", test))]
fn parse_linux_state(stat: &str, expected_pid: u32) -> Option<ProcessState> {
    let (pid, _) = stat.split_once(" (")?;
    if pid.parse::<u32>().ok()? != expected_pid {
        return None;
    }
    let (_, rest) = stat.rsplit_once(')')?;
    Some(match rest.split_whitespace().next()? {
        "T" | "t" => ProcessState::Stopped,
        "Z" | "X" | "x" => ProcessState::Zombie,
        _ => ProcessState::Active,
    })
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::ffi::CString;
    use std::fs::File;
    use std::io::{self, Read};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    fn open_at(parent: i32, name: &str, directory: bool) -> io::Result<OwnedFd> {
        let name = CString::new(name).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if directory { libc::O_DIRECTORY } else { 0 };
        // SAFETY: name is NUL-terminated, flags request no creation, and parent
        // is a live owned directory fd (or AT_FDCWD for the fixed root '/').
        let fd = unsafe { libc::openat(parent, name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a fresh valid fd; ownership transfers once.
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: fd is live and stat points to aligned writable full storage.
        if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful fstat initialized the full output structure.
        let stat = unsafe { stat.assume_init() };
        // SAFETY: geteuid has no arguments or preconditions.
        let own_uid = unsafe { libc::geteuid() };
        let kind = if directory {
            libc::S_IFDIR
        } else {
            libc::S_IFREG
        };
        if stat.st_mode & libc::S_IFMT != kind
            || stat.st_mode & 0o022 != 0
            || (stat.st_uid != 0 && stat.st_uid != own_uid)
        {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        Ok(owned)
    }
    fn read_proc(parts: &[&str], max: u64, missing_pid: bool) -> io::Result<String> {
        let root = open_at(libc::AT_FDCWD, "/", true)?;
        let mut directory = open_at(root.as_raw_fd(), "proc", true)?;
        for (index, name) in parts.iter().enumerate() {
            let last = index + 1 == parts.len();
            let next = open_at(directory.as_raw_fd(), name, !last).map_err(|error| {
                if missing_pid && error.kind() == io::ErrorKind::NotFound {
                    io::Error::from_raw_os_error(libc::ESRCH)
                } else {
                    error
                }
            })?;
            if last {
                let mut data = String::new();
                File::from(next).take(max + 1).read_to_string(&mut data)?;
                if data.len() as u64 > max {
                    return Err(io::Error::from(io::ErrorKind::InvalidData));
                }
                return Ok(data);
            }
            directory = next;
        }
        Err(io::Error::from(io::ErrorKind::InvalidInput))
    }
    pub fn boot() -> io::Result<String> {
        read_proc(&["sys", "kernel", "random", "boot_id"], 256, false)
    }
    pub fn start(pid: u32) -> io::Result<ProcessStartIdentity> {
        let stat = read_proc(&[&pid.to_string(), "stat"], 8192, true)?;
        parse_linux_start(&stat, pid).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
    }
    pub fn lineage(pid: u32) -> io::Result<ProcessLineage> {
        let stat = read_proc(&[&pid.to_string(), "stat"], 8192, true)?;
        parse_linux_lineage(&stat, pid).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
    }
    pub fn controlling_terminal() -> Option<String> {
        // The kernel's stat line records the session's controlling terminal as a
        // device number; an open `/dev/tty` would report the alias device, so the
        // session evidence names the terminal instead.
        let pid = std::process::id();
        let stat = read_proc(&[&pid.to_string(), "stat"], 8192, false).ok()?;
        super::linux_device_path(parse_linux_tty(&stat, pid)?)
    }
    pub fn state(pid: u32) -> io::Result<ProcessState> {
        let stat = read_proc(&[&pid.to_string(), "stat"], 8192, true)?;
        parse_linux_state(&stat, pid).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
    }
    pub fn clock() -> io::Result<Duration> {
        let mut time = std::mem::MaybeUninit::<libc::timespec>::uninit();
        // SAFETY: a supported clock id and aligned writable timespec storage.
        if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, time.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: clock_gettime succeeded and initialized time.
        let time = unsafe { time.assume_init() };
        if time.tv_sec < 0 || !(0..1_000_000_000).contains(&time.tv_nsec) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        Ok(Duration::new(time.tv_sec as u64, time.tv_nsec as u32))
    }
}
#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::io;
    pub fn boot() -> io::Result<String> {
        let mut data = [0u8; 256];
        let mut size = data.len();
        // SAFETY: constant NUL-terminated name, data is writable for size bytes,
        // size points to valid storage; null newp and zero newlen make this a
        // pure read. Both buffers remain live and unaliased through the call.
        if unsafe {
            libc::sysctlbyname(
                c"kern.bootsessionuuid".as_ptr(),
                data.as_mut_ptr().cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let bytes = data
            .get(..size)
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
        let value = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        String::from_utf8(value.to_vec()).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))
    }
    pub fn start(pid: u32) -> io::Result<ProcessStartIdentity> {
        let info = bsdinfo(pid)?;
        if info.pbi_start_tvusec >= 1_000_000 {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        Ok(ProcessStartIdentity::MacOs {
            seconds: info.pbi_start_tvsec,
            microseconds: info.pbi_start_tvusec,
        })
    }
    pub fn controlling_terminal() -> Option<String> {
        // The controlling-terminal descriptor answers `/dev/tty` and carries the
        // alias device, so the kernel's own record of this session's terminal is
        // used: bsdinfo's effective terminal device names the pty slave.
        let info = bsdinfo(std::process::id()).ok()?;
        super::pty_device_path(info.e_tdev)
    }
    pub fn lineage(pid: u32) -> io::Result<ProcessLineage> {
        let info = bsdinfo(pid)?;
        // SAFETY: getsid takes one pid and has no other precondition. A
        // negative result means no session fact, never a guessed session.
        let session = unsafe { libc::getsid(pid as libc::pid_t) };
        Ok(ProcessLineage {
            parent: info.pbi_ppid,
            session: (session > 0).then_some(session as u32),
        })
    }
    pub fn state(pid: u32) -> io::Result<ProcessState> {
        // SSTOP and SZOMB from the macOS SDK's sys/proc.h.
        Ok(match bsdinfo(pid)?.pbi_status {
            4 => ProcessState::Stopped,
            5 => ProcessState::Zombie,
            _ => ProcessState::Active,
        })
    }
    fn bsdinfo(pid: u32) -> io::Result<libc::proc_bsdinfo> {
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
        let size = std::mem::size_of::<libc::proc_bsdinfo>();
        // SAFETY: validated positive pid fits i32; PROC_PIDTBSDINFO takes no
        // additional argument. info is aligned writable storage of exactly size
        // bytes. No initialized fields are read unless the full size is returned.
        let count = unsafe {
            libc::proc_pidinfo(
                pid as i32,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size as i32,
            )
        };
        if count != size as i32 {
            return Err(if count <= 0 {
                io::Error::last_os_error()
            } else {
                io::Error::from(io::ErrorKind::InvalidData)
            });
        }
        // SAFETY: proc_pidinfo returned the full initialized structure size.
        let info = unsafe { info.assume_init() };
        if info.pbi_pid != pid {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        Ok(info)
    }
    // ABI from the macOS SDK's mach/mach_time.h. libc does not export
    // mach_continuous_time, and its old timebase wrapper is deprecated.
    #[repr(C)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    unsafe extern "C" {
        fn mach_continuous_time() -> u64;
        fn mach_timebase_info(info: *mut Timebase) -> i32;
    }
    pub fn clock() -> io::Result<Duration> {
        let mut scale = Timebase { numer: 0, denom: 0 };
        // SAFETY: scale is aligned writable storage for the required structure.
        if unsafe { mach_timebase_info(&mut scale) } != 0 || scale.denom == 0 {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        // SAFETY: mach_continuous_time has no arguments or preconditions.
        let ticks = unsafe { mach_continuous_time() };
        let nanos = u128::from(ticks) * u128::from(scale.numer) / u128::from(scale.denom);
        let seconds = u64::try_from(nanos / 1_000_000_000)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        Ok(Duration::new(seconds, (nanos % 1_000_000_000) as u32))
    }
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("effigy-kernel requires macOS or Linux");

#[cfg(test)]
mod tests {
    #[test]
    fn colima_runtime_process_arguments_preserve_own_argv() {
        let args = super::process_arguments(std::process::id()).expect("read own arguments");
        assert_eq!(args, std::env::args_os().collect::<Vec<_>>());
    }
    use super::*;
    #[test]
    fn linux_stat_lineage_and_state_reject_bad_evidence() {
        let stat = format!(
            "42 (name ) with ( spaces\n) S {} 98765 0 0",
            vec!["0"; 18].join(" ")
        );
        assert_eq!(
            parse_linux_start(&stat, 42),
            Some(ProcessStartIdentity::LinuxTicks(98765))
        );
        assert_eq!(parse_linux_start(&stat, 43), None);
        assert_eq!(parse_linux_start("42 (short) S 0", 42), None);
        assert_eq!(parse_linux_start(&stat.replace("98765", "bad"), 42), None);
        assert_eq!(parse_linux_state(&stat, 42), Some(ProcessState::Active));
        let stopped = stat.replace(") S ", ") T ");
        assert_eq!(parse_linux_state(&stopped, 42), Some(ProcessState::Stopped));
        assert_eq!(parse_linux_state(&stopped, 41), None);
        assert_eq!(
            parse_linux_lineage(&stat, 42),
            Some(ProcessLineage {
                parent: 0,
                session: Some(0)
            })
        );
        let lineage = format!("42 (n) S 7 8 9 {}", vec!["0"; 16].join(" "));
        assert_eq!(
            parse_linux_lineage(&lineage, 42),
            Some(ProcessLineage {
                parent: 7,
                session: Some(9)
            })
        );
        assert_eq!(parse_linux_lineage(&lineage, 41), None);
        assert_eq!(parse_linux_lineage("42 (short) S 0", 42), None);
        assert_eq!(parse_linux_lineage(&stat, 41), None);
        // The invoking process always has a readable lineage, and a pid outside
        // the process id domain is an error rather than a guessed parent.
        let own = process_lineage(std::process::id()).expect("read own lineage");
        assert_eq!(own.parent, std::os::unix::process::parent_id());
        assert!(process_lineage(0).is_err());
        assert!(process_lineage(i32::MAX as u32 + 1).is_err());
    }
    #[test]
    fn linux_stat_tty_names_the_controlling_device() {
        // Field 7 carries the controlling terminal's device; 34816 is
        // major 136, minor 0, the first pty slave.
        let stat = "42 (n) S 7 8 9 34816 0 0 0";
        assert_eq!(parse_linux_tty(stat, 42), Some(34816));
        assert_eq!(parse_linux_tty("42 (n) S 7 8 9 0 0 0", 42), None);
        assert_eq!(parse_linux_tty(stat, 41), None);
        assert_eq!(parse_linux_tty("42 (short) S 7 8", 42), None);
        assert_eq!(
            parse_linux_tty(stat, 42).and_then(linux_device_path),
            Some("/dev/pts/0".to_owned())
        );
    }

    #[test]
    fn linux_device_numbers_name_their_terminal() {
        // glibc's encoding: major from bits 8-19 and 32+, minor from bits 0-7
        // and 12-31.
        let device = |major: u64, minor: u64| (major << 8) | (minor & 0xff);
        assert_eq!(
            linux_device_path(device(136, 0)),
            Some("/dev/pts/0".to_owned())
        );
        assert_eq!(
            linux_device_path(device(136, 3)),
            Some("/dev/pts/3".to_owned())
        );
        // A pty beyond the first 256 minors moves to the next major.
        assert_eq!(
            linux_device_path(device(137, 0)),
            Some("/dev/pts/256".to_owned())
        );
        assert_eq!(
            linux_device_path(device(4, 1)),
            Some("/dev/tty1".to_owned())
        );
        assert_eq!(
            linux_device_path(device(4, 64)),
            Some("/dev/ttyS0".to_owned())
        );
        // The `/dev/tty` alias device names no terminal identity.
        assert_eq!(linux_device_path(device(2, 0)), None);
    }

    #[test]
    fn terminal_name_reads_a_pty_and_refuses_a_pipe() {
        let mut master: libc::c_int = -1;
        let mut slave: libc::c_int = -1;
        // SAFETY: openpty writes both descriptors and no termios/winp is passed.
        let opened = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(opened, 0, "a pty is available to the test");
        let name = tty_name(slave).expect("a pty slave has a terminal name");
        // Each platform names its pty slaves in its own directory.
        let prefix = if cfg!(target_os = "macos") {
            "/dev/ttys"
        } else {
            "/dev/pts/"
        };
        assert!(name.starts_with(prefix), "{name}");
        let device = device_of(slave).expect("the slave is a character device");
        // The slave's device names the same terminal, which is how the generic
        // /dev/tty alias is resolved on each platform.
        #[cfg(target_os = "macos")]
        assert_eq!(
            pty_device_path(device as u32),
            Some(name.clone()),
            "the device resolves to the same slave identity"
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            linux_device_path(device),
            Some(name.clone()),
            "the device resolves to the same slave identity"
        );
        // SAFETY: both descriptors were opened above and are closed once each.
        unsafe {
            libc::close(slave);
            libc::close(master);
        }
        let mut pipe = [0 as libc::c_int; 2];
        // SAFETY: pipe writes two descriptors into the live array.
        assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
        assert!(tty_name(pipe[0]).is_none(), "a pipe is not a terminal");
        // SAFETY: both descriptors were opened above and are closed once each.
        unsafe {
            libc::close(pipe[0]);
            libc::close(pipe[1]);
        }
    }

    /// A pty session whose child has no standard descriptor on the terminal:
    /// the controlling-terminal field must still name the slave device, not the
    /// generic `/dev/tty` alias (effigy#450 review).
    fn pty_child(master: &mut libc::c_int, name: &mut [libc::c_char; 128]) -> libc::pid_t {
        #[cfg(target_os = "macos")]
        // SAFETY: forkpty writes the master and name, and null optional
        // arguments request the defaults.
        unsafe {
            libc::forkpty(
                master,
                name.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        }
        #[cfg(target_os = "linux")]
        // SAFETY: as above; Linux takes const pointers for the optional values.
        unsafe {
            libc::forkpty(
                master,
                name.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
            )
        }
    }

    #[test]
    fn controlling_terminal_names_the_pty_itself_with_stdin_redirected() {
        let mut master: libc::c_int = -1;
        let mut name = [0 as libc::c_char; 128];
        let mut pipe = [0 as libc::c_int; 2];
        // SAFETY: pipe writes two descriptors into the live array.
        assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
        let child = pty_child(&mut master, &mut name);
        assert!(child >= 0, "fork a pty probe process");
        if child == 0 {
            // SAFETY: the child of a pty probe runs only libc calls and this
            // inference, and never returns into the test harness.
            unsafe {
                libc::close(pipe[0]);
                // A session keeps its controlling terminal while any descriptor
                // still refers to it, exactly as a shell does for the command it
                // runs; every standard descriptor then leaves the terminal, so
                // only the controlling terminal can answer.
                let kept = libc::dup(libc::STDIN_FILENO);
                if kept < 0 {
                    libc::_exit(1);
                }
                let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
                if null < 0 {
                    libc::_exit(1);
                }
                for fd in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
                    if libc::dup2(null, fd) < 0 {
                        libc::_exit(2);
                    }
                }
                libc::close(null);
                // Another terminal on stdout must not be mistaken for the
                // controlling terminal: only stdin and stderr leave the session's
                // terminal here.
                let mut other_master: libc::c_int = -1;
                let mut other_slave: libc::c_int = -1;
                let other = libc::openpty(
                    &mut other_master,
                    &mut other_slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                if other != 0 {
                    libc::_exit(3);
                }
                if libc::dup2(other_slave, libc::STDOUT_FILENO) < 0 {
                    libc::_exit(4);
                }
                libc::close(other_slave);
                libc::close(other_master);
                if libc::isatty(libc::STDIN_FILENO) != 0
                    || libc::isatty(libc::STDERR_FILENO) != 0
                    || libc::isatty(libc::STDOUT_FILENO) != 1
                {
                    libc::_exit(5);
                }
                let resolved = super::controlling_terminal();
                libc::close(kept);
                let Some(resolved) = resolved else {
                    libc::_exit(6);
                };
                let bytes = resolved.as_bytes();
                let written = libc::write(pipe[1], bytes.as_ptr().cast(), bytes.len());
                libc::_exit(if written == bytes.len() as isize {
                    0
                } else {
                    7
                });
            }
        }
        // SAFETY: the parent reads the child's one message and reaps it.
        let (status, resolved) = unsafe {
            libc::close(pipe[1]);
            let mut buffer = [0u8; 128];
            let read = libc::read(pipe[0], buffer.as_mut_ptr().cast(), buffer.len());
            libc::close(pipe[0]);
            let mut status: libc::c_int = 0;
            libc::waitpid(child, &mut status, 0);
            let resolved = if read > 0 {
                Some(String::from_utf8_lossy(&buffer[..read as usize]).into_owned())
            } else {
                None
            };
            libc::close(master);
            (status, resolved)
        };
        assert_eq!(
            status, 0,
            "the probe kept its controlling terminal with every standard descriptor redirected \
             (status {status})"
        );
        let expected = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        let prefix = if cfg!(target_os = "macos") {
            "/dev/ttys"
        } else {
            "/dev/pts/"
        };
        assert!(expected.starts_with(prefix), "{expected}");
        assert_eq!(
            resolved,
            Some(expected),
            "the record names the pty, not the /dev/tty alias"
        );
    }

    #[test]
    fn boot_uuid_and_cross_boot_clock_are_strict() {
        assert!(parse_boot_identity("not a UUID").is_none());
        let a = parse_boot_identity("aaaaaaaa-0000-0000-0000-000000000000").unwrap();
        let b = parse_boot_identity("bbbbbbbb-0000-0000-0000-000000000000").unwrap();
        assert_eq!(
            BootInstant {
                boot: a,
                elapsed: Duration::from_secs(10)
            }
            .elapsed_since(&BootInstant {
                boot: b,
                elapsed: Duration::ZERO
            }),
            None
        );
    }
}
