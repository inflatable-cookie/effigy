//! Parked-sentinel snapshots and the explicit release request (effigy#1034).
//!
//! A foreground memory guard that cannot prove group closure stays alive
//! holding the checkout gates. This module is the later action that releases
//! that holder: the same membership inventory the sentinel uses, a snapshot
//! the CLI can name, and a state-root request file the parked loop polls.
//! Nothing here signals a task process.

use crate::custody::platform::{self, MembershipRow, RoundVerdict};
use crate::fs::{FileName, Trust, FILE_MODE};
use crate::observe::Observed;
use crate::root::{Family, StateRoot};
use crate::run::Effect;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::time::Duration;

/// Schema for a parked-holder snapshot under `.effigy/state/parked/`.
pub const PARKED_SCHEMA: &str = "effigy.parked-sentinel.v1";

/// One inventory of the run's recorded process groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupInventory {
    /// Positively live members, sorted and unique.
    pub live_pids: Vec<u32>,
    /// A row stayed ambiguous: this inventory is not proof of emptiness.
    pub unproven: bool,
}

impl GroupInventory {
    /// No live member and no ambiguous row.
    pub fn proved_empty(&self) -> bool {
        self.live_pids.is_empty() && !self.unproven
    }
}

/// What a parked sentinel writes so `run stop`, lock-wait, and `doctor` can
/// name the holder, and so `run release` can re-inventory the same groups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParkedHolder {
    pub schema: String,
    pub run: String,
    pub task: String,
    pub sentinel_pid: u32,
    pub owner_pid: u32,
    pub checkout: String,
    pub pgids: Vec<u32>,
    pub live_pids: Vec<u32>,
    pub state: String,
}

impl ParkedHolder {
    pub fn release_command(&self) -> String {
        format!("effigy run release {}", self.run)
    }

    /// One line `run stop`, lock-wait and `doctor` can print.
    pub fn named(&self) -> String {
        format!(
            "run {} task {} sentinel pid {} owner pid {} checkout {} state {}",
            self.run, self.task, self.sentinel_pid, self.owner_pid, self.checkout, self.state
        )
    }
}

/// Why `run release` refused. Never a signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseError {
    /// No parked snapshot for this run: the sentinel is not parked, or it
    /// predates this protocol and cannot handle the request.
    NotParked {
        run: String,
    },
    /// At least one live member of a recorded group. Named, never signalled.
    LiveMembers {
        run: String,
        pids: Vec<u32>,
    },
    /// Inventory could not prove emptiness.
    Unproven {
        run: String,
    },
    Io(String),
}

impl std::fmt::Display for ReleaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotParked { run } => write!(
                f,
                "run {run} has no parked sentinel in this checkout; `effigy run release {run}` \
                 only asks a parked sentinel to drop its gates. An older sentinel that never wrote \
                 a snapshot cannot handle the request"
            ),
            Self::LiveMembers { run, pids } => {
                let named = pids
                    .iter()
                    .map(|pid| pid.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "run {run} still has live process-group members (pids {named}); \
                     `effigy run release {run}` signals nothing. Stop those processes through \
                     their own owner, then retry"
                )
            }
            Self::Unproven { run } => write!(
                f,
                "run {run}: process-group inventory could not prove emptiness; \
                 `effigy run release {run}` signals nothing"
            ),
            Self::Io(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ReleaseError {}

/// A release request that passed the live-member check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseOutcome {
    pub run: String,
    pub task: String,
    pub checkout: String,
    pub sentinel_pid: u32,
}

/// The recorded process groups on one run's custody receipts.
pub fn effect_pgids(effects: &[Effect]) -> Vec<u32> {
    let mut pgids: Vec<u32> = effects
        .iter()
        .filter_map(|effect| {
            effect
                .receipt
                .as_ref()
                .and_then(|value| value.get("pgid"))
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .filter(|pgid| *pgid > 0)
        })
        .collect();
    pgids.sort_unstable();
    pgids.dedup();
    pgids
}

/// One inventory of every recorded group, using the same membership rows the
/// sentinel uses. Never signals.
pub fn inventory_groups(pgids: &[u32]) -> io::Result<GroupInventory> {
    let mut live_pids = Vec::new();
    let mut unproven = false;
    for pgid in pgids.iter().copied().filter(|pgid| *pgid > 0) {
        match platform::group_closed(pgid, &[]) {
            Observed::Verified(_, _) | Observed::Absent(_) => {}
            Observed::Unknown(_) => unproven = true,
        }
        let rows = platform::group_membership(pgid)?;
        if platform::classify_round(&rows, &[]) == RoundVerdict::Unproven {
            unproven = true;
        }
        for row in rows {
            if let MembershipRow::Live(identity) = row {
                live_pids.push(identity.pid);
            }
        }
    }
    live_pids.sort_unstable();
    live_pids.dedup();
    Ok(GroupInventory {
        live_pids,
        unproven,
    })
}

