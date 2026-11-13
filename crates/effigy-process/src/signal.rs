//! Best-effort supervision signals for one owned child and the processes it
//! started.
//!
//! This module owns supervision signalling: the supervisor holds the direct
//! child, proves its exit with `try_wait`, and reaches alongside it the
//! processes it forked. Identity-evidenced custody signals are a different
//! obligation and live in `effigy_kernel::custody::platform`, which opens a
//! pidfd, checks the recorded generation and reports `Observed` verdicts.
//! Nothing here decides closure or claims authority over a process this
//! supervisor did not start.
//!
//! One process-table snapshot answers both questions the tree walk asks: which
//! live processes belong to a group, and which processes descend from a root.
//! Reading it twice could see two different trees.

use std::process::Child;

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::{getpgid, Pid};

#[cfg(unix)]
use std::collections::HashMap;
#[cfg(unix)]
use std::process::Command as ProcessCommand;

/// Why a supervision signal could not be shown to reach every owned descendant.
///
/// A tree termination reports this instead of returning as if the whole tree
/// had been signalled: a listed descendant that left the root's group is only
/// reachable through the process table.
#[derive(Debug)]
pub enum ProcessTreeError {
    /// The process table could not be read. The root's own process group was
    /// still signalled, but a descendant that changed group was not reached.
    Unenumerated(std::io::Error),
    /// A resolved target refused the signal. Earlier targets may already have
    /// received it.
    Refused(std::io::Error),
}

impl ProcessTreeError {
    /// Whether the failure is a missing process table rather than a refused
    /// signal, so the caller can re-observe the tree instead of re-signalling.
    pub fn is_unenumerated(&self) -> bool {
        matches!(self, Self::Unenumerated(_))
    }
}

impl std::fmt::Display for ProcessTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unenumerated(error) => write!(
                f,
                "process table unavailable ({error}), so a descendant outside the root's group was not signalled"
            ),
            Self::Refused(error) => write!(f, "a process in the tree refused the signal ({error})"),
        }
    }
}

impl std::error::Error for ProcessTreeError {}

pub(super) fn send_terminate(child: &mut Child) -> Result<(), ProcessTreeError> {
    #[cfg(unix)]
    {
        signal_process_tree(child.id() as i32, Signal::SIGTERM)
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        Ok(())
    }
}

pub(super) fn send_kill(child: &mut Child) -> Result<(), ProcessTreeError> {
    #[cfg(unix)]
    {
        signal_process_tree(child.id() as i32, Signal::SIGKILL)
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        Ok(())
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
        if !in_group_range(process_group_id) {
            return Err(invalid_group(process_group_id));
        }
        let table = read_process_table()?;
        Ok(group_member_ids(&table, process_group_id))
    }
    #[cfg(not(unix))]
    {
        let _ = process_group_id;
        Err(unsupported_group_inspection())
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

/// Terminate `pid` and the processes it started. A failed process-table read
/// still signals the root's own group, and is reported rather than read as an
/// empty tree.
pub fn terminate_process_tree(pid: u32, force: bool) -> Result<(), ProcessTreeError> {
    #[cfg(unix)]
    {
        if pid == 0 {
            return Ok(());
        }
        let signal = if force {
            Signal::SIGKILL
        } else {
            Signal::SIGTERM
        };
        signal_process_tree(pid as i32, signal)
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, force);
        Ok(())
    }
}

/// One row of the process table: the four facts a tree walk reads.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProcessRow {
    pid: i32,
    parent: i32,
    group: u32,
    /// A zombie has exited: it can hold nothing and cannot fork.
    zombie: bool,
}

#[cfg(unix)]
fn in_group_range(group: u32) -> bool {
    group > 0 && group <= i32::MAX as u32
}

#[cfg(not(unix))]
fn unsupported_group_inspection() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "process group inspection is unsupported on this platform",
    )
}

#[cfg(unix)]
fn invalid_group(group: u32) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("process group ID {group} is outside the supported range"),
    )
}

