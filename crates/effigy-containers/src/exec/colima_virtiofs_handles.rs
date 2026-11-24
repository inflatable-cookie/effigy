//! A read-only, PID-scoped probe for references that survive the guest cache drop.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[cfg(target_os = "macos")]
const LSOF: &str = "/usr/sbin/lsof";
const VZ_EXECUTABLE: &str =
    "com.apple.Virtualization.VirtualMachine.xpc/Contents/MacOS/com.apple.Virtualization.VirtualMachine";
const SAMPLE_LIMIT: usize = 5;

#[cfg(target_os = "macos")]
pub(super) fn warn_about_surviving_handles(repo_root: &Path, profile: &str) {
    use super::process::run_command_capture_allow_failure_with_deadline;
    use std::time::Duration;

    let Some(disk) = profile_disk(profile) else {
        return;
    };
    let Ok(root) = repo_root.canonicalize() else {
        return;
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    let candidates = vz_processes(deadline);
    let survivors = probe(&root, &disk, deadline, &candidates, |args, deadline| {
        run_command_capture_allow_failure_with_deadline(
            Path::new("/"),
            LSOF,
            args,
            "inspect Colima VirtioFS handles",
            Some(deadline),
        )
        .ok()
        .map(|output| output.stdout)
    });
    for (pid, handles) in survivors {
        // Stderr keeps the warning visible in both text and JSON invocations.
        eprintln!("{}", handles.warning(pid, &root, profile));
    }
}

#[cfg(target_os = "macos")]
fn profile_disk(profile: &str) -> Option<PathBuf> {
    // Colima maps the default profile to `colima` and strips a redundant
    // `colima-` prefix from other names.
    let instance = match profile {
        "default" | "colima" => "colima".to_owned(),
        name if !name.is_empty()
            && !name.starts_with('-')
            && name
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || b"-_.".contains(&ch)) =>
        {
            format!("colima-{}", name.strip_prefix("colima-").unwrap_or(name))
        }
        _ => return None,
    };
    let lima_home = std::env::var_os("LIMA_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| colima_home().map(|home| home.join("_lima")))?;
    let instance = lima_home.join(instance);
    // Lima switched the primary disk name from diffdisk to disk. Migrated
    // instances can have a disk symlink, so use the actual vnode path.
    ["disk", "diffdisk"]
        .iter()
        .filter_map(|name| instance.join(name).canonicalize().ok())
        .find(|path| path.is_file())
}

#[cfg(target_os = "macos")]
fn colima_home() -> Option<PathBuf> {
    let explicit = std::env::var_os("COLIMA_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let conventional = home.as_ref().map(|home| home.join(".colima"));
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".config")))
        .map(|path| path.join("colima"));
    [explicit, conventional, xdg]
        .into_iter()
        .flatten()
        .find(|path| path.is_dir())
}

#[cfg(target_os = "macos")]
fn vz_processes(deadline: Instant) -> Vec<u32> {
    use effigy_kernel::ident::process_executable;
    use nix::libc;

    // Enumerate process IDs and executable paths only. Open descriptor
    // enumeration is reserved for exact Virtualization service candidates.
    let mut pids = vec![0_i32; 16_384];
    let bytes = std::mem::size_of_val(pids.as_slice()) as i32;
    // SAFETY: pids is a writable, aligned buffer of exactly bytes bytes.
    let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    if count <= 0 || count as usize >= pids.len() {
        return Vec::new();
    }
    pids.truncate(count as usize);
    pids.into_iter()
        .take_while(|_| Instant::now() < deadline)
        .filter(|pid| *pid > 1)
        .map(|pid| pid as u32)
        .filter(|&pid| process_executable(pid).is_ok_and(|path| is_vz_executable(&path)))
        .collect()
}

fn is_vz_executable(path: &Path) -> bool {
    path.starts_with("/System/Library/Frameworks/Virtualization.framework")
        && path.ends_with(VZ_EXECUTABLE)
}