/// Write or replace this run's parked snapshot.
pub fn write_snapshot(root: &StateRoot, holder: &ParkedHolder) -> io::Result<()> {
    let dir = parked_dir(root, true)?.ok_or_else(|| {
        io::Error::other("checkout state could not create the parked-sentinel directory")
    })?;
    let name = snapshot_name(&holder.run)?;
    write_json(&dir, &name, holder)
}

/// Read one run's parked snapshot, if present.
pub fn read_snapshot(root: &StateRoot, run: &str) -> io::Result<Option<ParkedHolder>> {
    let Some(dir) = parked_dir(root, false)? else {
        return Ok(None);
    };
    let name = snapshot_name(run)?;
    read_json(&dir, &name)
}

/// Every parked snapshot in this checkout.
pub fn list_parked(root: &StateRoot) -> io::Result<Vec<ParkedHolder>> {
    let Some(dir) = parked_dir(root, false)? else {
        return Ok(Vec::new());
    };
    let mut holders = Vec::<ParkedHolder>::new();
    for name in dir.list(4096).map_err(io::Error::other)? {
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.ends_with(".json") || name.ends_with(".release") {
            continue;
        }
        let Ok(file) = FileName::new(name) else {
            continue;
        };
        if let Ok(Some(holder)) = read_json(&dir, &file) {
            holders.push(holder);
        }
    }
    holders.sort_by(|a, b| a.run.cmp(&b.run));
    Ok(holders)
}

/// Remove the snapshot after the sentinel drops its gates.
pub fn remove_snapshot(root: &StateRoot, run: &str) -> io::Result<()> {
    let Some(dir) = parked_dir(root, false)? else {
        return Ok(());
    };
    let name = snapshot_name(run)?;
    match dir.remove(&name, false) {
        Ok(()) => {}
        Err(error) if error.is_not_found() => {}
        Err(error) => return Err(io::Error::other(error)),
    }
    let release = release_name(run)?;
    match dir.remove(&release, false) {
        Ok(()) => {}
        Err(error) if error.is_not_found() => {}
        Err(error) => return Err(io::Error::other(error)),
    }
    Ok(())
}

/// Whether the CLI has asked this parked sentinel to drop its gates.
pub fn release_requested(root: &StateRoot, run: &str) -> bool {
    let Ok(Some(dir)) = parked_dir(root, false) else {
        return false;
    };
    let Ok(name) = release_name(run) else {
        return false;
    };
    dir.entry(&name).ok().flatten().is_some()
}

/// Re-inventory the parked run's groups and, only if they are empty, write the
/// release request the sentinel polls. Never signals.
pub fn request_release(root: &StateRoot, run: &str) -> Result<ReleaseOutcome, ReleaseError> {
    let holder = read_snapshot(root, run)
        .map_err(|error| ReleaseError::Io(error.to_string()))?
        .ok_or_else(|| ReleaseError::NotParked {
            run: run.to_owned(),
        })?;
    let inventory =
        inventory_groups(&holder.pgids).map_err(|error| ReleaseError::Io(error.to_string()))?;
    if !inventory.live_pids.is_empty() {
        return Err(ReleaseError::LiveMembers {
            run: run.to_owned(),
            pids: inventory.live_pids,
        });
    }
    if !inventory.proved_empty() {
        return Err(ReleaseError::Unproven {
            run: run.to_owned(),
        });
    }
    let dir = parked_dir(root, true)
        .map_err(|error| ReleaseError::Io(error.to_string()))?
        .ok_or_else(|| ReleaseError::Io("parked-sentinel directory missing".into()))?;
    let name = release_name(run).map_err(|error| ReleaseError::Io(error.to_string()))?;
    write_bytes(&dir, &name, b"release\n").map_err(|error| ReleaseError::Io(error.to_string()))?;
    Ok(ReleaseOutcome {
        run: holder.run,
        task: holder.task,
        checkout: holder.checkout,
        sentinel_pid: holder.sentinel_pid,
    })
}