/// Read one snapshot of the live process table.
#[cfg(unix)]
fn read_process_table() -> Result<Vec<ProcessRow>, std::io::Error> {
    let output = ProcessCommand::new("ps")
        .args(["-Ao", "pid=,ppid=,pgid=,stat="])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(
            "process table listing command failed",
        ));
    }
    parse_process_table(&String::from_utf8_lossy(&output.stdout))
}

/// Parse `ps -Ao pid=,ppid=,pgid=,stat=` output.
///
/// A row the walk cannot read makes the whole snapshot unusable: skipping it
/// would silently drop a member or a descendant from every answer derived from
/// this snapshot.
#[cfg(unix)]
fn parse_process_table(rendered: &str) -> Result<Vec<ProcessRow>, std::io::Error> {
    let mut rows = Vec::new();
    for line in rendered.lines() {
        let mut parts = line.split_whitespace();
        let pid = parts
            .next()
            .and_then(|value| value.parse::<i32>().ok())
            .ok_or_else(malformed_table)?;
        let parent = parts
            .next()
            .and_then(|value| value.parse::<i32>().ok())
            .ok_or_else(malformed_table)?;
        let group = parts
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(malformed_table)?;
        let state = parts.next().ok_or_else(malformed_table)?;
        rows.push(ProcessRow {
            pid,
            parent,
            group,
            // `ps` state starts with `Z` only for a zombie.
            zombie: state.starts_with('Z'),
        });
    }
    Ok(rows)
}

#[cfg(unix)]
fn malformed_table() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "process table listing contains a malformed row",
    )
}

/// The live, non-leader-filtered members of `group` in one snapshot.
#[cfg(unix)]
fn group_member_ids(table: &[ProcessRow], group: u32) -> Vec<u32> {
    let mut members: Vec<u32> = table
        .iter()
        .filter(|row| row.pid > 0 && row.group == group && !row.zombie)
        .map(|row| row.pid as u32)
        .collect();
    members.sort_unstable();
    members
}

/// Every live descendant of `root` in one snapshot, depth-first.
#[cfg(unix)]
fn descendant_ids(table: &[ProcessRow], root: i32) -> Vec<i32> {
    let mut children_by_parent: HashMap<i32, Vec<i32>> = HashMap::new();
    for row in table {
        if row.pid > 0 {
            children_by_parent
                .entry(row.parent)
                .or_default()
                .push(row.pid);
        }
    }
    let mut descendants = Vec::new();
    let mut stack = children_by_parent.remove(&root).unwrap_or_default();
    while let Some(pid) = stack.pop() {
        descendants.push(pid);
        if let Some(children) = children_by_parent.remove(&pid) {
            stack.extend(children);
        }
    }
    descendants
}

#[cfg(unix)]
struct SignalTargets {
    groups: Vec<i32>,
    processes: Vec<i32>,
}

/// Turn one snapshot read into targets and, when the read failed, the fact that
/// the tree could not be fully enumerated. The root and its own group survive a
/// failed read; a descendant outside that group does not, and the caller says so.
#[cfg(unix)]
fn signal_targets_after_read(
    root_pid: i32,
    table: Result<Vec<ProcessRow>, std::io::Error>,
) -> (SignalTargets, Option<std::io::Error>) {
    match table {
        Ok(table) => (signal_targets(root_pid, &table), None),
        Err(error) => (signal_targets(root_pid, &[]), Some(error)),
    }
}

