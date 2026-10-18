//! Read-only OS facts. Failure is uncertainty, never a guessed identity.
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootIdentity(String);
impl BootIdentity {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessStartIdentity {
    LinuxTicks(u64),
    MacOs { seconds: u64, microseconds: u64 },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub boot: BootIdentity,
    pub pid: u32,
    pub start: ProcessStartIdentity,
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
        if info.pbi_pid != pid || info.pbi_start_tvusec >= 1_000_000 {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        Ok(ProcessStartIdentity::MacOs {
            seconds: info.pbi_start_tvsec,
            microseconds: info.pbi_start_tvusec,
        })
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
    use super::*;
    #[test]
    fn linux_stat_handles_comm_delimiters_and_rejects_bad_evidence() {
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
