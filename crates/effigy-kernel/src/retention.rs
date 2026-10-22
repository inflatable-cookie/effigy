//! Bounded retention (§7.4, K15). Runs only in Mutate; never removes an
//! unsettled run, a referenced object or a live claim.
//!
//! A sweep is *cheap*: every candidate is selected by an index outside any
//! write transaction, and deletes land in short batches of their own. The
//! task-close path is throttled by [`due`], so a close never repeats a scan
//! another close just did, and a sweep's cost never grows with the store.
use crate::ctx::{EffectClass, EffectContext};
use crate::fs::FileName;
use crate::gate::{Gate, ScopeRemoval};
use crate::id::{CheckoutId, CheckoutKind, Scope};
use crate::ident;
use crate::observe::Observed;
use crate::root::{Family, StateRoot};
use crate::store::{wall_ms, BootDeadline, BusyWait, MaintainReport, Store, StoreError};
use rusqlite::{params, OptionalExtension};

pub const SETTLED_RUNS_KEPT: i64 = 20;
/// Bounded total history (K15). A store where every task of every checkout
/// has its own owner row keeps no run forever: the newest runs win and the
/// oldest settled runs beyond this bound are expired even inside an owner's
/// newest [`SETTLED_RUNS_KEPT`].
pub const SETTLED_RUNS_KEPT_TOTAL: i64 = 4_000;
pub const SUPERSEDED_OBJECTS_KEPT: i64 = 3;
pub const LOG_TOTAL_MAX: i64 = 2 << 30;
pub const TOMBSTONE_KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// A settled run older than this is expired regardless of rank.
pub const SETTLED_RUN_KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// An unreferenced resource untouched this long records nothing.
pub const RESOURCE_KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// A live checkout not seen this long, with no resource left, is tombstoned.
pub const CHECKOUT_KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// One sweep at most this often across the machine: a close skips a sweep
/// another process already ran (§7.4).
pub const SWEEP_INTERVAL_MS: i64 = 60_000;
/// Rows per short write transaction. Retention never holds the write lock
/// across a scan: each batch commits on its own.
pub const DELETE_BATCH: usize = 32;
/// Free pages released per sweep, and the most rows one sweep removes.
pub const VACUUM_PAGES: u32 = 1024;
const TREE_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionBudget {
    /// Most rows or directories removed by one call.
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
    /// Tombstoned scopes skipped because a guard in them still exists.
    pub busy_scopes: usize,
    pub exhausted: bool,
    /// A sweep is not due yet; this call did nothing.
    pub skipped: bool,
    pub maintenance: MaintainReport,
}

/// "Settled" is `Closed` with `Complete` cleanup (§2). A run referenced by a
/// claim that is not released is kept.
const SETTLED: &str = "phase='closed' AND cleanup_outcome='complete' \
    AND NOT EXISTS (SELECT 1 FROM claims c WHERE c.run_id=runs.run_id AND c.state<>'released')";

/// Whether a sweep is due. A close asks this first: the read is one indexed
/// lookup, so concurrent closes never stack retention scans (K15).
pub fn due(store: &Store) -> Result<bool, StoreError> {
    let last: Option<String> = store
        .conn()
        .query_row(
            "SELECT value FROM meta WHERE key='retention_at'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match last.and_then(|value| value.parse::<i64>().ok()) {
        Some(at) => wall_ms().saturating_sub(at) >= SWEEP_INTERVAL_MS,
        None => true,
    })
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
    // Claim the sweep before doing its work, so a concurrent close skips it.
    in_batch(store, cx, |txn| {
        txn.conn().execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('retention_at', ?1)",
            [wall_ms()],
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

    // Expired settled runs, with their events, requests and log rows (cascade).
    // Log files go after each batch's own commit.
    let expired = expired_runs(store, now, left)?;
    for batch in expired.chunks(DELETE_BATCH) {
        let ids = batch.to_vec();
        let deleting = ids.clone();
        in_batch(store, cx, move |txn| {
            let mut delete = txn.conn().prepare("DELETE FROM runs WHERE run_id=?1")?;
            for id in &deleting {
                delete.execute([id])?;
            }
            Ok(())
        })?;
        for id in &ids {
            runs_dir.remove_tree(&FileName::new(id)?, TREE_DEPTH)?;
        }
        report.runs += ids.len();
        left = left.saturating_sub(ids.len());
        cx.operation().check("retention runs").ok();
    }

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
        let oldest: Option<String> =
            rusqlite::OptionalExtension::optional(store.conn().query_row(
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
    let objects: Vec<String> = store.conn().prepare(
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

    // Tombstones: track when the last reference ended; after 30 days remove
    // the scope's gate directory (under its scope gate), then the rows.
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
             AND unreferenced_since IS NOT NULL AND unreferenced_since <= ?1 LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![now - TOMBSTONE_KEEP_MS, left as i64], |row| {
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
            ScopeRemoval::Removed | ScopeRemoval::Absent => {}
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
    // A bounded checkpoint and incremental vacuum keep the file proportionate
    // (§7.4); the maintenance itself never scans the journal.
    report.maintenance = store.maintain(cx, VACUUM_PAGES)?;
    report.exhausted = left == 0;
    Ok(report)
}

/// One short write transaction that waits only for its own context: retention
/// is optional work and must never stall the operation that triggered it.
fn in_batch<T>(
    store: &mut Store,
    cx: &EffectContext,
    f: impl FnOnce(&mut crate::store::Txn<'_>) -> Result<T, StoreError>,
) -> Result<T, StoreError> {
    store.transaction_within(cx, BusyWait::Context, f)
}

/// Expired settled runs, oldest first, by index: the age bound and the total
/// history bound. Never selects an unsettled run or one a claim still holds.
fn expired_runs(store: &Store, now: i64, limit: usize) -> Result<Vec<String>, StoreError> {
    let age_cutoff = now - SETTLED_RUN_KEEP_MS;
    // The newest `SETTLED_RUNS_KEPT_TOTAL` settled runs are kept: the cutoff is
    // the created_at of the last kept one, read backwards through the index.
    let cap: Option<i64> = OptionalExtension::optional(store.conn().query_row(
        "SELECT created_at FROM runs WHERE phase='closed' AND cleanup_outcome='complete' \
         ORDER BY created_at DESC, run_id DESC LIMIT 1 OFFSET ?1",
        params![SETTLED_RUNS_KEPT_TOTAL - 1],
        |row| row.get(0),
    ))?;
    let cutoff = cap.map_or(age_cutoff, |cap| cap.min(age_cutoff));
    let mut statement = store.conn().prepare(&format!(
        "SELECT run_id FROM runs WHERE created_at < ?1 AND {SETTLED} \
         ORDER BY created_at LIMIT ?2"
    ))?;
    let runs = statement
        .query_map(params![cutoff, limit as i64], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(runs)
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