#[cfg(unix)]
fn signal_targets(root_pid: i32, table: &[ProcessRow]) -> SignalTargets {
    let mut groups = std::collections::HashSet::new();
    let mut processes = std::collections::HashSet::new();

    let mut ordered_pids = descendant_ids(table, root_pid);
    ordered_pids.push(root_pid);
    ordered_pids.sort_unstable();
    ordered_pids.dedup();

    for pid in &ordered_pids {
        processes.insert(*pid);
        if let Some(row) = table.iter().find(|row| row.pid == *pid) {
            if row.group > 0 {
                groups.insert(row.group as i32);
            }
        }
    }
    // The root's group is an OS fact, not a table row: ask even when the table
    // is empty or unreadable so the fallback signal still reaches the group.
    if let Ok(group) = getpgid(Some(Pid::from_raw(root_pid))) {
        if group.as_raw() > 0 {
            groups.insert(group.as_raw());
        }
    }

    let mut groups = groups.into_iter().collect::<Vec<_>>();
    groups.sort_unstable();
    let mut processes = processes.into_iter().collect::<Vec<_>>();
    processes.sort_unstable_by(|left, right| right.cmp(left));

    SignalTargets { groups, processes }
}

#[cfg(unix)]
fn signal_process_tree(root_pid: i32, signal: Signal) -> Result<(), ProcessTreeError> {
    if root_pid <= 0 {
        return Ok(());
    }
    let (targets, unenumerated) = signal_targets_after_read(root_pid, read_process_table());
    let mut refused = None;
    for group in targets.groups {
        if let Err(error) = kill(Pid::from_raw(-group), signal) {
            refused = Some(error);
        }
    }
    for process in targets.processes {
        if let Err(error) = kill(Pid::from_raw(process), signal) {
            refused = Some(error);
        }
    }
    match (unenumerated, refused) {
        (Some(error), _) => Err(ProcessTreeError::Unenumerated(error)),
        (None, Some(error)) => Err(ProcessTreeError::Refused(error.into())),
        (None, None) => Ok(()),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn row(pid: i32, parent: i32, group: u32, zombie: bool) -> ProcessRow {
        ProcessRow {
            pid,
            parent,
            group,
            zombie,
        }
    }

    #[test]
    fn one_snapshot_answers_group_members_and_excludes_zombies() {
        let table = [
            row(10, 1, 10, false),
            row(11, 10, 10, false),
            row(12, 10, 10, true),
            row(13, 1, 13, false),
        ];
        assert_eq!(group_member_ids(&table, 10), vec![10, 11]);
        assert_eq!(group_member_ids(&table, 13), vec![13]);
        assert!(group_member_ids(&table, 99).is_empty());
    }

    #[test]
    fn one_snapshot_answers_the_descendant_walk() {
        let table = [
            row(10, 1, 10, false),
            row(11, 10, 10, false),
            row(12, 11, 10, false),
            row(13, 1, 13, false),
        ];
        let mut descendants = descendant_ids(&table, 10);
        descendants.sort_unstable();
        assert_eq!(descendants, vec![11, 12]);
        assert!(descendant_ids(&table, 13).is_empty());
    }

    #[test]
    fn a_malformed_row_refuses_the_whole_snapshot() {
        // A skipped row would drop a member or a descendant from both answers.
        assert!(parse_process_table("10 one 10 Ss\n").is_err());
        assert!(parse_process_table("10 1 notagroup Ss\n").is_err());
        assert!(parse_process_table("10 1 10\n").is_err());
    }

    #[test]
    fn a_zombie_row_parses_as_not_live() {
        let table = parse_process_table("10 1 10 Z+\n11 10 10 S\n").unwrap();
        assert!(table[0].zombie);
        assert!(!table[1].zombie);
    }

    #[test]
    fn an_unreadable_process_table_is_reported_instead_of_an_empty_tree() {
        // Before this seam a failed read produced an empty descendant set,
        // which read as "the root has no descendants" and narrowed the
        // termination with no error. Now the root stays a target and the
        // failure is stated.
        let (targets, error) =
            signal_targets_after_read(i32::MAX - 1, Err(std::io::Error::other("ps unavailable")));
        assert_eq!(targets.processes, vec![i32::MAX - 1]);
        let error = ProcessTreeError::Unenumerated(error.expect("the read failure is reported"));
        assert!(error.is_unenumerated());
        assert!(error.to_string().contains("not signalled"));
    }
}
