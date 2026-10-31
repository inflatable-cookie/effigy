use std::process::Child;

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::{getpgid, Pid};

#[cfg(unix)]
use std::collections::{HashMap, HashSet};
#[cfg(unix)]
use std::process::Command as ProcessCommand;

pub(super) fn send_terminate(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = signal_process_tree(child, Signal::SIGTERM);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

pub(super) fn send_kill(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = signal_process_tree(child, Signal::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

/// Whether `signal` is one a stop sends: a cancellation, not a crash.
///
/// Contract 052 names a cancellation by the signal a stop delivered, so a
/// process that dies from one of these was ended rather than failing on its
/// own. Any other signal is the process's own crash.
pub fn interrupted_signal(signal: i32) -> bool {
    matches!(
        signal,
        nix::libc::SIGHUP | nix::libc::SIGINT | nix::libc::SIGTERM | nix::libc::SIGKILL
    )
}

pub fn process_is_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        pid > 0 && kill(Pid::from_raw(pid as i32), None).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Snapshot live members of one Unix process group. This is used only for a
/// child group created by Effigy and recorded with its leader's start identity;
/// a group ID remains reserved while any live member is still present.
///
/// An exited process the kernel still lists as a zombie is not a member: it
/// holds no socket, route or work, and it disappears from the group the moment
/// its parent reaps it. Counting it would keep a generation unavailable after
/// the only process in its group had already gone.
pub fn process_group_member_ids(process_group_id: u32) -> Result<Vec<u32>, std::io::Error> {
    #[cfg(unix)]
    {
        if process_group_id == 0 || process_group_id > i32::MAX as u32 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "process group ID is outside the supported range",
            ));
        }
        let output = ProcessCommand::new("ps")
            .args(["-Ao", "pid=,pgid=,stat="])
            .output()?;
        if !output.status.success() {
            return Err(std::io::Error::other(
                "process group listing command failed",
            ));
        }
        let mut members = Vec::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut parts = line.split_whitespace();
            let Some(pid) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "process group listing contains a malformed PID",
                ));
            };
            let Some(pgid) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "process group listing contains a malformed group ID",
                ));
            };
            let Some(state) = parts.next() else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "process group listing contains a malformed process state",
                ));
            };
            // `ps` state starts with `Z` only for a zombie, which is already
            // exited and cannot hold anything the group reserves.
            if pid > 0 && pgid == process_group_id && !state.starts_with('Z') {
                members.push(pid as u32);
            }
        }
        members.sort_unstable();
        Ok(members)
    }
    #[cfg(not(unix))]
    {
        let _ = process_group_id;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "process group inspection is unsupported on this platform",
        ))
    }
}

/// Snapshot the live members of one Unix process group other than its leader.
///
/// The leader of an Effigy-owned group is its group ID, and once that leader has
/// exited it is never a surviving member of its own group: nothing it owned is
/// still reserved. Only a live process other than the leader keeps the group
/// (and the port, route and generation it stands for) unavailable.
pub fn surviving_group_member_ids(process_group_id: u32) -> Result<Vec<u32>, std::io::Error> {
    let mut members = process_group_member_ids(process_group_id)?;
    members.retain(|pid| *pid != process_group_id);
    Ok(members)
}

pub fn terminate_process_tree(pid: u32, force: bool) {
    #[cfg(unix)]
    {
        if pid == 0 {
            return;
        }
        let signal = if force {
            Signal::SIGKILL
        } else {
            Signal::SIGTERM
        };
        let targets = signal_targets_for_child(pid as i32);
        for pgid in targets.groups {
            let _ = kill(Pid::from_raw(-pgid), signal);
        }
        for process in targets.processes {
            let _ = kill(Pid::from_raw(process), signal);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, force);
    }
}

#[cfg(unix)]
fn signal_process_tree(child: &mut Child, signal: Signal) -> Result<(), nix::Error> {
    let pid = child.id() as i32;
    if pid <= 0 {
        return Ok(());
    }

    let targets = signal_targets_for_child(pid);
    let mut last_error = None;

    for pgid in targets.groups {
        if let Err(error) = kill(Pid::from_raw(-pgid), signal) {
            last_error = Some(error);
        }
    }
    for pid in targets.processes {
        if let Err(error) = kill(Pid::from_raw(pid), signal) {
            last_error = Some(error);
        }
    }

    last_error.map_or(Ok(()), Err)
}

#[cfg(unix)]
struct SignalTargets {
    groups: Vec<i32>,
    processes: Vec<i32>,
}

#[cfg(unix)]
fn signal_targets_for_child(root_pid: i32) -> SignalTargets {
    let descendants = process_descendants(root_pid);
    let mut groups = HashSet::new();
    let mut processes = HashSet::new();

    let mut ordered_pids = descendants;
    ordered_pids.push(root_pid);
    ordered_pids.sort_unstable();
    ordered_pids.dedup();

    for pid in &ordered_pids {
        processes.insert(*pid);
        if let Ok(group) = getpgid(Some(Pid::from_raw(*pid))) {
            let raw = group.as_raw();
            if raw > 0 {
                groups.insert(raw);
            }
        }
    }

    let mut groups = groups.into_iter().collect::<Vec<_>>();
    groups.sort_unstable();
    let mut processes = processes.into_iter().collect::<Vec<_>>();
    processes.sort_unstable_by(|left, right| right.cmp(left));

    SignalTargets { groups, processes }
}

#[cfg(unix)]
fn process_descendants(root_pid: i32) -> Vec<i32> {
    let output = ProcessCommand::new("ps")
        .args(["-Ao", "pid=,ppid="])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let rendered = String::from_utf8_lossy(&output.stdout);
    let mut children_by_parent: HashMap<i32, Vec<i32>> = HashMap::new();
    for line in rendered.lines() {
        let mut parts = line.split_whitespace();
        let Some(pid) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };
        let Some(ppid) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };
        children_by_parent.entry(ppid).or_default().push(pid);
    }

    let mut descendants = Vec::new();
    let mut stack = children_by_parent.remove(&root_pid).unwrap_or_default();
    while let Some(pid) = stack.pop() {
        descendants.push(pid);
        if let Some(children) = children_by_parent.remove(&pid) {
            stack.extend(children);
        }
    }

    descendants
}