/// Wait until the parked snapshot is gone, which is the sentinel dropping its
/// gates and exiting. Bounded; the caller reports the remaining holder if it
/// stays.
pub fn wait_released(root: &StateRoot, run: &str, budget: Duration) -> bool {
    let deadline = std::time::Instant::now() + budget;
    while std::time::Instant::now() < deadline {
        match read_snapshot(root, run) {
            Ok(None) => return true,
            Ok(Some(_)) | Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    matches!(read_snapshot(root, run), Ok(None))
}

fn parked_dir(root: &StateRoot, create: bool) -> io::Result<Option<crate::fs::DirFd>> {
    if create {
        root.family(Family::Parked).map_err(io::Error::other)
    } else {
        root.read_family(Family::Parked).map_err(io::Error::other)
    }
}

fn snapshot_name(run: &str) -> io::Result<FileName> {
    FileName::new(&format!("{run}.json")).map_err(io::Error::other)
}

fn release_name(run: &str) -> io::Result<FileName> {
    FileName::new(&format!("{run}.release")).map_err(io::Error::other)
}

fn write_json<T: Serialize>(dir: &crate::fs::DirFd, name: &FileName, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    write_bytes(dir, name, &bytes)
}

fn write_bytes(dir: &crate::fs::DirFd, name: &FileName, bytes: &[u8]) -> io::Result<()> {
    let temp = FileName::new(&format!(".{}.tmp", name.as_str())).map_err(io::Error::other)?;
    let _ = dir.remove(&temp, false);
    let (mut file, _) = dir
        .open_file(
            &temp,
            libc::O_WRONLY | libc::O_EXCL,
            Some(FILE_MODE),
            Trust::Private,
        )
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::other("parked file was not created"))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    dir.rename(&temp, name).map_err(io::Error::other)?;
    dir.sync().map_err(io::Error::other)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    dir: &crate::fs::DirFd,
    name: &FileName,
) -> io::Result<Option<T>> {
    let Some((mut file, _)) = dir
        .open_file(name, libc::O_RDONLY, None, Trust::Private)
        .map_err(io::Error::other)?
    else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    Ok(Some(value))
}

#[cfg(all(test, feature = "fault-injection"))]
mod tests {
    use super::*;
    use crate::ctx::{CancelToken, Deadline, EffectClass, Limits, OperationContext, Placement};
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::time::Instant;

    fn context() -> crate::ctx::EffectContext {
        OperationContext::new(
            Deadline::At(Instant::now() + Duration::from_secs(30)),
            CancelToken::default(),
            Limits {
                capture_tail: 4096,
                log_rotate: 4096,
                log_keep: 2,
                line_max: 512,
                queue_bytes: 4096,
                probe_concurrency: 1,
            },
            EffectClass::Mutate,
            Placement::Host,
        )
        .effects()
        .unwrap()
    }

    fn setsid_sleep() -> std::process::Child {
        let mut command = Command::new("/bin/sleep");
        command.arg("30");
        // SAFETY: setsid is async-signal-safe and runs only in this child.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        command.spawn().expect("setsid sleep")
    }

    fn stop(child: &mut std::process::Child) {
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn inventory_groups_names_a_live_member_and_is_empty_after_it_exits() {
        let mut child = setsid_sleep();
        let pid = child.id();
        let inventory = inventory_groups(&[pid]).expect("inventory");
        assert!(
            inventory.live_pids.contains(&pid),
            "live member must be named: {:?}",
            inventory.live_pids
        );
        assert!(!inventory.proved_empty());
        stop(&mut child);
        let closed = inventory_groups(&[pid]).expect("empty inventory");
        assert!(
            closed.proved_empty(),
            "a reaped setsid child is an empty group: {closed:?}"
        );
    }

    #[test]
    fn request_release_refuses_named_live_members_then_succeeds_once_gone() {
        let home = effigy_test_support::short_tempdir("pk-").unwrap();
        let cx = context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let mut child = setsid_sleep();
        let pid = child.id();
        let holder = ParkedHolder {
            schema: PARKED_SCHEMA.into(),
            run: "releasefixture0000000001".into(),
            task: "validate:release".into(),
            sentinel_pid: std::process::id(),
            owner_pid: 1,
            checkout: home.path().display().to_string(),
            pgids: vec![pid],
            live_pids: vec![pid],
            state: "parked".into(),
        };
        write_snapshot(&root, &holder).expect("snapshot");
        let refused = request_release(&root, &holder.run).expect_err("live member");
        match refused {
            ReleaseError::LiveMembers { pids, .. } => {
                assert!(pids.contains(&pid), "{pids:?}");
            }
            other => panic!("expected named live members, got {other}"),
        }
        stop(&mut child);
        let outcome = request_release(&root, &holder.run).expect("empty group releases");
        assert_eq!(outcome.run, holder.run);
        assert_eq!(outcome.task, holder.task);
        assert!(
            release_requested(&root, &holder.run),
            "the parked sentinel must see the release file"
        );
        remove_snapshot(&root, &holder.run).expect("cleanup snapshot");
    }
}
