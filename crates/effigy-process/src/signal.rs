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

pub fn process_is_descendant_of(pid: u32, ancestor_pid: u32) -> bool {
    #[cfg(unix)]
    {
        pid > 0
            && ancestor_pid > 0
            && process_descendant_ids(ancestor_pid)
                .is_ok_and(|descendants| descendants.contains(&pid))
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, ancestor_pid);
        false
    }
}

/// Snapshot the current descendants of a process using the host process
/// table. An unavailable or malformed process listing is an error, not an
/// empty tree, so ownership callers can refuse uncertain results.
pub fn process_descendant_ids(ancestor_pid: u32) -> Result<Vec<u32>, std::io::Error> {
    #[cfg(unix)]
    {
        if ancestor_pid == 0 || ancestor_pid > i32::MAX as u32 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "ancestor PID is outside the supported range",
            ));
        }
        let output = ProcessCommand::new("ps")
            .args(["-Ao", "pid=,ppid="])
            .output()?;
        if !output.status.success() {
            return Err(std::io::Error::other("process listing command failed"));
        }
        let mut children_by_parent: HashMap<i32, Vec<i32>> = HashMap::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut parts = line.split_whitespace();
            let Some(pid) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "process listing contains a malformed PID",
                ));
            };
            let Some(ppid) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "process listing contains a malformed parent PID",
                ));
            };
            if pid > 0 && ppid > 0 {
                children_by_parent.entry(ppid).or_default().push(pid);
            }
        }
        let mut descendants = Vec::new();
        let mut stack = children_by_parent
            .remove(&(ancestor_pid as i32))
            .unwrap_or_default();
        while let Some(pid) = stack.pop() {
            descendants.push(pid as u32);
            if let Some(children) = children_by_parent.remove(&pid) {
                stack.extend(children);
            }
        }
        descendants.sort_unstable();
        Ok(descendants)
    }
    #[cfg(not(unix))]
    {
        let _ = ancestor_pid;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "process descendant inspection is unsupported on this platform",
        ))
    }
}

/// Snapshot live members of one Unix process group. This is used only for a
/// child group created by Effigy and recorded with its leader's start identity;
/// a group ID remains reserved while any member is still present.
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
            .args(["-Ao", "pid=,pgid="])
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
            if pid > 0 && pgid == process_group_id {
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
