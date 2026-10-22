//! Bounded, incremental retention (§7.4, K15). Runs only in Mutate, never on
//! the close path, and never removes an unsettled run, a referenced object or
//! a live claim.
//!
//! A sweep is *small*: candidates are selected by index in plain reads, and
//! deletes land in short batches of their own. [`sweep`] is rate-limited per
//! machine, so an admission pays a few milliseconds at most, and a backlog
//! catches up in short passes instead of one long write lock.
use crate::ctx::{EffectClass, EffectContext};
use crate::fs::FileName;
use crate::gate::{self, Gate, ScopeRemoval};
use crate::id::{CheckoutId, CheckoutKind, Scope};
use crate::ident;
use crate::observe::Observed;
use crate::root::{Family, StateRoot};
use crate::store::{wall_ms, BootDeadline, BusyWait, MaintainReport, Store, StoreError};
use rusqlite::{params, OptionalExtension};
use std::collections::BTreeSet;
use std::time::Duration;

/// The newest settled successful runs kept per owner (task), regardless of age.
pub const SETTLED_RUNS_KEPT: i64 = 20;
/// A settled successful run older than this is expired once it is outside its
/// owner's newest [`SETTLED_RUNS_KEPT`] (§7.4).
pub const SUCCESS_KEEP_MS: i64 = 24 * 60 * 60 * 1000;
/// A failed, cancelled, timed-out or incompletely cleaned run is kept this
/// long: enough to inspect a failure the next day (§7.4).
pub const FAILED_KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// The machine-wide history bound. A store with thousands of short-lived
/// checkout identities still stays proportionate: the newest settled runs win.
pub const SETTLED_RUNS_KEPT_TOTAL: i64 = 4_000;
pub const SUPERSEDED_OBJECTS_KEPT: i64 = 3;
pub const LOG_TOTAL_MAX: i64 = 2 << 30;
pub const TOMBSTONE_KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// An unreferenced resource untouched this long records nothing.
pub const RESOURCE_KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// A live checkout not seen this long, with no resource left, is tombstoned.
pub const CHECKOUT_KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// One sweep at most this often per machine, once the backlog is gone.
pub const SWEEP_INTERVAL_MS: i64 = 10 * 60 * 1000;
/// While a backlog remains, the next pass is due soon, so a big store catches
/// up in short passes without any close or admission holding a long lock.
pub const BACKLOG_INTERVAL_MS: i64 = 5 * 1000;
/// Rows per sweep. Each pass is short; the backlog takes as many passes as it
/// needs.
pub const SWEEP_ROWS: usize = 512;
/// Rows per short write transaction. A pass commits one batch at a time, so a
/// concurrent writer waits for a batch, never for a scan.
pub const DELETE_BATCH: usize = 16;
/// Owners inspected for excess history per pass; the cursor walks the rest.
const OWNERS_PER_SWEEP: usize = 64;
/// Scopes whose gate directory is examined per pass.
const SCOPES_PER_SWEEP: usize = 16;
/// Free pages released per pass, and the most a maintenance pass releases.
pub const VACUUM_PAGES: u32 = 8192;
const TREE_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionBudget {
    /// Most rows or directories removed by one pass.
    pub max_removals: usize,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetentionReport {
    pub runs: usize,
    pub log_sets: usize,
    pub objects: usize,
    pub claims: usize,
    pub resources: usize,
    pub checkouts: usize,
    pub gates: usize,
    /// Tombstoned scopes skipped because a guard in them still exists.
    pub busy_scopes: usize,
    /// The pass used its whole budget: a backlog remains.
    pub exhausted: bool,
    /// A sweep is not due yet; this call did nothing.
    pub skipped: bool,
    pub maintenance: MaintainReport,
}

/// "Settled" is `Closed` (§2): every effect terminal. A run referenced by a
/// claim that is not released is kept.
const SETTLED: &str = "phase='closed' \
    AND NOT EXISTS (SELECT 1 FROM claims c WHERE c.run_id=runs.run_id AND c.state<>'released')";
/// A run that finished its work and cleaned up after itself (§7.4).
const SUCCEEDED: &str = "cleanup_outcome='complete' AND workload_outcome='\"Succeeded\"'";
/// A closed run that is not a clean success: kept for a week.
const NOT_SUCCEEDED: &str = "NOT (cleanup_outcome='complete' AND workload_outcome='\"Succeeded\"')";

/// Whether a sweep is due. One indexed read; an admission asks nothing else.
pub fn due(store: &Store) -> Result<bool, StoreError> {
    Ok(match meta_value(store, "retention_next")?.and_then(|value| value.parse::<i64>().ok()) {
        Some(at) => wall_ms() >= at,
        None => true,
    })
}

/// A rate-limited, bounded retention pass for an admission to run (§7.4).
/// Best effort: a busy or unwritable store simply skips the pass, and the run
/// it belongs to is never delayed by it.
pub fn sweep(cx: &EffectContext, root: &StateRoot) {
    if cx.operation().class() != EffectClass::Mutate {
        return;
    }
    let budget = cx.child(std::time::Duration::from_secs(5));
    let Ok(mut store) = Store::open_within(&budget, root, BusyWait::Context) else {
        return;
    };
    if !due(&store).unwrap_or(false) {
        return;
    }
    // Claim the pass atomically: concurrent admissions that all saw it due
    // must not stack their sweeps, so exactly one runs it (§7.4).
    let claimed = store
        .transaction_within(
            &budget.child(std::time::Duration::from_secs(2)),
            BusyWait::Context,
            |txn| -> Result<usize, StoreError> {
                let now = wall_ms();
                let changed = txn.conn().execute(
                    "UPDATE meta SET value=?1 WHERE key='retention_next' \
                     AND CAST(value AS INTEGER) <= ?2",
                    params![now + SWEEP_INTERVAL_MS, now],
                )?;
                Ok(changed)
            },
        )
        .unwrap_or(0);
    if claimed != 1 {
        return;
    }
    let _ = retain(
        &budget,
        root,
        &mut store,
        RetentionBudget {
            max_removals: SWEEP_ROWS,
        },
    );
}

pub fn retain(
    cx: &EffectContext,
    root: &StateRoot,
    store: &mut Store,
    budget: RetentionBudget,
) -> Result<RetentionReport, StoreError> {
    if cx.operation().class() != EffectClass::Mutate {
        return Err(StoreError::ClassRefused(cx.operation().class()));
    }
    let mut report = RetentionReport::default();
    // Claim the pass before doing its work, so a concurrent sweep skips it.
    // A crash mid-pass leaves the steady interval, which is always safe.
    in_batch(store, cx, |txn| {
        txn.conn().execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('retention_at', ?1)",
            [wall_ms()],
        )?;
        txn.conn().execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('retention_next', ?1)",
            [wall_ms() + SWEEP_INTERVAL_MS],
        )?;
        Ok(())
    })?;
    if budget.max_removals == 0 {
        return Ok(report);
    }
    let now = wall_ms();
    let mut left = budget.max_removals;
    let runs_dir = root.family(Family::Runs)?.expect("created directory");
    let objects_dir = root.family(Family::Objects)?.expect("created directory");

    // Per-owner history: a task keeps its newest `SETTLED_RUNS_KEPT` successful
    // runs, and its failed runs are kept a week. A bounded slice of owners is
    // inspected per pass and the cursor advances, so no pass scans every owner
    // and every owner is visited in turn.
    let mut cursor = meta_value(store, "retention_owner")?.unwrap_or_default();
    let owners: Vec<(String, i64)> = store
        .conn()
        .prepare(&owner_history_sql())?
        .query_map(params![cursor, OWNERS_PER_SWEEP as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if owners.len() < OWNERS_PER_SWEEP {
        // The owner space is exhausted; the next pass starts over.
        cursor.clear();
    } else if let Some((last, _)) = owners.last() {
        cursor = last.clone();
    }
    let excess: Vec<String> = owners
        .iter()
        .filter(|(_, settled)| *settled > SETTLED_RUNS_KEPT)
        .scan(0usize, |taken, (owner, settled)| {
            let excess = (settled - SETTLED_RUNS_KEPT) as usize;
            let room = left.saturating_sub(*taken);
            let excess = excess.min(room);
            *taken += excess;
            Some((owner, excess))
        })
        .filter(|(_, excess)| *excess > 0)
        .flat_map(|(owner, excess)| {
            let mut statement = store
                .conn()
                .prepare(&owner_excess_sql())
                .expect("retention excess query");
            statement
                .query_map(
                    params![owner, now - SUCCESS_KEEP_MS, excess as i64],
                    |row| row.get(0),
                )
                .expect("retention excess rows")
                .collect::<Result<Vec<String>, _>>()
                .expect("retention excess rows")
        })
        .collect();
    left = delete_runs(store, cx, &runs_dir, &excess, left, &mut report)?;
    in_batch(store, cx, |txn| {
        txn.conn().execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('retention_owner', ?1)",
            [&cursor],
        )?;
        Ok(())
    })?;

    // Expired settled runs: a failure after a week, a success after a day once
    // it is outside its owner's newest twenty, and anything beyond the newest
    // machine-wide history. Log files go after each batch's own commit.
    let expired = expired_runs(store, now, left)?;
    left = delete_runs(store, cx, &runs_dir, &expired, left, &mut report)?;

    // Log cap: oldest settled runs' logs first, until the total fits.
    while left > 0 {
        let total: i64 = store
            .conn()
            .query_row("SELECT coalesce(sum(bytes),0) FROM logs", [], |row| {
                row.get(0)
            })?;
        if total <= LOG_TOTAL_MAX {
            break;
        }
        let oldest: Option<String> = OptionalExtension::optional(store.conn().query_row(
            &format!(
                "SELECT run_id FROM runs WHERE {SETTLED} AND EXISTS \
                 (SELECT 1 FROM logs WHERE logs.run_id=runs.run_id) \
                 ORDER BY created_at, run_id LIMIT 1"
            ),
            [],
            |row| row.get(0),
        ))?;
        let Some(id) = oldest else { break };
        in_batch(store, cx, |txn| {
            txn.conn()
                .execute("DELETE FROM logs WHERE run_id=?1", [&id])?;
            Ok(())
        })?;
        runs_dir.remove_tree(&FileName::new(&id)?, TREE_DEPTH)?;
        report.log_sets += 1;
        left -= 1;
    }

    // Objects not referenced by a desired or last-applied generation and not
    // among the newest 3 superseded per resource.
    let objects: Vec<String> = store
        .conn()
        .prepare(
            "SELECT digest FROM objects o WHERE NOT EXISTS (\
               SELECT 1 FROM events e WHERE e.kind='kernel:object-staging' AND \
               e.state IN ('intended','applied','unknown','residue') AND \
               json_extract(e.target,'$.payload')=o.digest) AND NOT EXISTS (\
               SELECT 1 FROM object_refs r WHERE r.digest=o.digest AND \
               (r.role IN ('desired','last_applied') OR (r.role='superseded' AND (\
                 SELECT count(*) FROM object_refs n WHERE n.resource=r.resource \
                 AND n.role='superseded' AND n.seq > r.seq) < ?1))) LIMIT ?2",
        )?
        .query_map(params![SUPERSEDED_OBJECTS_KEPT, left as i64], |row| {
            row.get(0)
        })?
        .collect::<Result<Vec<String>, _>>()?;
    if !objects.is_empty() {
        let object_guard = crate::objects::lock(cx, root)?;
        for digest in &objects {
            objects_dir.remove_tree(&FileName::new(digest)?, TREE_DEPTH + 4)?;
            objects_dir.sync()?;
            in_batch(store, cx, |txn| {
                txn.conn()
                    .execute("DELETE FROM objects WHERE digest=?1", [digest])?;
                Ok(())
            })?;
        }
        drop(object_guard);
    }
    report.objects = objects.len();
    left = left.saturating_sub(objects.len());

    // Released claims whose quarantine has ended on the boot clock. An
    // `Unknown` clock keeps them (R3).
    let now_clock = ident::boot_clock();
    let claims: Vec<(String, Option<String>, Option<i64>)> = store
        .conn()
        .prepare(
            "SELECT resource, quarantine_boot, quarantine_nanos FROM claims \
             WHERE state='released' LIMIT ?1",
        )?
        .query_map([left as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let ended: Vec<String> = claims
        .into_iter()
        .filter(|(_, boot, nanos)| match (boot, nanos) {
            (Some(boot), Some(nanos)) => matches!(
                BootDeadline {
                    boot: boot.clone(),
                    nanos: *nanos as u64
                }
                .passed(&now_clock),
                Observed::Verified(true, _)
            ),
            _ => true,
        })
        .map(|(resource, _, _)| resource)
        .collect();
    for batch in ended.chunks(DELETE_BATCH) {
        let names = batch.to_vec();
        let deleting = names.clone();
        in_batch(store, cx, move |txn| {
            let mut delete = txn.conn().prepare("DELETE FROM claims WHERE resource=?1")?;
            for name in &deleting {
                delete.execute([name])?;
            }
            Ok(())
        })?;
        report.claims += names.len();
        left = left.saturating_sub(names.len());
    }

    // Resources no row references any more: their runs are gone, their claims
    // are released and nothing is attached. This is what a store with one
    // owner row per task and per checkout depends on to stay proportionate.
    let stale: Vec<String> = store
        .conn()
        .prepare(
            "SELECT name FROM resources r WHERE r.updated_at <= ?1 \
             AND NOT EXISTS (SELECT 1 FROM runs x WHERE x.owner=r.name) \
             AND NOT EXISTS (SELECT 1 FROM claims c WHERE c.resource=r.name OR c.holder=r.name) \
             AND NOT EXISTS (SELECT 1 FROM attachments a WHERE a.environment=r.name) \
             AND NOT EXISTS (SELECT 1 FROM object_refs o WHERE o.resource=r.name) \
             LIMIT ?2",
        )?
        .query_map(params![now - RESOURCE_KEEP_MS, left as i64], |row| {
            row.get(0)
        })?
        .collect::<Result<Vec<String>, _>>()?;
    for batch in stale.chunks(DELETE_BATCH) {
        let names = batch.to_vec();
        let deleting = names.clone();
        in_batch(store, cx, move |txn| {
            let mut delete = txn.conn().prepare("DELETE FROM resources WHERE name=?1")?;
            for name in &deleting {
                delete.execute([name])?;
            }
            Ok(())
        })?;
        report.resources += names.len();
        left = left.saturating_sub(names.len());
    }

    // Checkouts nothing has used for a month become tombstones; the expiry
    // below then removes their scope and rows (§7.4).
    let tombstoned = in_batch(store, cx, |txn| {
        Ok(txn.conn().execute(
            "UPDATE checkouts SET state='tombstoned', tombstoned_at=coalesce(tombstoned_at, ?1) \
             WHERE token IN (SELECT token FROM checkouts c WHERE c.state='live' AND c.last_seen <= ?2 \
               AND NOT EXISTS (SELECT 1 FROM resources r WHERE r.checkout=c.token) LIMIT ?3)",
            params![now, now - CHECKOUT_KEEP_MS, left as i64],
        )?)
    })?;
    report.checkouts += tombstoned;

    // A tombstone whose last reference has ended loses its scope's gate
    // directory at once (under that scope gate); its rows follow after 30 days.
    let expired_checkouts = in_batch(store, cx, |txn| {
        txn.conn().execute(
            &format!(
                "UPDATE checkouts SET unreferenced_since = CASE WHEN EXISTS (\
                   SELECT 1 FROM resources r WHERE r.checkout=checkouts.token AND (\
                     EXISTS (SELECT 1 FROM runs WHERE runs.owner=r.name AND NOT ({SETTLED})) \
                     OR EXISTS (SELECT 1 FROM claims c WHERE (c.resource=r.name OR c.holder=r.name) \
                       AND c.state<>'released') \
                     OR EXISTS (SELECT 1 FROM attachments a WHERE a.environment=r.name \
                       AND a.state<>'released'))) \
                 THEN NULL ELSE coalesce(unreferenced_since, ?1) END \
                 WHERE state='tombstoned'"
            ),
            [now],
        )?;
        let mut statement = txn.conn().prepare(
            "SELECT token, kind FROM checkouts WHERE state='tombstoned' \
             AND unreferenced_since IS NOT NULL ORDER BY unreferenced_since LIMIT ?1",
        )?;
        let rows = statement
            .query_map([SCOPES_PER_SWEEP as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u8>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    for (token, kind) in expired_checkouts {
        let Some(checkout) = checkout_id(&token, kind) else {
            continue;
        };
        match Gate::remove_scope(cx, root, Scope::Checkout(checkout))? {
            ScopeRemoval::Busy => {
                report.busy_scopes += 1;
                continue;
            }
            ScopeRemoval::Removed => report.gates += 1,
            ScopeRemoval::Absent => {}
        }
        // The rows go when the tombstone itself has expired.
        let unreferenced_for: Option<i64> = OptionalExtension::optional(store.conn().query_row(
            "SELECT unreferenced_since FROM checkouts WHERE token=?1",
            [&token],
            |row| row.get(0),
        ))?;
        if unreferenced_for.is_none_or(|since| since > now - TOMBSTONE_KEEP_MS) {
            continue;
        }
        in_batch(store, cx, |txn| {
            let conn = txn.conn();
            conn.execute(
                "DELETE FROM runs WHERE owner IN (SELECT name FROM resources WHERE checkout=?1)",
                [&token],
            )?;
            conn.execute(
                "DELETE FROM object_refs WHERE resource IN \
                 (SELECT name FROM resources WHERE checkout=?1)",
                [&token],
            )?;
            conn.execute(
                "DELETE FROM attachments WHERE environment IN \
                 (SELECT name FROM resources WHERE checkout=?1)",
                [&token],
            )?;
            conn.execute(
                "DELETE FROM claims WHERE resource IN (SELECT name FROM resources WHERE checkout=?1)",
                [&token],
            )?;
            conn.execute("DELETE FROM resources WHERE checkout=?1", [&token])?;
            conn.execute("DELETE FROM checkouts WHERE token=?1", [&token])?;
            Ok(())
        })?;
        report.checkouts += 1;
        left = left.saturating_sub(1);
    }

    // Gate directories for scopes that no live checkout owns any more: a
    // removed or replaced checkout, or a scope whose rows are gone. The scope
    // gate proves no run holds a gate in it (§7.4).
    sweep_gates(cx, root, store, &mut report)?;

    // A bounded checkpoint and incremental vacuum keep the file proportionate
    // (§7.4); the maintenance itself never scans the journal.
    report.maintenance = store.maintain(cx, VACUUM_PAGES)?;
    report.exhausted = left == 0;
    in_batch(store, cx, |txn| {
        txn.conn().execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('retention_next', ?1)",
            [wall_ms()
                + if report.exhausted {
                    BACKLOG_INTERVAL_MS
                } else {
                    SWEEP_INTERVAL_MS
                }],
        )?;
        Ok(())
    })?;
    Ok(report)
}

/// Delete expired run rows and their `runs/<id>/` directories in short
/// batches. Returns the remaining budget.
fn delete_runs(
    store: &mut Store,
    cx: &EffectContext,
    runs_dir: &crate::fs::DirFd,
    ids: &[String],
    mut left: usize,
    report: &mut RetentionReport,
) -> Result<usize, StoreError> {
    for batch in ids.chunks(DELETE_BATCH) {
        let deleting = batch.to_vec();
        in_batch(store, cx, move |txn| {
            let mut delete = txn.conn().prepare("DELETE FROM runs WHERE run_id=?1")?;
            for id in &deleting {
                delete.execute([id])?;
            }
            Ok(())
        })?;
        for id in batch {
            runs_dir.remove_tree(&FileName::new(id)?, TREE_DEPTH)?;
        }
        report.runs += batch.len();
        left = left.saturating_sub(batch.len());
        // Give another writer the store between batches: a pass must never
        // starve a run's own admission or settle (§5.5).
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(left)
}

fn meta_value(store: &Store, key: &str) -> Result<Option<String>, StoreError> {
    Ok(OptionalExtension::optional(store.conn().query_row(
        "SELECT value FROM meta WHERE key=?1",
        [key],
        |row| row.get(0),
    ))?)
}

/// One short write transaction that waits only for its own context: retention
/// is optional work and must never stall the operation that triggered it.
fn in_batch<T>(
    store: &mut Store,
    cx: &EffectContext,
    f: impl FnOnce(&mut crate::store::Txn<'_>) -> Result<T, StoreError>,
) -> Result<T, StoreError> {
    store.transaction_within(
        &cx.child(std::time::Duration::from_secs(2)),
        BusyWait::Context,
        f,
    )
}

/// Owners whose successful history exceeds [`SETTLED_RUNS_KEPT`], walked in
/// name order by the `runs_owner` index: bounded by `LIMIT`, never a scan of
/// the journal into a write transaction (§7.4).
pub(crate) fn owner_history_sql() -> String {
    format!(
        "SELECT owner, count(*) FROM runs WHERE owner > ?1 AND {SETTLED} AND {SUCCEEDED} \
         GROUP BY owner ORDER BY owner LIMIT ?2"
    )
}
/// One owner's oldest successful runs beyond [`SETTLED_RUNS_KEPT`], past the
/// 24 h window: index-driven by `(owner, created_at)`.
pub(crate) fn owner_excess_sql() -> String {
    format!(
        "SELECT run_id FROM runs WHERE owner=?1 AND {SETTLED} AND {SUCCEEDED} \
         AND coalesce(closed_at, created_at) < ?2 ORDER BY created_at, run_id LIMIT ?3"
    )
}
/// Settled runs beyond the machine-wide history bound: one ordered range
/// through `runs_settled`, oldest first.
pub(crate) fn history_bound_sql() -> String {
    format!(
        "SELECT run_id FROM runs WHERE created_at < ?1 AND {SETTLED} \
         ORDER BY created_at LIMIT ?2"
    )
}
/// Closed failures past their seven-day window.
pub(crate) fn failed_age_sql() -> String {
    format!(
        "SELECT run_id FROM runs WHERE created_at < ?1 AND {SETTLED} AND {NOT_SUCCEEDED} \
         AND coalesce(closed_at, created_at) < ?2 ORDER BY created_at LIMIT ?3"
    )
}
/// Expired settled runs, oldest first, by index: the seven-day failure bound
/// and the machine-wide history bound. A successful run is expired by its
/// owner's history (see [`retain`]), not here, so an owner's newest twenty
/// survive regardless of age.
fn expired_runs(store: &Store, now: i64, limit: usize) -> Result<Vec<String>, StoreError> {
    // The newest `SETTLED_RUNS_KEPT_TOTAL` settled runs are kept: the cutoff is
    // the created_at of the last kept one, read backwards through the index.
    let cap: Option<i64> = OptionalExtension::optional(store.conn().query_row(
        "SELECT created_at FROM runs WHERE phase='closed' \
         ORDER BY created_at DESC, run_id DESC LIMIT 1 OFFSET ?1",
        params![SETTLED_RUNS_KEPT_TOTAL - 1],
        |row| row.get(0),
    ))?;
    let mut expired = BTreeSet::new();
    if let Some(cap) = cap {
        let mut statement = store.conn().prepare(&history_bound_sql())?;
        let rows = statement
            .query_map(params![cap, limit as i64], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        expired.extend(rows);
    }
    let mut statement = store.conn().prepare(&failed_age_sql())?;
    let rows = statement
        .query_map(
            params![now - FAILED_KEEP_MS, now - FAILED_KEEP_MS, limit as i64],
            |row| row.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    expired.extend(rows);
    let mut runs = expired.into_iter().collect::<Vec<_>>();
    runs.truncate(limit);
    Ok(runs)
}

/// Remove `gates/<scope>/` directories no live checkout owns, under each
/// scope's gate. A scope whose rows are gone, a tombstoned checkout and a
/// stale ephemeral checkout all release their entries here (§7.4).
fn sweep_gates(
    cx: &EffectContext,
    root: &StateRoot,
    store: &Store,
    report: &mut RetentionReport,
) -> Result<(), StoreError> {
    let Some(gates) = root.read_family(Family::Gates)? else {
        return Ok(());
    };
    // A scope with a checkout row belongs to the expiry loop above: it removes
    // the scope the moment the tombstone is unreferenced. This sweep owns the
    // directories no checkout row names at all.
    let known: BTreeSet<String> = store
        .conn()
        .prepare("SELECT token, kind FROM checkouts")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u8>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|(token, kind)| checkout_id(&token, kind))
        .map(|checkout| gate::scope_name(Scope::Checkout(checkout)))
        .collect();
    let mut removed = 0;
    for entry in gates.list(1 << 16)? {
        if removed >= SCOPES_PER_SWEEP {
            break;
        }
        let Some(name) = entry.to_str() else {
            continue;
        };
        if name.ends_with(".scope") || name == gate::scope_name(Scope::Machine) {
            continue;
        }
        if known.contains(name) {
            continue;
        }
        match Gate::remove_named_scope(cx, root, name)? {
            ScopeRemoval::Removed => removed += 1,
            ScopeRemoval::Busy => {
                report.busy_scopes += 1;
                continue;
            }
            ScopeRemoval::Absent => continue,
        }
    }
    report.gates += removed;
    Ok(())
}

fn checkout_id(token: &str, kind: u8) -> Option<CheckoutId> {
    let kind = match kind {
        0 => CheckoutKind::Primary,
        1 => CheckoutKind::LinkedWorktree,
        2 => CheckoutKind::EphemeralClone,
        3 => CheckoutKind::Machine,
        _ => return None,
    };
    if token.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(token.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(CheckoutId::new(kind, bytes))
}
