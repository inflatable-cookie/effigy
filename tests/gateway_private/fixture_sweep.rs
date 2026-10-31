//! Owned-identity cleanup for the processes a fixture starts.
//!
//! A fixture records each generation it starts (listener children and gateway
//! daemons) with the boot and start identity Effigy wrote for it. This module
//! signals only processes that still hold those identities:
//!
//! - the recorded leader, while it holds its identity;
//! - the members of its process group that began after it, while it holds its
//!   identity (the group window only exists while the leader lives);
//! - members captured while the leader was alive. A captured member keeps its
//!   own identity, so it is still stopped after the leader exits.
//!
//! A leader that has exited proves no group window, so a process that was never
//! captured and only shares the old group id is left alone. A PID the kernel
//! reused has a different start identity, so it is never signalled.
//!
//! A fixture's watchdog outlives the fixture if the fixture is killed, which is
//! the only cleanup an interrupted owner gets: no `Drop` runs in a killed
//! process.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use effigy_gateway::identity::{
    read_live_process_identity, GatewayStartIdentity, LiveProcessIdentity,
};

const WATCHDOG_ROOT_ENV: &str = "EFFIGY_FIXTURE_WATCHDOG_ROOT";
const WATCHDOG_OWNER_ENV: &str = "EFFIGY_FIXTURE_WATCHDOG_OWNER";
const WATCHDOG_TEST: &str = "fixture_sweep::fixture_watchdog_helper";
const WATCHDOG_INTERVAL: Duration = Duration::from_millis(200);

/// A process that was in a recorded leader's group, with its identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Member {
    pub(super) pid: u32,
    pub(super) boot: String,
    pub(super) start: GatewayStartIdentity,
}

/// A recorded generation: its leader's PID and identity, and the members of
/// its group captured while the leader was alive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Record {
    pub(super) pid: u32,
    pub(super) boot: String,
    pub(super) start: GatewayStartIdentity,
    pub(super) members: Vec<Member>,
}

/// Process IDs whose process group is `pgid`, from the process table.
pub(super) fn process_group_members(pgid: u32) -> Vec<u32> {
    let Ok(output) = Command::new("ps").args(["-axo", "pid=,pgid="]).output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line
                .split_whitespace()
                .map(|field| field.parse::<u32>().ok());
            let pid = fields.next()??;
            let group = fields.next()??;
            (group == pgid).then_some(pid)
        })
        .collect()
}

fn live(pid: u32) -> Option<LiveProcessIdentity> {
    read_live_process_identity(pid).ok()
}

fn holds(live: &LiveProcessIdentity, boot: &str, start: &GatewayStartIdentity) -> bool {
    live.boot_identity == boot && live.start_identity == *start
}

/// Whether `pid` is a zombie: it has exited and only waits to be reaped.
fn is_zombie(pid: u32) -> bool {
    Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .starts_with('Z')
        })
        .unwrap_or(false)
}

/// Whether `pid` still holds `boot` and `start` and has not exited.
fn holds_alive(pid: u32, boot: &str, start: &GatewayStartIdentity) -> bool {
    live(pid).is_some_and(|live| holds(&live, boot, start)) && !is_zombie(pid)
}

/// Whether `process` began at or after the record's leader in the same boot.
fn began_at_or_after(process: &LiveProcessIdentity, leader: &Record) -> bool {
    if process.boot_identity != leader.boot {
        return false;
    }
    match (&process.start_identity, &leader.start) {
        (
            GatewayStartIdentity::Linux { start_ticks: now },
            GatewayStartIdentity::Linux { start_ticks: then },
        ) => now >= then,
        (
            GatewayStartIdentity::Macos {
                start_seconds: now_seconds,
                start_microseconds: now_micros,
            },
            GatewayStartIdentity::Macos {
                start_seconds: then_seconds,
                start_microseconds: then_micros,
            },
        ) => (now_seconds, now_micros) >= (then_seconds, then_micros),
        _ => false,
    }
}

/// The generation records under one checkout's listener state, and the gateway
/// daemon records under `gateway_root`. Reads state only.
fn read_records(checkouts: &[PathBuf], gateway_root: &Path) -> Vec<Record> {
    let mut found = Vec::new();
    for checkout in checkouts {
        listener_records(&checkout.join(".effigy/runtime/host-processes"), &mut found);
    }
    gateway_records(gateway_root, &mut found);
    found
}

fn listener_records(dir: &Path, found: &mut Vec<Record>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            listener_records(&path, found);
        } else if path.to_string_lossy().ends_with(".listener.json") {
            let Some(state) = fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            else {
                continue;
            };
            push_record(
                found,
                state["child_pid"].as_u64(),
                state["child_boot_identity"].as_str(),
                state["child_start_identity"].clone(),
            );
        }
    }
}

fn gateway_records(dir: &Path, found: &mut Vec<Record>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            gateway_records(&path, found);
        } else if path
            .file_name()
            .is_some_and(|name| name == "gateway.identity")
        {
            let Some(record) = fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            else {
                continue;
            };
            push_record(
                found,
                record["pid"].as_u64(),
                record["boot_identity"].as_str(),
                record["start_identity"].clone(),
            );
        }
    }
}

