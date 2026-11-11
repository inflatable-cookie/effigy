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
//! The watchdog keeps those captures in `sweep-ledger.json` under the fixture
//! root. A fixture's normal cleanup reads the ledger too, because a settled
//! `down` clears the leader's record before the cleanup runs. A fixture's
//! watchdog outlives the fixture if the fixture is killed, which is the only
//! cleanup an interrupted owner gets: no `Drop` runs in a killed process.
//! Its stop pass is followed by an exit receipt naming its owner's PID and
//! its own boot/start identity, so a proof that removes the root after an
//! interrupted owner can wait for the helper itself instead of inferring its
//! completion from the recorded generations going quiet (effigy#617).
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use effigy_gateway::identity::{
    read_live_process_identity, GatewayStartIdentity, LiveProcessIdentity,
};

const WATCHDOG_ROOT_ENV: &str = "EFFIGY_FIXTURE_WATCHDOG_ROOT";
const WATCHDOG_OWNER_ENV: &str = "EFFIGY_FIXTURE_WATCHDOG_OWNER";
/// Set only by the removal-ordering discriminator, which freezes the real
/// watchdog at its ledger-write boundary behind a control file.
const WATCHDOG_HOLD_ENV: &str = "EFFIGY_FIXTURE_WATCHDOG_HOLD";
const WATCHDOG_TEST: &str = "fixture_sweep::fixture_watchdog_helper";
/// One process-table read per tick. A `ps` call costs tens of milliseconds, so
/// a shorter tick would load a parallel run enough to slow the fixtures it guards.
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(1);
const LEDGER_FILE: &str = "sweep-ledger.json";
const WATCHDOG_EXIT_FILE: &str = "sweep-watchdog-exit.json";
/// How long a held watchdog waits for the discriminator's control file to go
/// before proceeding on its own, so a failed discriminator cannot park it.
const WATCHDOG_HOLD_BUDGET: Duration = Duration::from_secs(30);

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

/// The watchdog's own boot and start identity, as published in its receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HelperIdentity {
    boot: String,
    start: GatewayStartIdentity,
}

/// The watchdog's completion receipt: the owner it watched, its own PID and
/// its own identity. It is the helper's final write, so a receipt that names
/// this owner proves every ledger write and the whole stop pass happened
/// before it, and the named helper writes nothing after it. The identity is
/// part of the receipt, not an extra: a receipt without one is refused,
/// never accepted as settled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WatchdogExit {
    pub(super) owner: u32,
    pub(super) watchdog: u32,
    identity: HelperIdentity,
}

/// `(pid, pgid)` for every process, from one read of the process table.
fn process_table() -> Vec<(u32, u32)> {
    let Ok(output) = Command::new("ps").args(["-axo", "pid=,pgid="]).output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line
                .split_whitespace()
                .map(|field| field.parse::<u32>().ok());
            Some((fields.next()??, fields.next()??))
        })
        .collect()
}

/// Process IDs whose process group is `pgid`, from the process table.
pub(super) fn process_group_members(pgid: u32) -> Vec<u32> {
    group_members(&process_table(), pgid)
}

fn group_members(table: &[(u32, u32)], pgid: u32) -> Vec<u32> {
    table
        .iter()
        .filter(|(_, group)| *group == pgid)
        .map(|(pid, _)| *pid)
        .collect()
}

fn live(pid: u32) -> Option<LiveProcessIdentity> {
    read_live_process_identity(pid).ok()
}

fn holds(live: &LiveProcessIdentity, boot: &str, start: &GatewayStartIdentity) -> bool {
    live.boot_identity == boot && live.start_identity == *start
}

/// Whether `record`'s leader still holds its recorded identity.
fn leader_holds(record: &Record) -> bool {
    live(record.pid).is_some_and(|leader| holds(&leader, &record.boot, &record.start))
}

/// Whether a recorded process remains live or closure is still unresolved.
fn holds_alive(pid: u32, boot: &str, start: &GatewayStartIdentity) -> bool {
    recorded_liveness(
        read_live_process_identity(pid),
        effigy_kernel::ident::process_state(pid),
        boot,
        start,
    )
}

fn recorded_liveness(
    identity: io::Result<LiveProcessIdentity>,
    state: effigy_kernel::observe::Observed<effigy_kernel::ident::ProcessState>,
    boot: &str,
    start: &GatewayStartIdentity,
) -> bool {
    use effigy_kernel::{ident::ProcessState, observe::Observed};
    if identity.is_ok_and(|identity| !holds(&identity, boot, start)) {
        return false;
    }
    match state {
        Observed::Absent(_) | Observed::Verified(ProcessState::Zombie, _) => false,
        Observed::Verified(..) => true,
        Observed::Unknown(_) => true,
    }
}

