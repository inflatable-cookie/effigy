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
    let ps_error = match read_process_table_with_ps() {
        Ok(rows) => return Ok(rows),
        Err(error) => error,
    };
    // macOS `/bin/ps` is setuid root, so `sandbox-exec` refuses the `execve`
    // with `EPERM` before the listing program can run at all. Public libproc
    // answers the same four facts in-process, so a supervised run under a
    // Seatbelt profile still enumerates its own tree.
    #[cfg(target_os = "macos")]
    if blocked_setuid_ps(&ps_error) {
        return read_process_table_libproc();
    }
    Err(ps_error)
}

/// Whether `ps` failed because this platform refused to execute it at all.
///
/// macOS `/bin/ps` is setuid root, so a `sandbox-exec` Seatbelt profile refuses
/// `execve` with `EPERM` before the program runs, whatever the profile allows.
#[cfg(target_os = "macos")]
fn blocked_setuid_ps(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
        || error.raw_os_error() == Some(libc::EPERM)
}

#[cfg(unix)]
fn read_process_table_with_ps() -> Result<Vec<ProcessRow>, std::io::Error> {
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

/// Read the same snapshot through public macOS libproc.
///
/// `proc_listallpids` names every PID and `proc_pidinfo(PROC_PIDTBSDINFO)`
/// answers one row's parent, group and status. Neither is setuid, so both work
/// inside `sandbox-exec`. A PID that vanished or belongs to another user is
/// skipped: its row cannot be read, and it cannot be a member or descendant of
/// a process this supervisor started. The caller's own PID must appear, so a
/// wholly broken libproc call is reported rather than read as an empty tree.
#[cfg(target_os = "macos")]
fn read_process_table_libproc() -> Result<Vec<ProcessRow>, std::io::Error> {
    let mut pids = vec![0i32; 1 << 17];
    // SAFETY: `pids` is an aligned writable allocation sized by the byte count
    // passed below, which stays well inside `i32::MAX`.
    let count = unsafe {
        libc::proc_listallpids(
            pids.as_mut_ptr().cast(),
            (pids.len() * std::mem::size_of::<i32>()) as i32,
        )
    };
    if count <= 0 {
        // libproc answers 0 both for an empty inventory and for a failure; the
        // empty inventory is impossible here, so this is a failure.
        let error = std::io::Error::last_os_error();
        return Err(if error.raw_os_error() == Some(0) {
            std::io::Error::other("process inventory unavailable")
        } else {
            error
        });
    }
    if count as usize >= pids.len() {
        return Err(std::io::Error::other("process inventory is truncated"));
    }
    pids.truncate(count as usize);
    let own_pid = std::process::id();
    let mut rows = Vec::with_capacity(pids.len());
    let mut own_pid_seen = false;
    for pid in pids {
        if pid <= 0 {
            continue;
        }
        match read_bsdinfo(pid) {
            Ok(info) => {
                own_pid_seen |= info.pbi_pid == own_pid;
                rows.push(ProcessRow {
                    pid: info.pbi_pid.cast_signed(),
                    parent: info.pbi_ppid.cast_signed(),
                    group: info.pbi_pgid,
                    // `SZOMB` is the kernel's exited-but-unreaped state.
                    zombie: info.pbi_status == libc::SZOMB,
                });
            }
            Err(error) if is_unreadable_row(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    if !own_pid_seen {
        return Err(std::io::Error::other(
            "process inventory did not name the caller",
        ));
    }
    Ok(rows)
}

/// One PID's `proc_bsdinfo`, or the errno that refused it.
#[cfg(target_os = "macos")]
fn read_bsdinfo(pid: i32) -> Result<libc::proc_bsdinfo, std::io::Error> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    // SAFETY: an aligned writable destination of exactly the byte count passed,
    // for a positive PID taken from the inventory. No initialized field is read
    // unless the full structure size was returned.
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size as i32,
        )
    };
    if read != size as i32 {
        return Err(if read <= 0 {
            std::io::Error::last_os_error()
        } else {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "macOS process info returned a partial structure",
            )
        });
    }
    // SAFETY: `proc_pidinfo` returned exactly the initialized destination size.
    let info = unsafe { info.assume_init() };
    if info.pbi_pid != pid as u32 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "macOS process info returned a different PID",
        ));
    }
    Ok(info)
}

/// Whether one PID's unreadable row is expected rather than a broken reader.
///
/// `ESRCH` is a process that exited between the inventory and the read, and
/// `EPERM` is another user's process; neither can be a member or descendant of
/// a process this supervisor started. Errno 0 is libproc refusing to answer
/// with no reason recorded, which is what a process the kernel has already torn
/// down reads as, so it is a row that is not there rather than a broken reader.
/// A wholly broken libproc fails the caller's own row instead.
#[cfg(target_os = "macos")]
fn is_unreadable_row(error: &std::io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(libc::ESRCH) | Some(libc::EPERM) | Some(0)
    )
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