fn push_record(
    found: &mut Vec<Record>,
    pid: Option<u64>,
    boot: Option<&str>,
    start: serde_json::Value,
) {
    let (Some(pid), Some(boot)) = (pid, boot) else {
        return;
    };
    let Ok(start) = serde_json::from_value::<GatewayStartIdentity>(start) else {
        return;
    };
    found.push(Record {
        pid: pid as u32,
        boot: boot.to_owned(),
        start,
        members: Vec::new(),
    });
}

/// The checkouts a fixture root holds: its direct subdirectories.
fn checkouts_under(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

/// Captures the members of `record`'s group that began after the leader. Only
/// a leader that still holds its identity opens that window; members captured
/// earlier stay recorded with their own identities.
fn capture_members(record: &mut Record) {
    let Some(leader) = live(record.pid) else {
        return;
    };
    if !holds(&leader, &record.boot, &record.start) {
        return;
    }
    for pid in process_group_members(record.pid) {
        if pid == record.pid || record.members.iter().any(|member| member.pid == pid) {
            continue;
        }
        let Some(process) = live(pid) else {
            continue;
        };
        if began_at_or_after(&process, record) {
            record.members.push(Member {
                pid,
                boot: process.boot_identity,
                start: process.start_identity,
            });
        }
    }
}

/// The records under `checkouts` and `gateway_root`, with each group's members
/// captured while its leader still holds its identity. Take this before a stop
/// that may clear a listener's own record.
pub(super) fn snapshot(checkouts: &[PathBuf], gateway_root: &Path) -> Vec<Record> {
    let mut records = read_records(checkouts, gateway_root);
    for record in records.iter_mut() {
        capture_members(record);
    }
    records
}

/// `snapshot` for a fixture root: its checkouts and gateway state under it.
pub(super) fn snapshot_root(root: &Path) -> Vec<Record> {
    snapshot(&checkouts_under(root), root)
}

/// Signals one record's owned processes: the leader when it holds its identity
/// and the members that began after it while it still does, plus every
/// captured member whose identity still holds.
fn terminate_record(record: &Record) {
    let mut owned = Vec::new();
    if live(record.pid).is_some_and(|leader| holds(&leader, &record.boot, &record.start)) {
        owned.push(record.pid);
        for pid in process_group_members(record.pid) {
            if pid != record.pid && live(pid).is_some_and(|p| began_at_or_after(&p, record)) {
                owned.push(pid);
            }
        }
    }
    for member in &record.members {
        if live(member.pid).is_some_and(|p| holds(&p, &member.boot, &member.start)) {
            owned.push(member.pid);
        }
    }
    owned.sort_unstable();
    owned.dedup();
    for pid in owned {
        eprintln!("fixture sweep stopped process {pid} it owned");
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}

/// Stops the owned processes of the given snapshot records.
pub(super) fn terminate_records(records: &[Record]) {
    for record in records {
        terminate_record(record);
    }
}

/// Stops every generation under a fixture root, capturing group members while
/// their leaders still hold their identities.
pub(super) fn terminate_root(root: &Path) {
    terminate_records(&snapshot_root(root));
}

/// Whether any record still holds a live leader or a live captured member.
pub(super) fn any_alive(records: &[Record]) -> bool {
    records.iter().any(|record| {
        holds_alive(record.pid, &record.boot, &record.start)
            || record
                .members
                .iter()
                .any(|member| holds_alive(member.pid, &member.boot, &member.start))
    })
}

/// Starts the fixture watchdog for `root`, owned by this process. If this
/// process is killed, the watchdog stops what it recorded, by identity.
pub(super) fn spawn_watchdog(root: &Path) -> Child {
    use std::os::unix::process::CommandExt;
    Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", WATCHDOG_TEST, "--ignored", "--nocapture"])
        .env(WATCHDOG_ROOT_ENV, root)
        .env(WATCHDOG_OWNER_ENV, std::process::id().to_string())
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the fixture watchdog")
}

/// The fixture watchdog body. It keeps the owner's generations and their group
/// members captured while the owner lives, then stops them once the owner is
/// gone. It runs only as the child of a fixture.
#[test]
#[ignore = "started by a fixture as its watchdog"]
fn fixture_watchdog_helper() {
    let root = PathBuf::from(std::env::var_os(WATCHDOG_ROOT_ENV).expect("watchdog root"));
    let owner: u32 = std::env::var(WATCHDOG_OWNER_ENV)
        .expect("watchdog owner")
        .parse()
        .expect("watchdog owner pid");
    let mut known: Vec<Record> = Vec::new();
    while nix::sys::signal::kill(nix::unistd::Pid::from_raw(owner as i32), None).is_ok() {
        for fresh in snapshot_root(&root) {
            let position = known.iter().position(|record| {
                record.pid == fresh.pid && record.boot == fresh.boot && record.start == fresh.start
            });
            match position {
                Some(index) => {
                    for member in fresh.members {
                        if !known[index].members.contains(&member) {
                            known[index].members.push(member);
                        }
                    }
                }
                None => known.push(fresh),
            }
        }
        for record in known.iter_mut() {
            capture_members(record);
        }
        thread::sleep(WATCHDOG_INTERVAL);
    }
    terminate_records(&known);
}
