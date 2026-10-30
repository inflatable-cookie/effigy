//! Journal rows are private to the owner-gated Run API.
use super::*;
use crate::store::Txn;
use rusqlite::{Connection, OptionalExtension};

#[derive(Debug)]
pub(super) struct RunRecord {
    pub id: RunId,
    pub owner: String,
    pub generation: Generation,
    pub phase: Phase,
    pub workload: Option<WorkloadOutcome>,
    pub acknowledged: bool,
}
pub(super) fn run(conn: &Connection, id: RunId) -> Result<RunRecord, RunError> {
    let row = conn.query_row("SELECT owner,generation,phase,workload_outcome,residue_ack IS NOT NULL FROM runs WHERE run_id=?1",
        [id.to_string()], |r| Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,bool>(4)?)))?;
    Ok(RunRecord {
        id,
        owner: row.0,
        generation: Generation::new(row.1 as u64),
        phase: Phase::parse(&row.2)?,
        workload: row.3.map(|s| serde_json::from_str(&s)).transpose()?,
        acknowledged: row.4,
    })
}
pub(super) fn predecessors(
    conn: &Connection,
    owner: &ResourceId,
) -> Result<Vec<RunRecord>, RunError> {
    // Acknowledged residue remains listed and retried, even after a successor.
    let mut stmt = conn.prepare("SELECT run_id FROM runs WHERE owner=?1 AND (phase<>'closed' OR cleanup_outcome IS NULL OR cleanup_outcome<>'complete') ORDER BY generation")?;
    let ids = stmt
        .query_map([owner.name().to_string()], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter()
        .map(|id| {
            run(
                conn,
                id.parse()
                    .map_err(|e: crate::id::IdentityError| RunError::Store(e.into()))?,
            )
        })
        .collect()
}
pub(super) fn check_fence(conn: &Connection, fence: &Fence) -> Result<(), RunError> {
    let current = conn
        .query_row(
            "SELECT generation FROM resources WHERE name=?1",
            [fence.resource.name().to_string()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|value| Generation::new(value as u64));
    if current != Some(fence.generation) {
        return Err(RunError::StaleGeneration {
            expected: fence.clone(),
            current,
        });
    }
    Ok(())
}
/// Workspace overlap authenticates a *run*, never a generic resource target.
/// Its immutable admission record survives mapping replacement, so an older
/// session can still receive controls and finish its own journal.
pub(super) fn check_run_fence(conn: &Connection, id: RunId, fence: &Fence) -> Result<(), RunError> {
    match check_fence(conn, fence) {
        Ok(()) => Ok(()),
        Err(error) => {
            let authorized: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM workspace_session_runs s \
                 JOIN runs r ON r.run_id=s.run_id \
                 LEFT JOIN workspace_activation_reservations a ON a.reservation_id=s.reservation_id \
                 WHERE s.run_id=?1 AND s.owner=?2 AND s.generation=?3 \
                 AND r.owner=s.owner AND r.generation=s.generation AND r.phase<>'closed' \
                 AND (s.reservation_id IS NULL OR a.state IN ('reserved','committed')))",
                params![id.to_string(), fence.resource.name().to_string(), fence.generation.get() as i64],
                |row| row.get(0),
            )?;
            if authorized {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

pub(super) fn check_run_target(
    conn: &Connection,
    id: RunId,
    owner: &Fence,
    effect: &Effect,
) -> Result<(), RunError> {
    if effect.spec.target == *owner {
        check_run_fence(conn, id, owner)
    } else {
        check_target(conn, effect)
    }
}

/// New remote work may use the still-published predecessor's exact Borrow
/// attachment. Generic targets and environment mutations keep strict fencing.
pub(super) fn check_intent_target(
    conn: &Connection,
    id: RunId,
    spec: &EffectSpec,
) -> Result<(), RunError> {
    match check_fence(conn, &spec.target) {
        Ok(()) => Ok(()),
        Err(error) => {
            let borrowing: bool = if spec.kind == crate::remote::KIND {
                conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM attachments WHERE run_id=?1 AND environment=?2 \
                     AND env_generation=?3 AND mode='\"Borrow\"' AND state='attached')",
                    params![
                        id.to_string(),
                        spec.target.resource.name().to_string(),
                        spec.target.generation.get() as i64
                    ],
                    |row| row.get(0),
                )?
            } else {
                false
            };
            if borrowing && crate::workspace_session::preserved_environment(conn, &spec.target)? {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

/// A callback's target check. A remote workload is bound to its environment
/// at intent; afterwards its acknowledgement and completion check that the
/// environment still exists, not its transition generation. Every bring-up
/// advances that generation, while container identity and VM boot (I-RC2)
/// and holder protection (R4) keep a stale session off a replacement (K7).
pub(super) fn check_target(conn: &Connection, effect: &Effect) -> Result<(), RunError> {
    if effect.spec.kind != crate::remote::KIND {
        return check_fence(conn, &effect.spec.target);
    }
    let exists = conn
        .query_row(
            "SELECT 1 FROM resources WHERE name=?1",
            [effect.spec.target.resource.name().to_string()],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Err(RunError::StaleGeneration {
            expected: effect.spec.target.clone(),
            current: None,
        });
    }
    Ok(())
}
pub(super) fn insert_effect(
    txn: &mut crate::store::Txn<'_>,
    id: RunId,
    spec: &EffectSpec,
) -> Result<i64, RunError> {
    let record = run(txn.conn(), id)?;
    if matches!(record.phase, Phase::Stopping | Phase::Closed) {
        return Err(RunError::refused("run is stopping"));
    }
    let sequence: i64 = txn.conn().query_row(
        "SELECT coalesce(max(seq),0)+1 FROM events WHERE run_id=?1",
        [id.to_string()],
        |r| r.get(0),
    )?;
    // A process or child-run intent ends positive NotStarted evidence before
    // any fork or child admission.
    if spec.kind == crate::custody::KIND || spec.kind == super::children::KIND {
        txn.conn().execute(
            "UPDATE runs SET workload_outcome=NULL WHERE run_id=?1",
            [id.to_string()],
        )?;
    }
    // target is the versioned typed effect envelope, including engine payload.
    txn.conn().execute("INSERT INTO events(run_id,seq,effect_id,kind,target,settlement,state,wall_time) VALUES (?1,?2,?3,?4,?5,?6,'intended',?7)",
        params![id.to_string(),sequence,sequence.to_string(),spec.kind,serde_json::to_string(spec)?,serde_json::to_string(&spec.settlement)?,txn.now()])?;
    txn.conn().execute("UPDATE runs SET phase=CASE WHEN phase='admitted' THEN 'starting' ELSE phase END,updated_at=?2 WHERE run_id=?1", params![id.to_string(),txn.now()])?;
    Ok(sequence)
}
pub(super) fn effects(conn: &Connection, id: RunId) -> Result<Vec<Effect>, RunError> {
    let mut stmt = conn.prepare(
        "SELECT seq,target,state,receipt,error FROM events WHERE run_id=?1 ORDER BY seq",
    )?;
    let rows = stmt
        .query_map([id.to_string()], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(sequence, spec, state, receipt, reason)| {
            Ok(Effect {
                sequence,
                spec: serde_json::from_str(&spec)?,
                state: serde_json::from_value(Value::String(state))?,
                receipt: receipt.map(|v| serde_json::from_str(&v)).transpose()?,
                reason,
            })
        })
        .collect()
}
pub(super) fn effect(conn: &Connection, id: RunId, seq: i64) -> Result<Effect, RunError> {
    effects(conn, id)?
        .into_iter()
        .find(|e| e.sequence == seq)
        .ok_or_else(|| RunError::broken("unknown effect ticket"))
}
pub(super) fn write_effect(
    txn: &Txn<'_>,
    id: RunId,
    seq: i64,
    state: EffectState,
    receipt: Option<&Value>,
    reason: Option<&str>,
) -> Result<(), RunError> {
    let state = serde_json::to_value(state)?;
    let state = state
        .as_str()
        .ok_or_else(|| RunError::broken("effect state encoding"))?;
    txn.conn().execute("UPDATE events SET state=?3,receipt=coalesce(?4,receipt),error=?5 WHERE run_id=?1 AND seq=?2",
        params![id.to_string(),seq,state,receipt.map(serde_json::to_string).transpose()?,reason])?;
    Ok(())
}