/// A kill failure that is a refusal, or `None` when the target has gone.
///
/// `ESRCH` says the process or group this walk resolved no longer exists. A
/// termination that finds its target already gone has done what it was asked:
/// reporting that as a refusal would tell a caller an achieved `SIGKILL` was
/// rejected, and would turn every race with a self-exiting workload into a
/// failure. Every other failure is a target that refused the signal.
#[cfg(unix)]
fn refusal(error: nix::errno::Errno) -> Option<std::io::Error> {
    let error = std::io::Error::from(error);
    (error.raw_os_error() != Some(nix::libc::ESRCH)).then_some(error)
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
            if let Some(error) = refusal(error) {
                refused = Some(error);
            }
        }
    }
    for process in targets.processes {
        if let Err(error) = kill(Pid::from_raw(process), signal) {
            if let Some(error) = refusal(error) {
                refused = Some(error);
            }
        }
    }
    match (unenumerated, refused) {
        (Some(error), _) => Err(ProcessTreeError::Unenumerated(error)),
        (None, Some(error)) => Err(ProcessTreeError::Refused(error)),
        (None, None) => Ok(()),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use std::os::unix::process::CommandExt;

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
    fn a_target_that_is_already_gone_is_not_a_refusal() {
        // A kill that finds its target gone has done its job: ESRCH is
        // absence, not refusal.
        assert!(refusal(nix::errno::Errno::ESRCH).is_none());
        let refused = refusal(nix::errno::Errno::EPERM).expect("a refused signal");
        assert_eq!(refused.raw_os_error(), Some(nix::libc::EPERM));
    }

    #[test]
    fn terminating_an_absent_target_succeeds_instead_of_reporting_a_refusal() {
        // Above the platform's maximum pid, so no live process can be
        // signalled: the walk resolves the root, the kill answers ESRCH, and
        // there is nothing left to terminate. Before this fix every
        // post-group-kill ESRCH became `Refused`, so this reported a delivered
        // SIGKILL as rejected.
        let absent = (i32::MAX - 1) as u32;
        assert!(terminate_process_tree(absent, true).is_ok());
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

    /// The setuid helper is replaced only when the platform refused to execute
    /// it; a listing that ran and failed stays a failure.
    #[cfg(target_os = "macos")]
    #[test]
    fn only_an_unexecutable_ps_falls_back_to_libproc() {
        assert!(blocked_setuid_ps(&std::io::Error::from_raw_os_error(
            libc::EPERM
        )));
        assert!(blocked_setuid_ps(&std::io::Error::from_raw_os_error(
            libc::EACCES
        )));
        assert!(!blocked_setuid_ps(&std::io::Error::other("exit status 1")));
        assert!(!blocked_setuid_ps(&std::io::Error::from_raw_os_error(
            libc::EAGAIN
        )));
    }

    /// A PID that vanished or belongs to another user is skipped, so one
    /// unreadable row never invalidates the whole snapshot.
    #[cfg(target_os = "macos")]
    #[test]
    fn vanished_and_foreign_rows_are_skipped() {
        assert!(is_unreadable_row(&std::io::Error::from_raw_os_error(
            libc::ESRCH
        )));
        assert!(is_unreadable_row(&std::io::Error::from_raw_os_error(
            libc::EPERM
        )));
        assert!(!is_unreadable_row(&std::io::Error::other("broken reader")));
    }

    /// Public libproc answers the whole table in-process, so supervision keeps
    /// working under `sandbox-exec`, where the setuid `/bin/ps` is refused.
    #[cfg(target_os = "macos")]
    #[test]
    fn libproc_table_names_this_process_its_parent_and_a_child_group() {
        let own_pid = std::process::id();
        let parent_pid = nix::unistd::getppid().as_raw();
        let own_group = nix::unistd::getpgid(None)
            .expect("own process group")
            .as_raw();

        // One live child as the leader of its own group, exactly the shape of
        // an Effigy-owned workload group. `process_group(0)` puts the child in a
        // fresh group whose ID is its own PID.
        let mut child = ProcessCommand::new("sleep")
            .arg("5")
            .process_group(0)
            .spawn()
            .expect("spawn a child in its own group");
        let child_pid = i32::try_from(child.id()).expect("child pid");

        let table = read_process_table_libproc().expect("libproc process table");
        let own = table
            .iter()
            .find(|row| row.pid == own_pid as i32)
            .expect("the snapshot names the caller");
        assert_eq!(own.group, own_group as u32);
        assert!(!own.zombie, "the caller is live");
        assert!(
            table.iter().any(|row| row.pid == parent_pid),
            "the snapshot names the caller's parent {parent_pid}"
        );
        let child_row = table
            .iter()
            .find(|row| row.pid == child_pid)
            .expect("the snapshot names the spawned child");
        assert_eq!(child_row.parent, own_pid as i32);
        assert_eq!(child_row.group, child.id());
        assert!(!child_row.zombie);
        assert_eq!(group_member_ids(&table, child.id()), vec![child.id()]);

        child.kill().expect("stop the child");
        child.wait().expect("reap the child");
    }

    /// An exited child the supervisor has not reaped never counts as a live
    /// group member.
    ///
    /// The kernel answers no `proc_bsdinfo` for a zombie this libproc path can
    /// read, so such a child drops out of the snapshot instead of standing in it
    /// as a live member; a row the kernel does answer during the exit window is
    /// marked by its `SZOMB` status. Either way the group's live membership
    /// excludes it, which is what keeps a retired process group available.
    #[cfg(target_os = "macos")]
    #[test]
    fn libproc_table_excludes_an_exited_unreaped_child_from_live_members() {
        let mut child = ProcessCommand::new("true")
            // Its own group, so its exit is observable as the group emptying.
            .process_group(0)
            .spawn()
            .expect("spawn a child that exits at once");
        let child_pid = i32::try_from(child.id()).expect("child pid");
        let group = child.id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut observed_row = None;
        let mut excluded = false;
        while std::time::Instant::now() < deadline {
            let table = read_process_table_libproc().expect("libproc process table");
            if !group_member_ids(&table, group).contains(&group) {
                observed_row = table.iter().find(|row| row.pid == child_pid).copied();
                excluded = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child.wait().expect("reap the child");
        assert!(
            excluded,
            "an exited unreaped child stops being a live group member"
        );
        assert!(
            observed_row.is_none_or(|row| row.zombie),
            "a row answered for the exited child is marked as a zombie"
        );
    }
}
