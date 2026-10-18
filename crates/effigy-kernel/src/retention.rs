//! Bounded retention (§7.4, K15). Runs only in Mutate, bounded per call;
//! never removes an unsettled run, a referenced object or a live claim.
use crate::ctx::{EffectClass, EffectContext};
use crate::fs::FileName;
use crate::gate::{Gate, ScopeRemoval};
use crate::id::{CheckoutId, CheckoutKind, Scope};
use crate::ident;
use crate::observe::Observed;
use crate::root::{Family, StateRoot};
use crate::store::{BootDeadline, Store, StoreError};
use rusqlite::params;

pub const SETTLED_RUNS_KEPT: i64 = 20;
pub const SUPERSEDED_OBJECTS_KEPT: i64 = 3;
pub const LOG_TOTAL_MAX: i64 = 2 << 30;
pub const TOMBSTONE_KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;
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
    pub checkouts: usize,
    /// Tombstoned scopes skipped because a guard in them still exists.
    pub busy_scopes: usize,
    pub exhausted: bool,
}

/// "Settled" is `Closed` with `Complete` cleanup (§2). A run referenced by a
/// claim that is not released is kept.
const SETTLED: &str = "phase='closed' AND cleanup_outcome='complete' \
    AND NOT EXISTS (SELECT 1 FROM claims c WHERE c.run_id=runs.run_id AND c.state<>'released')";

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
    let mut left = budget.max_removals;
    let runs_dir = root.family(Family::Runs)?.expect("created directory");
    let objects_dir = root.family(Family::Objects)?.expect("created directory");

    // Runs beyond the newest 20 settled per owner, with their events,
    // requests and log rows (cascade). Log files go after the commit.
    let removed: Vec<String> = store.txn(cx, |txn| {
        let mut statement = txn.conn().prepare(&format!(
            "SELECT run_id FROM (SELECT run_id, row_number() OVER \
             (PARTITION BY owner ORDER BY created_at DESC, run_id DESC) AS rank \
             FROM runs WHERE {SETTLED}) WHERE rank > ?1 LIMIT ?2"
        ))?;
        let ids = statement
            .query_map(params![SETTLED_RUNS_KEPT, left as i64], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        for id in &ids {
            txn.conn()
                .execute("DELETE FROM runs WHERE run_id=?1", [id])?;
        }
        Ok(ids)
    })?;
    report.runs = removed.len();
    left -= removed.len().min(left);
    for id in &removed {
        runs_dir.remove_tree(&FileName::new(id)?, TREE_DEPTH)?;
    }

    // Log cap: oldest settled runs' logs first, until the total fits.
    while left > 0 {
        let oldest: Option<String> = store.txn(cx, |txn| {
            let total: i64 =
                txn.conn()
                    .query_row("SELECT coalesce(sum(bytes),0) FROM logs", [], |row| {
                        row.get(0)
                    })?;
            if total <= LOG_TOTAL_MAX {
                return Ok(None);
            }
            let oldest: Option<String> =
                rusqlite::OptionalExtension::optional(txn.conn().query_row(
                    &format!(
                        "SELECT run_id FROM runs WHERE {SETTLED} AND EXISTS \
                     (SELECT 1 FROM logs WHERE logs.run_id=runs.run_id) \
                     ORDER BY created_at, run_id LIMIT 1"
                    ),
                    [],
                    |row| row.get(0),
                ))?;
            if let Some(id) = &oldest {
                txn.conn()
                    .execute("DELETE FROM logs WHERE run_id=?1", [id])?;
            }
            Ok(oldest)
        })?;
        let Some(id) = oldest else { break };
        runs_dir.remove_tree(&FileName::new(&id)?, TREE_DEPTH)?;
        report.log_sets += 1;
        left -= 1;
    }

    // Objects not referenced by a desired or last-applied generation and not
    // among the newest 3 superseded per resource.
    let objects: Vec<String> = store.txn(cx, |txn| {
        let mut statement = txn.conn().prepare(
            "SELECT digest FROM objects o WHERE NOT EXISTS (\
               SELECT 1 FROM object_refs r WHERE r.digest=o.digest AND \
               (r.role IN ('desired','last_applied') OR (r.role='superseded' AND (\
                 SELECT count(*) FROM object_refs n WHERE n.resource=r.resource \
                 AND n.role='superseded' AND n.seq > r.seq) < ?1))) LIMIT ?2",
        )?;
        let digests = statement
            .query_map(params![SUPERSEDED_OBJECTS_KEPT, left as i64], |row| {
                row.get(0)
            })?
            .collect::<Result<Vec<String>, _>>()?;
        for digest in &digests {
            txn.conn()
                .execute("DELETE FROM objects WHERE digest=?1", [digest])?;
        }
        Ok(digests)
    })?;
    for digest in &objects {
        objects_dir.remove_tree(&FileName::new(digest)?, TREE_DEPTH)?;
    }
    report.objects = objects.len();
    left -= objects.len().min(left);

    // Released claims whose quarantine has ended on the boot clock. An
    // `Unknown` clock keeps them (R3).
    let now = ident::boot_clock();
    report.claims = store.txn(cx, |txn| {
        let mut statement = txn.conn().prepare(
            "SELECT resource, quarantine_boot, quarantine_nanos FROM claims \
             WHERE state='released' LIMIT ?1",
        )?;
        let rows = statement
            .query_map([left as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut removed = 0;
        for (resource, boot, nanos) in rows {
            let ended = match (boot, nanos) {
                (Some(boot), Some(nanos)) => matches!(
                    BootDeadline {
                        boot,
                        nanos: nanos as u64
                    }
                    .passed(&now),
                    Observed::Verified(true, _)
                ),
                _ => true,
            };
            if ended {
                txn.conn()
                    .execute("DELETE FROM claims WHERE resource=?1", [&resource])?;
                removed += 1;
            }
        }
        Ok(removed)
    })?;
    left -= report.claims.min(left);

    // Tombstones: track when the last reference ended; after 30 days remove
    // the scope's gate directory (under its scope gate), then the rows.
    let expired: Vec<(String, u8)> = store.txn(cx, |txn| {
        let now = txn.now();
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
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    for (token, kind) in expired {
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
        store.txn(cx, |txn| {
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
    report.exhausted = left == 0;
    Ok(report)
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