#[test]
fn recorded_identity_and_state_unknown_never_prove_closure() {
    use effigy_kernel::observe::{Observed, UnknownReason};
    for reason in [UnknownReason::PermissionDenied, UnknownReason::Injected] {
        assert!(recorded_liveness(
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "identity unavailable"
            )),
            Observed::Unknown(reason),
            "recorded-boot",
            &GatewayStartIdentity::Linux { start_ticks: 1 },
        ));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn recorded_zombie_is_not_a_live_survivor() {
    let mut child = Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .spawn()
        .expect("owned zombie control");
    let pid = child.id();
    let identity = live(pid).expect("unreaped child identity");
    let deadline = Instant::now() + Duration::from_secs(5);
    let is_zombie = || {
        matches!(
            effigy_kernel::ident::process_state(pid),
            effigy_kernel::observe::Observed::Verified(
                effigy_kernel::ident::ProcessState::Zombie,
                _
            )
        )
    };
    while !is_zombie() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let zombie = is_zombie();
    let alive = holds_alive(pid, &identity.boot_identity, &identity.start_identity);
    child.wait().expect("reap owned zombie control");
    assert!(zombie, "the exited child must remain a zombie until reaped");
    assert!(!alive, "an unreaped child cannot be a live survivor");
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

/// Host custody in checkout journals, plus legacy listener and private gateway
/// records. Reads state only.
fn read_records(checkouts: &[PathBuf], gateway_root: &Path) -> Vec<Record> {
    let mut found = Vec::new();
    for checkout in checkouts {
        for (_, state) in super::host_lifecycle::snapshots(checkout).unwrap_or_default() {
            push_record(
                &mut found,
                state["child_pid"].as_u64(),
                state["child_boot_identity"].as_str(),
                state["child_start_identity"].clone(),
            );
        }
        identity_records(
            &checkout.join(".effigy/runtime/host-processes"),
            &mut found,
            "child_",
        );
    }
    identity_records(gateway_root, &mut found, "");
    found
}

fn identity_records(dir: &Path, found: &mut Vec<Record>, prefix: &str) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            identity_records(&path, found, prefix);
        } else {
            let matches = if prefix.is_empty() {
                path.file_name()
                    .is_some_and(|name| name == "gateway.identity")
            } else {
                path.to_string_lossy().ends_with(".listener.json")
            };
            if !matches {
                continue;
            }
            let Some(record) = fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            else {
                continue;
            };
            push_record(
                found,
                record[format!("{prefix}pid")].as_u64(),
                record[format!("{prefix}boot_identity")].as_str(),
                record[format!("{prefix}start_identity")].clone(),
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

/// Adds `fresh` to `records`, or merges its members into the record with the
/// same leader identity.
fn merge(records: &mut Vec<Record>, fresh: Record) {
    match records.iter().position(|record| {
        record.pid == fresh.pid && record.boot == fresh.boot && record.start == fresh.start
    }) {
        Some(index) => {
            for member in fresh.members {
                if !records[index].members.contains(&member) {
                    records[index].members.push(member);
                }
            }
        }
        None => records.push(fresh),
    }
}

/// Captures the members of `record`'s group that began after the leader. Only
/// a leader that still holds its identity opens that window; members captured
/// earlier stay recorded with their own identities.
fn capture_members(record: &mut Record, table: &[(u32, u32)]) {
    let Some(leader) = live(record.pid) else {
        return;
    };
    if !holds(&leader, &record.boot, &record.start) {
        return;
    }
    for pid in group_members(table, record.pid) {
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

/// Captures group members for every record whose leader still holds its
/// identity. The process table is read at most once.
fn capture_all(records: &mut [Record]) {
    if !records.iter().any(leader_holds) {
        return;
    }
    let table = process_table();
    for record in records.iter_mut().filter(|record| leader_holds(record)) {
        capture_members(record, &table);
    }
}

/// The records under `checkouts` and `gateway_root`, with each group's members
/// captured while its leader still holds its identity. Take this before a stop
/// that may clear a listener's own record.
pub(super) fn snapshot(checkouts: &[PathBuf], gateway_root: &Path) -> Vec<Record> {
    let mut records = Vec::new();
    for record in read_records(checkouts, gateway_root) {
        merge(&mut records, record);
    }
    capture_all(&mut records);
    records
}

/// `snapshot` for a fixture root: its checkouts and gateway state under it,
/// plus every identity the watchdog captured and recorded in the ledger.
pub(super) fn snapshot_root(root: &Path) -> Vec<Record> {
    let mut records = Vec::new();
    for record in read_records(&checkouts_under(root), root)
        .into_iter()
        .chain(read_ledger(root))
    {
        merge(&mut records, record);
    }
    capture_all(&mut records);
    records
}

fn ledger_path(root: &Path) -> PathBuf {
    root.join(LEDGER_FILE)
}

fn member_value(member: &Member) -> serde_json::Value {
    serde_json::json!({
        "pid": member.pid,
        "boot": member.boot,
        "start": serde_json::to_value(&member.start).expect("start identity"),
    })
}

/// Writes `records` to the ledger in one rename, so a reader never sees a torn
/// file.
fn write_ledger(root: &Path, records: &[Record]) {
    let value = serde_json::Value::Array(
        records
            .iter()
            .map(|record| {
                serde_json::json!({
                    "pid": record.pid,
                    "boot": record.boot,
                    "start": serde_json::to_value(&record.start).expect("start identity"),
                    "members": record.members.iter().map(member_value).collect::<Vec<_>>(),
                })
            })
            .collect(),
    );
    let temporary = root.join(format!("{LEDGER_FILE}.tmp"));
    fs::write(&temporary, value.to_string()).expect("write the sweep ledger");
    fs::rename(&temporary, ledger_path(root)).expect("publish the sweep ledger");
}

/// Whether the watchdog's ledger under `root` holds `pid` as a captured member.
pub(super) fn ledger_has_member(root: &Path, pid: u32) -> bool {
    read_ledger(root)
        .iter()
        .any(|record| record.members.iter().any(|member| member.pid == pid))
}

fn exit_receipt_path(root: &Path) -> PathBuf {
    root.join(WATCHDOG_EXIT_FILE)
}

/// The watchdog's own identity at receipt time. Required: a receipt without
/// the helper's identity is never written, so the wait can always closure-
/// check the exact process it names.
fn own_identity() -> HelperIdentity {
    let live = read_live_process_identity(std::process::id())
        .expect("the watchdog's own identity for its exit receipt");
    HelperIdentity {
        boot: live.boot_identity,
        start: live.start_identity,
    }
}

fn identity_value(identity: &HelperIdentity) -> serde_json::Value {
    serde_json::json!({
        "boot": identity.boot,
        "start": serde_json::to_value(&identity.start).expect("start identity"),
    })
}

/// Publishes the watchdog's exit receipt in one rename, like the ledger. This
/// is the helper's final action before it exits: nothing writes afterwards.
fn write_exit_receipt(root: &Path, owner: u32) {
    let identity = own_identity();
    let value = serde_json::json!({
        "owner": owner,
        "watchdog": std::process::id(),
        "watchdog_identity": identity_value(&identity),
    });
    let temporary = root.join(format!("{WATCHDOG_EXIT_FILE}.tmp"));
    fs::write(&temporary, value.to_string()).expect("write the watchdog exit receipt");
    fs::rename(&temporary, exit_receipt_path(root)).expect("publish the watchdog exit receipt");
}

fn read_exit_receipt(root: &Path) -> Option<WatchdogExit> {
    let value = fs::read_to_string(exit_receipt_path(root))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())?;
    let owner = value["owner"].as_u64()? as u32;
    let watchdog = value["watchdog"].as_u64()? as u32;
    let identity = value["watchdog_identity"]
        .as_object()
        .and_then(|identity| {
            Some(HelperIdentity {
                boot: identity["boot"].as_str()?.to_owned(),
                start: serde_json::from_value(identity["start"].clone()).ok()?,
            })
        })?;
    Some(WatchdogExit {
        owner,
        watchdog,
        identity,
    })
}

/// Waits for the watchdog under `root` to publish its exit receipt for this
/// exact `owner`, then for that recorded helper identity to be gone. The
/// receipt is the helper's final write, so a returned receipt proves every
/// ledger write and the whole stop pass preceded it; the closure check rules
/// out a helper that stopped writing but never exited. A missing, unreadable,
/// identity-less, foreign-owner or unexited receipt keeps the wait polling
/// until `budget` and then fails naming the last observation: absence and
/// unknown identity are never proof of quiescence.
pub(super) fn wait_for_watchdog_exit(
    root: &Path,
    owner: u32,
    budget: Duration,
) -> io::Result<WatchdogExit> {
    let deadline = Instant::now() + budget;
    loop {
        let last = match read_exit_receipt(root) {
            Some(exit) if exit.owner == owner => {
                if !holds_alive(exit.watchdog, &exit.identity.boot, &exit.identity.start) {
                    return Ok(exit);
                }
                format!(
                    "receipt names watchdog {} which still holds its identity",
                    exit.watchdog
                )
            }
            Some(exit) => format!("receipt names owner {}, not {owner}", exit.owner),
            None => "no readable receipt with a helper identity yet".to_owned(),
        };
        if Instant::now() >= deadline {
            return Err(io::Error::other(format!(
                "the watchdog's exit receipt under {} did not settle within {budget:?}: {last}",
                root.display()
            )));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Freezes the watchdog at its ledger-write boundary while the discriminator's
/// control file exists, and marks that this real writer reached the boundary.
/// The marker names the owner's PID and the exact helper identity at the
/// boundary; the hold releases on its own after [`WATCHDOG_HOLD_BUDGET`].
/// Only the discriminator sets the environment, so no fixture watchdog runs
/// held.
fn hold_at_ledger_boundary(hold: &Path, owner: u32) {
    let held = hold.with_extension("held");
    let marker = serde_json::json!({
        "owner": owner,
        "watchdog": std::process::id(),
        "watchdog_identity": identity_value(&own_identity()),
    });
    fs::write(&held, marker.to_string()).expect("write the watchdog hold marker");
    let deadline = Instant::now() + WATCHDOG_HOLD_BUDGET;
    while hold.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    let _ = fs::remove_file(&held);
}

/// The records the watchdog wrote. A missing or unreadable ledger is empty,
/// so only identities actually written can be signalled from it.
fn read_ledger(root: &Path) -> Vec<Record> {
    let Some(value) = fs::read_to_string(ledger_path(root))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return Vec::new();
    };
    let Some(entries) = value.as_array() else {
        return Vec::new();
    };
    entries.iter().filter_map(ledger_record).collect()
}

fn ledger_record(entry: &serde_json::Value) -> Option<Record> {
    let pid = entry["pid"].as_u64()? as u32;
    let boot = entry["boot"].as_str()?.to_owned();
    let start = serde_json::from_value(entry["start"].clone()).ok()?;
    let members = entry["members"]
        .as_array()?
        .iter()
        .filter_map(|member| {
            Some(Member {
                pid: member["pid"].as_u64()? as u32,
                boot: member["boot"].as_str()?.to_owned(),
                start: serde_json::from_value(member["start"].clone()).ok()?,
            })
        })
        .collect();
    Some(Record {
        pid,
        boot,
        start,
        members,
    })
}

/// Signals one record's owned processes: the leader when it holds its identity
/// and the members that began after it while it still does, plus every
/// captured member whose identity still holds.
fn terminate_record(record: &Record) {
    let mut owned = Vec::new();
    if leader_holds(record) {
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
/// their leaders still hold their identities, and the identities the watchdog
/// captured before its leader went.
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
    spawn_watchdog_for(root, std::process::id(), None)
}

/// [`spawn_watchdog`] with an explicit owner PID and an optional hold control
/// file for the removal-ordering discriminator.
pub(super) fn spawn_watchdog_for(root: &Path, owner: u32, hold: Option<&Path>) -> Child {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(std::env::current_exe().expect("test binary"));
    command
        .args(["--exact", WATCHDOG_TEST, "--ignored", "--nocapture"])
        .env(WATCHDOG_ROOT_ENV, root)
        .env(WATCHDOG_OWNER_ENV, owner.to_string())
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(hold) = hold {
        command.env(WATCHDOG_HOLD_ENV, hold);
    }
    command.spawn().expect("start the fixture watchdog")
}

/// The fixture watchdog body. It keeps the owner's generations and their group
/// members captured in the ledger while the owner lives, then stops them once
/// the owner is gone and publishes its exit receipt. It runs only as the child
/// of a fixture.
#[test]
#[ignore = "started by a fixture as its watchdog"]
fn fixture_watchdog_helper() {
    let root = PathBuf::from(std::env::var_os(WATCHDOG_ROOT_ENV).expect("watchdog root"));
    let owner: u32 = std::env::var(WATCHDOG_OWNER_ENV)
        .expect("watchdog owner")
        .parse()
        .expect("watchdog owner pid");
    let hold = std::env::var_os(WATCHDOG_HOLD_ENV).map(PathBuf::from);
    let mut known: Vec<Record> = Vec::new();
    while nix::sys::signal::kill(nix::unistd::Pid::from_raw(owner as i32), None).is_ok() {
        for fresh in read_records(&checkouts_under(&root), &root) {
            merge(&mut known, fresh);
        }
        capture_all(&mut known);
        if let Some(hold) = &hold {
            hold_at_ledger_boundary(hold, owner);
        }
        write_ledger(&root, &known);
        thread::sleep(WATCHDOG_INTERVAL);
    }
    terminate_records(&known);
    write_exit_receipt(&root, owner);
}
