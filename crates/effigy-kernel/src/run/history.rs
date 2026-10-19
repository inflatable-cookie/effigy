//! Pure journal reads for engine status projections (§5.6 status). Nothing
//! here repairs a run, probes a process or acquires a gate; callers combine a
//! row with `Gate::probe` of its owner.
use super::*;
use crate::id::Scope;
use crate::observe::{Evidence, Observed};
use crate::store::StoreReader;

/// One persisted run as its owner wrote it. `phase` is the stored phase:
/// `Interrupted` is derived by the reader from a free owner gate.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub id: RunId,
    pub generation: Generation,
    pub phase: Phase,
    pub workload: Option<WorkloadOutcome>,
    /// `None` until settle commits `Closed`.
    pub cleanup_complete: Option<bool>,
    /// The canonical invocation the run was admitted with, when decodable.
    pub invocation: Option<ResolvedInvocation>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub closed_at_ms: Option<i64>,
}

const COLUMNS: &str = "runs.run_id,runs.generation,runs.phase,runs.workload_outcome,\
    runs.cleanup_outcome,runs.invocation,runs.created_at,runs.updated_at,runs.closed_at";

/// An owner's runs, newest first, at most `limit`.
pub fn history(
    reader: &StoreReader,
    owner: &ResourceId,
    limit: usize,
) -> Observed<Vec<RunSummary>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM runs WHERE owner=?1 ORDER BY created_at DESC, generation DESC LIMIT ?2"
    );
    read(
        reader,
        &sql,
        params![owner.name().to_string(), limit as i64],
    )
}

/// One run by id; `Absent` when the journal has no such row (never run, or
/// removed by retention).
pub fn summary(reader: &StoreReader, id: RunId) -> Observed<RunSummary> {
    let sql = format!("SELECT {COLUMNS} FROM runs WHERE run_id=?1");
    match read(reader, &sql, params![id.to_string()]) {
        Observed::Verified(mut rows, evidence) => match rows.pop() {
            Some(row) => Observed::Verified(row, evidence),
            None => Observed::Absent(crate::observe::AbsenceProof::NoRow {
                table: "runs".into(),
            }),
        },
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}

/// The newest runs whose owner lives in `scope`, or in any scope, newest
/// first, at most `limit`, open or closed.
pub fn recent(
    reader: &StoreReader,
    scope: Option<Scope>,
    limit: usize,
) -> Observed<Vec<RunSummary>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM runs JOIN resources r ON r.name=runs.owner \
         WHERE ?1 IS NULL OR r.scope=?1 ORDER BY runs.created_at DESC, runs.run_id DESC LIMIT ?2"
    );
    read(
        reader,
        &sql,
        params![scope.map(crate::gate::scope_name), limit as i64],
    )
}

/// Every non-closed run whose owner lives in `scope`, oldest first. A free
/// owner gate makes such a run `Interrupted`, never absent.
pub fn unclosed(reader: &StoreReader, scope: Scope) -> Observed<Vec<RunSummary>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM runs JOIN resources r ON r.name=runs.owner \
         WHERE runs.phase<>'closed' AND r.scope=?1 ORDER BY runs.created_at, runs.run_id"
    );
    read(reader, &sql, params![crate::gate::scope_name(scope)])
}

fn read(
    reader: &StoreReader,
    sql: &str,
    params: impl rusqlite::Params,
) -> Observed<Vec<RunSummary>> {
    let rows = (|| -> Result<Vec<RunSummary>, RunError> {
        let mut statement = reader.conn().prepare(sql)?;
        let raw = statement
            .query_map(params, |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(
                |(
                    id,
                    generation,
                    phase,
                    workload,
                    cleanup,
                    invocation,
                    created,
                    updated,
                    closed,
                )| {
                    Ok(RunSummary {
                        id: id
                            .parse()
                            .map_err(|e: crate::id::IdentityError| RunError::Store(e.into()))?,
                        generation: Generation::new(generation as u64),
                        phase: Phase::parse(&phase)?,
                        workload: workload.map(|s| serde_json::from_str(&s)).transpose()?,
                        cleanup_complete: cleanup.map(|value| value == "complete"),
                        invocation: invocation
                            .and_then(|text| ResolvedInvocation::decode(text.as_bytes()).ok()),
                        created_at_ms: created,
                        updated_at_ms: updated,
                        closed_at_ms: closed,
                    })
                },
            )
            .collect()
    })();
    match rows {
        Ok(rows) => Observed::Verified(
            rows,
            Evidence::new("kernel run journal".into(), None).expect("fixed evidence"),
        ),
        Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
    }
}
