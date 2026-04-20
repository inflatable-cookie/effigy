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
