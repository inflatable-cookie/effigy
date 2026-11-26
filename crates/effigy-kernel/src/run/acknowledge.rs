//! Explicit operator acknowledgement of verified-dead `closed` + `incomplete`
//! residue (contract 052 §K5).
//!
//! Nothing here signals a process. Every recorded identity (`owner_identity`,
//! `sentinel_identity`, `supervisor_identity`) and every residue effect's
//! `identity`, `anchor`, and `pgid` group inventory must be positively dead
//! before any orphaned interpolation file is removed and `runs.residue_ack` is
//! recorded.

use super::*;
use crate::ctx::Capability;
use crate::ident::{self, ProcessIdentity, ProcessState};
pub use crate::interpolation::SkippedInterpolationCleanup;
use crate::interpolation::{check_pending_compose_env, DeadInterpolationCleanup};
use crate::observe::Observed;
use crate::store::BusyWait;
use crate::supervise::park;
use rusqlite::OptionalExtension;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AcknowledgeOutcome {
    pub run: RunId,
    pub acknowledged_at_ms: i64,
    pub removed_interpolation_files: Vec<PathBuf>,
    pub skipped_interpolation_files: Vec<SkippedInterpolationCleanup>,
}

struct StoredRunRow {
    owner: ResourceId,
    gates: Vec<ResourceId>,
    phase: Phase,
    cleanup_outcome: Option<String>,
    residue_ack: Option<String>,
    owner_identity: Option<ProcessIdentity>,
    supervisor_identity: Option<ProcessIdentity>,
    sentinel_identity: Option<ProcessIdentity>,
}

enum IdentityLiveness {
    Dead,
    Live(u32),
    Unproven(String),
}

