//! One suite-owned sweep for a built-in `test` run.
//!
//! The route owns one suite token. A host suite receives it in the environment
//! and opens its marker descriptor from a shell prologue, because a kernel
//! custody payload closes inherited descriptors on purpose. After the last
//! suite ends the sweep reads live kernel state once, TERMs and then KILLs
//! whatever still carries the token, and fails the route naming it. Nothing is
//! recorded and nothing is repaired.
use effigy_core::shell::shell_quote;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The variable that reaches every suite descendant that keeps its environment.
const TOKEN_ENV: &str = "EFFIGY_TEST_SUITE";
/// The descriptor a suite opens for itself, above the custody protocol's slots.
const MARKER_FD: u32 = 9;
const TERM_GRACE: Duration = Duration::from_secs(2);
const KILL_GRACE: Duration = Duration::from_secs(2);
/// A machine-wide install is never one suite's descendant.
const INSTALLED_EFFIGY: &str = ".local-install";

pub(super) struct Sweep {
    token: String,
    marker: PathBuf,
}

impl Sweep {
    /// Begin one suite in the run's scratch. `None` means the marker could not
    /// be created, and the route then has nothing to sweep rather than failing.
    pub(super) fn begin(scratch: Option<&Path>) -> Option<Self> {
        let dir = scratch.map_or_else(std::env::temp_dir, Path::to_path_buf);
        for _ in 0..4 {
            let token = token();
            let marker = dir.join(format!("effigy-suite-{token}.marker"));
            if std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)
                .is_ok()
            {
                return Some(Self { token, marker });
            }
        }
        eprintln!("suite sweep: cannot create a marker in {}", dir.display());
        None
    }

    pub(super) fn token_env(&self) -> (&'static str, String) {
        (TOKEN_ENV, self.token.clone())
    }

    /// A suite command that first opens the marker descriptor for itself, so
    /// the suite and everything it starts carry the suite identity.
    pub(super) fn command(&self, command: &str) -> String {
        format!(
            "exec {MARKER_FD}>>{} || exit 1\n{command}",
            shell_quote(&self.marker.to_string_lossy())
        )
    }

    /// Name and reap every process that still carries this suite. A surviving
    /// process is reported by the caller as the route's failure.
    pub(super) fn end(self) -> Vec<String> {
        let found = holders(&self.marker, &self.token)
            .into_iter()
            .filter(|pid| *pid != std::process::id())
            .map(|pid| (pid, describe(pid)))
            .filter(|(_, command)| !command.contains(INSTALLED_EFFIGY))
            .collect::<Vec<_>>();
        if found.is_empty() {
            return Vec::new();
        }
        for (pid, _) in &found {
            signal(*pid, libc::SIGTERM);
        }
        let remaining = wait_until_gone(found.iter().map(|(pid, _)| *pid), TERM_GRACE);
        for pid in &remaining {
            signal(*pid, libc::SIGKILL);
        }
        let survivors = wait_until_gone(remaining, KILL_GRACE);
        for pid in survivors {
            eprintln!("unreaped test process: {pid} survived SIGKILL");
        }
        found
            .into_iter()
            .map(|(pid, command)| format!("{pid} ({command})"))
            .collect()
    }
}

impl Drop for Sweep {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.marker);
    }
}

/// A fresh token per invocation: unique enough to name one live suite.
fn token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or_default();
    format!(
        "{:x}{:x}{:x}",
        std::process::id(),
        nanos,
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn signal(pid: u32, signal: i32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    // SAFETY: the sweep signals only the PIDs it just found carrying its own
    // suite, and only the two termination signals the grace escalates through.
    unsafe {
        libc::kill(pid, signal);
    }
}

fn alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 only probes for the exact PID the sweep found.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn wait_until_gone(pids: impl IntoIterator<Item = u32>, grace: Duration) -> Vec<u32> {
    let mut remaining = pids
        .into_iter()
        .filter(|pid| alive(*pid))
        .collect::<Vec<_>>();
    let deadline = Instant::now() + grace;
    while !remaining.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        remaining.retain(|pid| alive(*pid));
    }
    remaining
}

/// A PID's command line, for the failure report.
fn describe(pid: u32) -> String {
    let output = std::process::Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output();
    let description = output
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    description.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Live PIDs of this user carrying the suite marker. macOS hides another
/// process's environment, so the inherited descriptor is the discovery
/// primitive there; Linux reads the environment the token names.
#[cfg(target_os = "macos")]
fn holders(marker: &Path, _token: &str) -> Vec<u32> {
    let output = std::process::Command::new("/usr/sbin/lsof")
        .args(["-n", "-w", "-Fp", "--"])
        .arg(marker)
        .output();
    let Ok(output) = output else {
        eprintln!("suite sweep: cannot list marker holders");
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix('p'))
        .filter_map(|pid| pid.parse().ok())
        .collect()
}

#[cfg(target_os = "linux")]
fn holders(_marker: &Path, token: &str) -> Vec<u32> {
    let want = format!("{TOKEN_ENV}={token}");
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut pids = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse().ok())
        else {
            continue;
        };
        let Ok(environment) = std::fs::read(entry.path().join("environ")) else {
            continue;
        };
        if environment
            .split(|byte| *byte == 0)
            .any(|entry| entry == want.as_bytes())
        {
            pids.push(pid);
        }
    }
    pids
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn holders(_marker: &Path, _token: &str) -> Vec<u32> {
    Vec::new()
}