fn probe(
    root: &Path,
    disk: &Path,
    deadline: Instant,
    candidates: &[u32],
    mut capture: impl FnMut(&[&str], Instant) -> Option<Vec<u8>>,
) -> BTreeMap<u32, Survivors> {
    let mut capture = |args: &[&str]| {
        if Instant::now() >= deadline {
            return Vec::new();
        }
        capture(args, deadline).unwrap_or_default()
    };
    let Some(disk_name) = disk.to_str().filter(|_| !candidates.is_empty()) else {
        return BTreeMap::new();
    };

    // The VZ XPC service can be launched by launchd. Lima's ha.pid and vz.pid
    // name the driver, so PPID or a PID file alone cannot identify the holder.
    // Match the running service by its open profile disk before reading its
    // descriptor paths. Both lsof calls are restricted to named PIDs.
    let candidate_list = pid_list(candidates.iter().copied());
    let attached = capture(&[
        "-nP",
        "-X",
        "-a",
        "-p",
        &candidate_list,
        "-F0p",
        "-f",
        "--",
        disk_name,
    ]);
    let owned: Vec<_> = fields(&attached)
        .filter(|(tag, _)| *tag == b'p')
        .filter_map(|(_, value)| number(value))
        .filter(|pid| candidates.contains(pid))
        .collect();
    if owned.is_empty() {
        return BTreeMap::new();
    }
    let pids = pid_list(owned.iter().copied());
    let descriptors = capture(&["-nP", "-X", "-p", &pids, "-F0pftn"]);
    workspace_handles(&descriptors, &owned, root, disk)
}

fn pid_list(pids: impl Iterator<Item = u32>) -> String {
    pids.map(|pid| pid.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn number(bytes: &[u8]) -> Option<u32> {
    std::str::from_utf8(bytes).ok()?.trim().parse().ok()
}

// With -F0, lsof separates fields with NUL and ends each record with LF.
fn fields(output: &[u8]) -> impl Iterator<Item = (u8, &[u8])> {
    output
        .split_inclusive(|&byte| byte == 0)
        .filter_map(|field| {
            let field = field.strip_suffix(&[0])?;
            let field = field.strip_prefix(b"\n").unwrap_or(field);
            let (&tag, value) = field.split_first()?;
            Some((tag, value))
        })
}

#[derive(Default)]
struct Survivors {
    count: usize,
    paths: Vec<PathBuf>,
}

impl Survivors {
    fn record(&mut self, path: &Path) {
        self.count += 1;
        if self.paths.len() < SAMPLE_LIMIT && !self.paths.iter().any(|sample| sample == path) {
            self.paths.push(path.to_owned());
        }
    }

    fn warning(&self, pid: u32, root: &Path, profile: &str) -> String {
        let paths = self
            .paths
            .iter()
            .map(|path| format!("`{}`", path.display().to_string().escape_debug()))
            .collect::<Vec<_>>()
            .join(", ");
        let root = root.display().to_string();
        let handles = if self.count == 1 { "handle" } else { "handles" };
        format!(
            "[warn] Colima profile `{profile}` still has {} open VirtioFS {handles} under `{}` after teardown (Virtualization PID {pid}). Paths include {paths}. Run `colima restart {profile}` to release them.",
            self.count,
            root.escape_debug(),
        )
    }
}

fn workspace_handles(
    output: &[u8],
    pids: &[u32],
    root: &Path,
    disk: &Path,
) -> BTreeMap<u32, Survivors> {
    let mut scans = BTreeMap::<u32, (bool, Survivors)>::new();
    let (mut pid, mut descriptor, mut file_type) = (None, false, &b""[..]);
    for (tag, value) in fields(output) {
        match tag {
            b'p' => {
                pid = number(value).filter(|pid| pids.contains(pid));
                descriptor = false;
            }
            b'f' => {
                descriptor = number(value).is_some();
                file_type = b"";
            }
            b't' => file_type = value,
            b'n' if descriptor && matches!(file_type, b"DIR" | b"REG") => {
                if let Some(pid) = pid {
                    let path = Path::new(OsStr::from_bytes(value));
                    let (disk_open, handles) = scans.entry(pid).or_default();
                    if file_type == b"REG" && path == disk {
                        // Recheck attribution in the same snapshot as the
                        // workspace descriptors, in case a PID was reused.
                        *disk_open = true;
                    } else if path.starts_with(root) {
                        handles.record(path);
                    }
                }
            }
            _ => {}
        }
    }
    scans
        .into_iter()
        .filter_map(|(pid, (disk_open, handles))| {
            (disk_open && handles.count > 0).then_some((pid, handles))
        })
        .collect()
}

#[cfg(test)]
mod tests;