/// Verify that `id` is `closed` with `incomplete` cleanup and unacknowledged
/// residue, re-verify that every recorded process and process group is dead,
/// clean up any orphaned interpolation files owned by dead processes, and
/// record `runs.residue_ack` through the kernel store.
pub fn acknowledge_residue(
    cx: &EffectContext,
    root: &StateRoot,
    id: RunId,
    effigy_home: Option<&Path>,
) -> Result<AcknowledgeOutcome, RunError> {
    cx.require(Capability::AdapterEffect)
        .map_err(|_| RunError::refused("acknowledgement requires Mutate"))?;
    cx.require(Capability::HostAuthority).map_err(|_| {
        RunError::refused("residue acknowledgement requires host authority, not a guest placement")
    })?;

    let initial = {
        let store = Store::open(&cx.child(Duration::from_secs(30)), root)?;
        read_run_row(store.conn(), id)?
    };
    validate_acknowledgeable(id, &initial)?;

    let mut gate_ids = initial.gates.clone();
    gate_ids.push(initial.owner.clone());
    let gate_ids = aliases::gates(root, &initial.owner, &gate_ids)?;
    // K14: acquire the owner gate before reopening the store for mutation.
    let _guard = Gate::acquire(cx, root, &gate_ids, Wait::FailFast)?;

    let mut store = Store::open(&cx.child(Duration::from_secs(30)), root)?;
    store.set_run(id);
    let row = read_run_row(store.conn(), id)?;
    validate_acknowledgeable(id, &row)?;

    let effects = journal::effects(store.conn(), id)?;
    let residue_effects: Vec<&Effect> = effects.iter().filter(|e| !e.state.terminal()).collect();

    let mut live_pids = BTreeSet::new();
    let mut unproven = Vec::new();
    let mut pgids = Vec::new();
    let mut interpolations = Vec::new();
    let mut has_pending_prelaunch_interpolation = false;
    let current_boot = ident::boot_identity().verified().cloned();

    for identity in [
        row.owner_identity.as_ref(),
        row.supervisor_identity.as_ref(),
        row.sentinel_identity.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        record_identity_liveness(identity, &mut live_pids, &mut unproven);
    }

    for effect in &residue_effects {
        if effect.spec.payload.get("interpolation_prelaunch_v1") == Some(&Value::Bool(true)) {
            has_pending_prelaunch_interpolation = true;
        }
        if let Some(receipt) = crate::custody::residue_receipt(effect) {
            record_identity_liveness(&receipt.process.identity, &mut live_pids, &mut unproven);
            if let Some(anchor) = &receipt.anchor {
                record_identity_liveness(anchor, &mut live_pids, &mut unproven);
            }
            let same_boot = current_boot
                .as_ref()
                .is_some_and(|boot| *boot == receipt.process.identity.boot);
            if same_boot && receipt.process.pgid > 0 {
                pgids.push(receipt.process.pgid);
            }
            if let Some(binding) = receipt.binding {
                interpolations.push((effect.spec.target.clone(), binding));
            }
        } else if let Some(receipt) = &effect.receipt {
            inspect_raw_receipt(
                receipt,
                current_boot.as_ref(),
                &mut live_pids,
                &mut unproven,
                &mut pgids,
                &mut interpolations,
                &effect.spec.target,
            );
        }
    }

    pgids.sort_unstable();
    pgids.dedup();
    if !pgids.is_empty() {
        let inventory = park::inventory_groups(&pgids)?;
        for pid in inventory.live_pids {
            live_pids.insert(pid);
        }
        if inventory.unproven {
            unproven.push("process-group inventory could not prove emptiness".to_owned());
        }
    }

    if !live_pids.is_empty() {
        let named = live_pids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(RunError::refused(format!(
            "run {id} still has live process(es) (pids {named}); `effigy run acknowledge {id}` \
             signals nothing. Stop those processes through their own owner, then retry"
        )));
    }
    if !unproven.is_empty() {
        return Err(RunError::unavailable(format!(
            "run {id}: process liveness could not be verified ({}); `effigy run acknowledge {id}` signals nothing",
            unproven.join("; ")
        )));
    }

    let mut removed_interpolation_files = Vec::new();
    let mut skipped_interpolation_files = Vec::new();
    if let Some(home) = effigy_home {
        for (fence, binding) in &interpolations {
            match crate::interpolation::cleanup_dead_residue_interpolation(
                home, id, fence, binding,
            )? {
                DeadInterpolationCleanup::Removed(path) => removed_interpolation_files.push(path),
                DeadInterpolationCleanup::AlreadyAbsent => {}
                DeadInterpolationCleanup::Skipped(skipped) => {
                    skipped_interpolation_files.push(skipped);
                }
            }
        }
        if has_pending_prelaunch_interpolation && interpolations.is_empty() {
            if let Some(skipped) = check_pending_compose_env(
                home,
                "prelaunch interpolation crashed before binding its temporary file stamp; inspect compose-env and remove any leftover file from this run",
            )? {
                skipped_interpolation_files.push(skipped);
            }
        }
    } else if !interpolations.is_empty() || has_pending_prelaunch_interpolation {
        skipped_interpolation_files.push(SkippedInterpolationCleanup {
            path: None,
            reason: "Effigy state home could not be determined to inspect compose-env for orphaned interpolation files"
                .to_owned(),
        });
    }

    let acknowledged_at_ms = store.transaction_within(
        &cx.child(Duration::from_secs(30)),
        BusyWait::AtLeast(SETTLE_BUSY_WAIT),
        |txn| {
            let (phase, cleanup, ack): (String, Option<String>, Option<String>) = txn
                .conn()
                .query_row(
                    "SELECT phase, cleanup_outcome, residue_ack FROM runs WHERE run_id=?1",
                    [id.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
            if phase != "closed" || cleanup.as_deref() != Some("incomplete") || ack.is_some() {
                return Err(RunError::refused(format!(
                    "run {id} state changed before residue acknowledgement could commit"
                )));
            }
            let now = txn.now();
            let updated = txn.conn().execute(
                "UPDATE runs SET residue_ack=?2 WHERE run_id=?1 AND phase='closed' AND cleanup_outcome='incomplete' AND residue_ack IS NULL",
                params![id.to_string(), now],
            )?;
            if updated != 1 {
                return Err(RunError::broken(format!(
                    "expected to acknowledge 1 row for run {id}, updated {updated}"
                )));
            }
            Ok::<_, RunError>(now)
        },
    )?;

    Ok(AcknowledgeOutcome {
        run: id,
        acknowledged_at_ms,
        removed_interpolation_files,
        skipped_interpolation_files,
    })
}

fn read_run_row(conn: &rusqlite::Connection, id: RunId) -> Result<StoredRunRow, RunError> {
    let row = conn
        .query_row(
            "SELECT r.phase, r.cleanup_outcome, r.residue_ack, r.invocation, \
             r.owner_identity, r.supervisor_identity, r.sentinel_identity, res.canonical \
             FROM runs r JOIN resources res ON res.name=r.owner WHERE r.run_id=?1",
            [id.to_string()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Vec<u8>>(7)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| RunError::refused(format!("run {id} was not found in this checkout journal")))?;

    let invocation = row
        .3
        .as_deref()
        .map(|text| ResolvedInvocation::decode(text.as_bytes()))
        .transpose()
        .map_err(|e| RunError::Store(e.into()))?;
    let owner = match &invocation {
        Some(inv) => inv.spec().owner.clone(),
        None => ResourceId::decode_canonical(&row.7).map_err(|e| RunError::Store(e.into()))?,
    };
    let gates = invocation
        .as_ref()
        .map(|inv| inv.spec().gates.clone())
        .unwrap_or_default();
    let parse_identity = |raw: Option<String>| -> Result<Option<ProcessIdentity>, RunError> {
        Ok(raw.map(|text| serde_json::from_str(&text)).transpose()?)
    };

    Ok(StoredRunRow {
        owner,
        gates,
        phase: Phase::parse(&row.0)?,
        cleanup_outcome: row.1,
        residue_ack: row.2,
        owner_identity: parse_identity(row.4)?,
        supervisor_identity: parse_identity(row.5)?,
        sentinel_identity: parse_identity(row.6)?,
    })
}

fn validate_acknowledgeable(id: RunId, row: &StoredRunRow) -> Result<(), RunError> {
    if row.phase != Phase::Closed {
        return Err(RunError::refused(format!(
            "run {id} is in phase `{}` and must be closed before its residue can be acknowledged",
            row.phase.name()
        )));
    }
    match row.cleanup_outcome.as_deref() {
        Some("incomplete") => {}
        Some("complete") => {
            return Err(RunError::refused(format!(
                "run {id} already completed cleanup and has no residue to acknowledge"
            )));
        }
        Some(other) => {
            return Err(RunError::broken(format!(
                "run {id} has unexpected cleanup outcome `{other}`"
            )));
        }
        None => {
            return Err(RunError::refused(format!(
                "run {id} has no incomplete cleanup outcome to acknowledge"
            )));
        }
    }
    if row.residue_ack.is_some() {
        return Err(RunError::refused(format!(
            "run {id} residue was already acknowledged"
        )));
    }
    Ok(())
}

fn record_identity_liveness(
    identity: &ProcessIdentity,
    live_pids: &mut BTreeSet<u32>,
    unproven: &mut Vec<String>,
) {
    match check_identity(identity) {
        IdentityLiveness::Dead => {}
        IdentityLiveness::Live(pid) => {
            live_pids.insert(pid);
        }
        IdentityLiveness::Unproven(reason) => unproven.push(reason),
    }
}

fn check_identity(identity: &ProcessIdentity) -> IdentityLiveness {
    if identity.pid == 0 || identity.pid > i32::MAX as u32 {
        return IdentityLiveness::Unproven(format!("invalid pid {}", identity.pid));
    }
    match ident::process_identity(identity.pid) {
        Observed::Absent(_) => IdentityLiveness::Dead,
        Observed::Verified(actual, _) if actual != *identity => IdentityLiveness::Dead,
        Observed::Verified(_, _) => match ident::process_state(identity.pid) {
            Observed::Absent(_) => IdentityLiveness::Dead,
            Observed::Verified(ProcessState::Zombie, _) => {
                // Recheck identity so a reused PID cannot lend its zombie state.
                match ident::process_identity(identity.pid) {
                    Observed::Verified(_, _) | Observed::Absent(_) => IdentityLiveness::Dead,
                    Observed::Unknown(reason) => {
                        IdentityLiveness::Unproven(format!("pid {}: {reason}", identity.pid))
                    }
                }
            }
            Observed::Verified(ProcessState::Active | ProcessState::Stopped, _) => {
                IdentityLiveness::Live(identity.pid)
            }
            Observed::Unknown(reason) => {
                IdentityLiveness::Unproven(format!("pid {}: {reason}", identity.pid))
            }
        },
        Observed::Unknown(reason) => {
            IdentityLiveness::Unproven(format!("pid {}: {reason}", identity.pid))
        }
    }
}

fn inspect_raw_receipt(
    receipt: &Value,
    current_boot: Option<&ident::BootIdentity>,
    live_pids: &mut BTreeSet<u32>,
    unproven: &mut Vec<String>,
    pgids: &mut Vec<u32>,
    interpolations: &mut Vec<(Fence, Value)>,
    target: &Fence,
) {
    if let Some(binding) = receipt.get("binding") {
        interpolations.push((target.clone(), binding.clone()));
    }
    let mut stack = vec![receipt];
    while let Some(node) = stack.pop() {
        let Some(map) = node.as_object() else {
            continue;
        };
        if let Some(identity_val) = map.get("identity") {
            if let Ok(identity) = serde_json::from_value::<ProcessIdentity>(identity_val.clone()) {
                record_identity_liveness(&identity, live_pids, unproven);
            }
        }
        if let Some(anchor_val) = map.get("anchor") {
            if let Ok(anchor) = serde_json::from_value::<ProcessIdentity>(anchor_val.clone()) {
                record_identity_liveness(&anchor, live_pids, unproven);
            }
        }
        if let Some(pgid) = map
            .get("pgid")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .filter(|pgid| *pgid > 0)
        {
            let boot_matches = map
                .get("identity")
                .and_then(|v| serde_json::from_value::<ProcessIdentity>(v.clone()).ok())
                .is_none_or(|id| current_boot.is_some_and(|boot| *boot == id.boot));
            if boot_matches {
                pgids.push(pgid);
            }
        }
        for key in ["interpolation_v1", "discard_v1"] {
            if let Some(nested) = map.get(key) {
                stack.push(nested);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{CheckoutId, CheckoutKind, Surface, TaskId};
    use crate::outcome::{CleanupOutcome, WorkloadOutcome};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::process::Command;
    #[cfg(feature = "fault-injection")]
    use std::time::{Duration, Instant};

    fn test_owner(tag: &str) -> ResourceId {
        let checkout = CheckoutId::new(CheckoutKind::EphemeralClone, [9; 16]);
        ResourceId::environment(checkout, tag).unwrap()
    }

    #[cfg(feature = "fault-injection")]
    fn kill_and_reap(identity: &ProcessIdentity) {
        if let Observed::Verified(actual, _) = ident::process_identity(identity.pid) {
            if actual == *identity {
                // SAFETY: tests only signal the exact fixture process they own.
                unsafe {
                    libc::kill(identity.pid as i32, libc::SIGKILL);
                }
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut status = 0;
            // SAFETY: non-blocking wait on our own direct children; ECHILD for indirect helpers.
            unsafe {
                libc::waitpid(identity.pid as i32, &mut status, libc::WNOHANG);
            }
            match ident::process_identity(identity.pid) {
                Observed::Absent(_) => break,
                Observed::Verified(actual, _) if actual != *identity => break,
                _ => {
                    assert!(
                        Instant::now() < deadline,
                        "timed out waiting for fixture process {} to exit",
                        identity.pid
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    fn setup_closed_incomplete_interpolated_run(
        home: &Path,
        cx: &EffectContext,
        root: &StateRoot,
        owner: &ResourceId,
        effigy_home: &Path,
    ) {
        let mut fake = super::super::tests::Fake::new(home);
        let run = Run::begin(
            cx,
            root,
            &super::super::tests::inv(owner.clone()),
            super::super::tests::opts(1),
            &super::super::tests::cleanup(cx),
            &mut fake,
        )
        .unwrap();
        let fence = run.fence().clone();
        run.settle(&super::super::tests::cleanup(cx), &mut fake)
            .unwrap();

        let env_dir = effigy_home.join("compose-env");
        std::fs::create_dir_all(&env_dir).unwrap();
        std::fs::set_permissions(&env_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let file_name = "compose-fixture.env";
        let file_path = env_dir.join(file_name);
        {
            let _file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&file_path)
                .unwrap();
        }
        let dir_meta = std::fs::metadata(&env_dir).unwrap();
        let file_meta = std::fs::metadata(&file_path).unwrap();

        let mut dead_child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let dead_id = ident::process_identity(dead_child.id())
            .verified()
            .cloned()
            .unwrap();
        let _ = dead_child.kill();
        let _ = dead_child.wait();

        let target_spec = EffectSpec {
            target: fence.clone(),
            kind: crate::custody::KIND.to_owned(),
            payload: Value::Null,
            settlement: Settlement::Release(Value::Null),
            required: true,
        };
        let effect = Effect {
            sequence: 1,
            spec: target_spec.clone(),
            state: EffectState::Applied,
            receipt: None,
            reason: None,
        };
        let invocation = "0123456789abcdef0123456789abcdef";
        let binding = serde_json::json!({
            "version": 1,
            "invocation": invocation,
            "run": super::super::tests::id(1),
            "fence": fence,
            "effect": effect,
            "original": file_name,
            "location": "Original",
            "file": { "device": file_meta.dev(), "inode": file_meta.ino() },
            "directory": { "device": dir_meta.dev(), "inode": dir_meta.ino() },
            "completed": false,
        });
        {
            use std::io::Write;
            let mut custody = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(env_dir.join(format!(".custody-{invocation}")))
                .unwrap();
            custody
                .write_all(&serde_json::to_vec(&binding).unwrap())
                .unwrap();
        }
        let receipt = serde_json::json!({
            "interpolation_v1": {
                "identity": dead_id,
                "pgid": 0,
                "nonce": "dead-nonce",
                "grace_millis": 100,
            },
            "binding": binding,
        });
        {
            let store = Store::open(cx, root).unwrap();
            store
                .conn()
                .execute(
                    "UPDATE runs SET phase='closed', cleanup_outcome='incomplete', residue_ack=NULL WHERE run_id=?1",
                    [super::super::tests::id(1).to_string()],
                )
                .unwrap();
            store
                .conn()
                .execute(
                    "INSERT INTO events(run_id, seq, effect_id, kind, target, settlement, state, receipt, error, wall_time) \
                     VALUES (?1, 1, '1', ?2, ?3, ?4, 'residue', ?5, 'retained interpolation requires its live descriptor owner', 1)",
                    params![
                        super::super::tests::id(1).to_string(),
                        crate::custody::KIND,
                        serde_json::to_string(&target_spec).unwrap(),
                        serde_json::to_string(&Settlement::Release(Value::Null)).unwrap(),
                        serde_json::to_string(&receipt).unwrap(),
                    ],
                )
                .unwrap();
        }

        let blocked = Run::begin(
            cx,
            root,
            &super::super::tests::inv(owner.clone()),
            super::super::tests::opts(2),
            &super::super::tests::cleanup(cx),
            &mut fake,
        );
        assert!(
            matches!(blocked, Err(RunError::PredecessorUnsettled { run, .. }) if run == super::super::tests::id(1)),
            "expected PredecessorUnsettled after interrupted interpolated run, got {blocked:?}"
        );
    }

    #[cfg(feature = "fault-injection")]
    #[test]
    fn live_custody_crash_leaves_residue_acknowledgeable_once_processes_exit() {
        use crate::interpolation::{Location, RetainedInterpolation};
        use crate::supervise::LifecyclePlan;
        use std::os::fd::AsRawFd;

        let home = effigy_test_support::short_tempdir("ack-live-crash-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let owner = test_owner("ack-live-crash");
        let effigy_home = home.path().join("effigy-home");
        let env_dir = effigy_home.join("compose-env");
        std::fs::create_dir_all(&env_dir).unwrap();
        std::fs::set_permissions(&env_dir, std::fs::Permissions::from_mode(0o700)).unwrap();

        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(env_dir.join("compose-fixture.env"))
            .unwrap();
        // SAFETY: live fixture descriptor lock.
        assert_eq!(
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) },
            0
        );
        let retained = RetainedInterpolation::new(
            file.into(),
            std::fs::File::open(&env_dir).unwrap().into(),
            "compose-fixture.env".into(),
            Location::Original,
        )
        .unwrap();

        let mut fake = super::super::tests::Fake::new(home.path());
        let mut run = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner.clone()),
            super::super::tests::opts(1),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        run.enable_custody(&LifecyclePlan {
            executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
                .unwrap(),
            placement: crate::supervise::Placement::Detached,
        })
        .unwrap();
        let fence = run.fence().clone();
        let backup = run.checkout_backup_identity().unwrap();
        let sentinel = run.sentinel.as_ref().unwrap().identity.clone();
        let process = crate::custody::Custody::spawn_interpolated(
            &mut run,
            crate::custody::ProcessSpec {
                argv: vec!["/bin/sleep".into(), "30".into()],
                env: std::collections::BTreeMap::new(),
                cwd: home.path().to_path_buf(),
                grace: Duration::from_millis(10),
                lifetime: crate::custody::Lifetime::Finite(Duration::from_secs(30)),
                terminal: crate::custody::Terminal::None,
                label: "interpolated crash fixture".into(),
            },
            &cx,
            retained,
        )
        .unwrap();
        let effect = run
            .effects()
            .unwrap()
            .into_iter()
            .find(|effect| {
                effect.spec.kind == crate::custody::KIND
                    && effect
                        .spec
                        .payload
                        .get("interpolation_prelaunch_v1")
                        .is_none()
            })
            .unwrap();
        let anchor: ProcessIdentity =
            serde_json::from_value(effect.receipt.unwrap()["interpolation_v1"]["anchor"].clone())
                .unwrap();
        let child = process.identity().identity;

        kill_and_reap(&sentinel);
        kill_and_reap(&backup);
        drop(process);
        drop(run);
        kill_and_reap(&child);
        kill_and_reap(&anchor);

        let blocked = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner.clone()),
            super::super::tests::opts(2),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        );
        assert!(
            matches!(blocked, Err(RunError::PredecessorUnsettled { run, .. }) if run == super::super::tests::id(1)),
            "expected PredecessorUnsettled after real crash, got {blocked:?}"
        );
        assert!(env_dir.join("compose-fixture.env").exists());
        let _ = fence;

        // While the original owner PID (this test process) is still live,
        // acknowledgement must refuse by PID and keep the secret file intact.
        let owner_pid = std::process::id();
        let refused =
            acknowledge_residue(&cx, &root, super::super::tests::id(1), Some(&effigy_home))
                .expect_err("live owner_identity must refuse acknowledgement");
        assert!(refused.to_string().contains(&owner_pid.to_string()), "{refused}");
        assert!(env_dir.join("compose-fixture.env").exists());

        // Simulate the crashed owner & detached supervisor process having exited cleanly.
        let mut dead_owner = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let dead_owner_id = ident::process_identity(dead_owner.id())
            .verified()
            .cloned()
            .unwrap();
        let _ = dead_owner.kill();
        let _ = dead_owner.wait();
        {
            let store = Store::open(&cx, &root).unwrap();
            store
                .conn()
                .execute(
                    "UPDATE runs SET owner_identity=?2, supervisor_identity=?2 WHERE run_id=?1",
                    params![
                        super::super::tests::id(1).to_string(),
                        serde_json::to_string(&dead_owner_id).unwrap()
                    ],
                )
                .unwrap();
        }

        let outcome =
            acknowledge_residue(&cx, &root, super::super::tests::id(1), Some(&effigy_home))
                .expect("real crashed interpolated run should acknowledge cleanly");
        assert!(!env_dir.join("compose-fixture.env").exists());
        assert_eq!(outcome.run, super::super::tests::id(1));
        assert!(outcome.skipped_interpolation_files.is_empty());
    }

    #[test]
    fn acknowledge_clears_dead_interpolated_residue_and_unblocks_successor() {
        let home = effigy_test_support::short_tempdir("ack-ok-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let owner = test_owner("ack-unblock");
        let effigy_home = home.path().join("effigy-home");
        std::fs::create_dir(&effigy_home).unwrap();

        setup_closed_incomplete_interpolated_run(home.path(), &cx, &root, &owner, &effigy_home);

        let env_dir = effigy_home.join("compose-env");
        assert!(
            env_dir.join("compose-fixture.env").exists(),
            "orphaned interpolation file must survive settlement into Residue"
        );
        let msg = RunError::PredecessorUnsettled {
            run: super::super::tests::id(1),
            effects: Vec::new(),
        }
        .to_string();
        assert!(
            msg.contains(&format!(
                "effigy run acknowledge {}",
                super::super::tests::id(1)
            )),
            "{msg}"
        );
        assert!(!msg.contains("stop what holds it"), "{msg}");

        let outcome =
            acknowledge_residue(&cx, &root, super::super::tests::id(1), Some(&effigy_home))
                .expect("dead interpolated residue should acknowledge cleanly");
        assert_eq!(outcome.run, super::super::tests::id(1));
        assert_eq!(
            outcome.removed_interpolation_files,
            vec![env_dir.join("compose-fixture.env")]
        );
        assert!(
            outcome.skipped_interpolation_files.is_empty(),
            "{:?}",
            outcome.skipped_interpolation_files
        );
        assert!(!env_dir.join("compose-fixture.env").exists());
        assert!(
            !std::fs::read_dir(&env_dir)
                .unwrap()
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with(".custody-")),
            "custody journal for the orphaned interpolation file should be removed"
        );

        let mut fake = super::super::tests::Fake::new(home.path());
        let successor = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner),
            super::super::tests::opts(3),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .expect("acknowledged residue must unblock successor runs");
        successor
            .settle(&super::super::tests::cleanup(&cx), &mut fake)
            .unwrap();
    }

    #[test]
    fn acknowledge_refuses_while_recorded_pid_is_alive_then_succeeds_after_exit() {
        let home = effigy_test_support::short_tempdir("ack-live-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let owner = test_owner("ack-live");
        let effigy_home = home.path().join("effigy-home");
        std::fs::create_dir(&effigy_home).unwrap();

        setup_closed_incomplete_interpolated_run(home.path(), &cx, &root, &owner, &effigy_home);

        let mut live_child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let live_pid = live_child.id();
        let live_identity = ident::process_identity(live_pid)
            .verified()
            .cloned()
            .expect("live child identity");
        let live_json = serde_json::to_string(&live_identity).unwrap();

        {
            let store = Store::open(&cx, &root).unwrap();
            store
                .conn()
                .execute(
                    "UPDATE runs SET sentinel_identity=?2 WHERE run_id=?1",
                    params![super::super::tests::id(1).to_string(), live_json],
                )
                .unwrap();
        }

        let refused = acknowledge_residue(&cx, &root, super::super::tests::id(1), Some(&effigy_home))
            .expect_err("must refuse acknowledgement while recorded sentinel PID is alive");
        let detail = refused.to_string();
        assert!(detail.contains(&live_pid.to_string()), "{detail}");
        assert!(detail.contains("signals nothing"), "{detail}");
        assert!(
            effigy_home
                .join("compose-env/compose-fixture.env")
                .exists(),
            "refused acknowledgement must leave orphaned interpolation file untouched"
        );

        let _ = live_child.kill();
        let _ = live_child.wait();

        let outcome =
            acknowledge_residue(&cx, &root, super::super::tests::id(1), Some(&effigy_home))
                .expect("once the recorded process exits, acknowledgement succeeds");
        assert_eq!(outcome.run, super::super::tests::id(1));
        assert!(!effigy_home
            .join("compose-env/compose-fixture.env")
            .exists());
    }

    #[test]
    fn acknowledge_refuses_open_complete_and_already_acknowledged_runs() {
        let home = effigy_test_support::short_tempdir("ack-rules-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let checkout = CheckoutId::new(CheckoutKind::EphemeralClone, [8; 16]);
        let task_owner = TaskId::new(checkout, "", Surface::Published, "task", None)
            .unwrap()
            .resource();
        let mut fake = super::super::tests::Fake::new(home.path());

        // 1. Open run is refused.
        let mut run1 = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(task_owner.clone()),
            super::super::tests::opts(1),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        let open_err = acknowledge_residue(&cx, &root, super::super::tests::id(1), None)
            .expect_err("open run cannot be acknowledged");
        assert!(open_err.to_string().contains("must be closed"), "{open_err}");

        // 2. Closed + complete run is refused.
        run1.finish(WorkloadOutcome::Succeeded).unwrap();
        run1.settle(&super::super::tests::cleanup(&cx), &mut fake)
            .unwrap();
        let complete_err = acknowledge_residue(&cx, &root, super::super::tests::id(1), None)
            .expect_err("complete run has no residue");
        assert!(
            complete_err.to_string().contains("already completed cleanup"),
            "{complete_err}"
        );

        // 3. Double acknowledgement is refused.
        let mut run2 = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(task_owner),
            super::super::tests::opts(2),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        fake.unknown = true;
        super::super::tests::apply(&mut run2, &fake, "owned");
        run2.finish(WorkloadOutcome::Succeeded).unwrap();
        let settled = run2.settle(&super::super::tests::cleanup(&cx), &mut fake).unwrap();
        assert_eq!(settled.cleanup(), CleanupOutcome::Incomplete);

        acknowledge_residue(&cx, &root, super::super::tests::id(2), None)
            .expect("first acknowledgement succeeds");
        let again = acknowledge_residue(&cx, &root, super::super::tests::id(2), None)
            .expect_err("second acknowledgement must refuse");
        assert!(
            again.to_string().contains("already acknowledged"),
            "{again}"
        );
    }

    #[test]
    fn acknowledge_reports_skipped_cleanup_when_binding_is_pending_and_compose_env_has_files() {
        let home = effigy_test_support::short_tempdir("ack-pending-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let owner = test_owner("ack-pending");
        let effigy_home = home.path().join("effigy-home");
        let env_dir = effigy_home.join("compose-env");
        std::fs::create_dir_all(&env_dir).unwrap();
        std::fs::set_permissions(&env_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let unbound_file = env_dir.join("compose-pending.env");
        {
            let _f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&unbound_file)
                .unwrap();
        }

        let mut fake = super::super::tests::Fake::new(home.path());
        let run = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner.clone()),
            super::super::tests::opts(1),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        let fence = run.fence().clone();
        run.settle(&super::super::tests::cleanup(&cx), &mut fake)
            .unwrap();

        let mut dead_child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let dead_id = ident::process_identity(dead_child.id())
            .verified()
            .cloned()
            .unwrap();
        let _ = dead_child.kill();
        let _ = dead_child.wait();

        let target_spec = EffectSpec {
            target: fence,
            kind: crate::custody::KIND.to_owned(),
            payload: Value::Null,
            settlement: Settlement::Release(Value::Null),
            required: true,
        };
        let pending_receipt = serde_json::json!({
            "interpolation_v1": {
                "identity": dead_id,
                "pgid": 0,
                "nonce": "pending-nonce",
                "grace_millis": 100,
            },
            "binding": { "pending": true },
        });
        {
            let store = Store::open(&cx, &root).unwrap();
            store
                .conn()
                .execute(
                    "UPDATE runs SET phase='closed', cleanup_outcome='incomplete', residue_ack=NULL WHERE run_id=?1",
                    [super::super::tests::id(1).to_string()],
                )
                .unwrap();
            store
                .conn()
                .execute(
                    "INSERT INTO events(run_id, seq, effect_id, kind, target, settlement, state, receipt, error, wall_time) \
                     VALUES (?1, 1, '1', ?2, ?3, ?4, 'residue', ?5, 'retained interpolation requires its live descriptor owner', 1)",
                    params![
                        super::super::tests::id(1).to_string(),
                        crate::custody::KIND,
                        serde_json::to_string(&target_spec).unwrap(),
                        serde_json::to_string(&Settlement::Release(Value::Null)).unwrap(),
                        serde_json::to_string(&pending_receipt).unwrap(),
                    ],
                )
                .unwrap();
        }

        let outcome =
            acknowledge_residue(&cx, &root, super::super::tests::id(1), Some(&effigy_home))
                .expect("pending binding should acknowledge while reporting skipped cleanup");
        assert!(outcome.removed_interpolation_files.is_empty());
        assert_eq!(outcome.skipped_interpolation_files.len(), 1);
        assert_eq!(
            outcome.skipped_interpolation_files[0].path.as_deref(),
            Some(env_dir.as_path())
        );
        assert!(
            outcome.skipped_interpolation_files[0]
                .reason
                .contains("pending"),
            "{:?}",
            outcome.skipped_interpolation_files[0]
        );
        assert!(
            unbound_file.exists(),
            "unverified unbound secret file must not be deleted silently"
        );

        let successor = Run::begin(
            &cx,
            &root,
            &super::super::tests::inv(owner),
            super::super::tests::opts(2),
            &super::super::tests::cleanup(&cx),
            &mut fake,
        )
        .expect("acknowledged pending residue unblocks successor");
        successor
            .settle(&super::super::tests::cleanup(&cx), &mut fake)
            .unwrap();
    }

    #[test]
    fn acknowledge_reports_skipped_cleanup_when_candidate_stamp_mismatches() {
        let home = effigy_test_support::short_tempdir("ack-mismatch-").unwrap();
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let owner = test_owner("ack-mismatch");
        let effigy_home = home.path().join("effigy-home");
        std::fs::create_dir(&effigy_home).unwrap();

        setup_closed_incomplete_interpolated_run(home.path(), &cx, &root, &owner, &effigy_home);

        // Replace compose-fixture.env with a distinct inode so the recorded stamp mismatches.
        let file_path = effigy_home.join("compose-env/compose-fixture.env");
        std::fs::remove_file(&file_path).unwrap();
        {
            let _replacement = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&file_path)
                .unwrap();
        }

        let outcome =
            acknowledge_residue(&cx, &root, super::super::tests::id(1), Some(&effigy_home))
                .expect("stamp mismatch acknowledges while reporting skipped cleanup");
        assert!(outcome.removed_interpolation_files.is_empty());
        assert_eq!(outcome.skipped_interpolation_files.len(), 1);
        assert_eq!(
            outcome.skipped_interpolation_files[0].path.as_deref(),
            Some(file_path.as_path())
        );
        assert!(
            outcome.skipped_interpolation_files[0]
                .reason
                .contains("inode stamp"),
            "{:?}",
            outcome.skipped_interpolation_files[0]
        );
        assert!(
            file_path.exists(),
            "foreign/replaced inode must never be deleted"
        );
    }
}
