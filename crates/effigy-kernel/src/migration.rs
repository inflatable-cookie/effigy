//! Legacy-source inspection. Observation never acquires migration authority,
//! stages a destination, repairs a source, or signals a recorded process.
pub(crate) mod adoption;
mod control;
mod copy;
mod execute;
mod fence;
mod files;
pub use adoption::{authority, Authority, ImportReceipt, ImportedDestination};
pub use control::{request_legacy_stop, stop_legacy_writer, LegacyStop, WriterTarget};
pub use execute::{committed, import, CheckoutLocation, CheckoutResolver, ImportReport};
mod plan;
pub use fence::LegacyQuiescence;
pub use fence::{destination_blocker, DestinationBlocker};
pub use plan::{plan, ImportPlan, Partition};

use crate::ctx::ObserveContext;
use crate::fs::{FileName, Trust};
use crate::gate::{self, GateState, HolderDiag};
use crate::id::ResourceId;
use crate::ident::{self, ProcessIdentity, ProcessState};
use crate::observe::{Observed, UnknownReason};
use crate::root::{Family, StateRoot};
use crate::store::{Store, StoreError, SCHEMA_VERSION};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeMap;

const ENTRY_LIMIT: usize = 100_000;

#[derive(Debug)]
pub enum MigrationError {
    Store(StoreError),
    Blocked(Preflight),
    DestinationBlocked(DestinationBlocker),
}
impl From<StoreError> for MigrationError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}
impl From<crate::id::IdentityError> for MigrationError {
    fn from(error: crate::id::IdentityError) -> Self {
        Self::Store(error.into())
    }
}
impl From<crate::fs::FsError> for MigrationError {
    fn from(error: crate::fs::FsError) -> Self {
        Self::Store(error.into())
    }
}
impl From<crate::gate::GateError> for MigrationError {
    fn from(error: crate::gate::GateError) -> Self {
        Self::Store(error.into())
    }
}
impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => error.fmt(f),
            Self::Blocked(report) => write!(
                f,
                "legacy migration blocked by {} writer observations",
                report.blockers.len()
            ),
            Self::DestinationBlocked(report) => write!(
                f,
                "migration destination {} is busy: {}",
                report.checkout_root.display(),
                report.reason
            ),
        }
    }
}
impl std::error::Error for MigrationError {}

/// An exact legacy writer, or uncertainty that must be resolved before import.
/// Commands to stop/drain it belong to the invoking engine's typed CLI model.
#[derive(Debug, Clone, Serialize)]
pub struct Blocker {
    pub scope: String,
    pub resource: String,
    pub run: Option<String>,
    pub role: String,
    pub process: Observed<ProcessIdentity>,
    pub state: Observed<ProcessState>,
}

#[derive(Debug, Serialize)]
pub struct Preflight {
    pub blockers: Vec<Blocker>,
    pub resources: usize,
    pub checkouts: u64,
    pub unresolved_runs: usize,
    pub completed_runs: u64,
    pub writer_build: Option<String>,
}
impl Preflight {
    pub fn ready(&self) -> bool {
        self.blockers.is_empty()
    }
}

/// Inspect gates before opening SQLite: a legacy writer can hold its database
/// write lock while we still return its exact identity without waiting on it.
/// This is a preflight snapshot only; import must repeat it under all source
/// gates, with legacy launchers quiesced, before it creates any destination.
pub fn inspect(cx: &ObserveContext, root: &StateRoot) -> Result<Preflight, StoreError> {
    let legacy = root.legacy_source();
    let root = &legacy;
    cx.check("legacy migration preflight")
        .map_err(StoreError::Interrupted)?;
    let mut report = Preflight {
        blockers: inspect_gates(cx, root)?,
        resources: 0,
        checkouts: 0,
        unresolved_runs: 0,
        completed_runs: 0,
        writer_build: None,
    };
    // No read which might build SQLite's WAL index follows a busy preflight.
    // A quiesced source must be checkpointed by its owning old binary first.
    if !report.ready() {
        return Ok(report);
    }
    let reader = match Store::read_sealed(cx, root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Ok(report),
        Observed::Unknown(reason) => {
            return Err(StoreError::Corrupt(format!(
                "legacy source cannot be read without recovery: {reason:?}"
            )))
        }
    };
    if reader.schema_version() != SCHEMA_VERSION {
        return Err(StoreError::UnknownSchema {
            found: reader.schema_version(),
        });
    }
    // One SQLite snapshot covers all related rows, even if an unauthorized old
    // launcher violates quiescence. It still cannot make this snapshot authority.
    reader.conn().execute_batch("BEGIN")?;
    report.writer_build = match reader.last_written_by() {
        Observed::Verified(build, _) => Some(build),
        Observed::Absent(_) => None,
        Observed::Unknown(reason) => return Err(inconsistent(reason)),
    };
    let resources = inspect_resources(cx, reader.conn())?;
    report.resources = resources.len();
    report.checkouts = reader
        .conn()
        .query_row("SELECT count(*) FROM checkouts", [], |r| {
            r.get::<_, i64>(0).map(|count| count as u64)
        })?;
    report.completed_runs = reader.conn().query_row(
        "SELECT count(*) FROM runs WHERE phase='closed' AND cleanup_outcome='complete'",
        [],
        |r| r.get::<_, i64>(0).map(|count| count as u64),
    )?;
    let mut query = reader.conn().prepare(
        "SELECT run_id,owner,owner_identity,supervisor_identity,sentinel_identity FROM runs \
         WHERE phase<>'closed' OR cleanup_outcome IS NULL OR cleanup_outcome<>'complete'",
    )?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        cx.check("legacy run identity inspection")
            .map_err(StoreError::Interrupted)?;
        report.unresolved_runs += 1;
        if report.unresolved_runs > ENTRY_LIMIT {
            return Err(StoreError::Corrupt(
                "legacy run inspection limit exceeded".into(),
            ));
        }
        let run: String = row.get(0)?;
        run.parse::<crate::id::RunId>()?;
        let owner: String = row.get(1)?;
        let id = resources.get(&owner).ok_or_else(|| {
            StoreError::Corrupt(format!("legacy run {run} has no canonical owner"))
        })?;
        for (column, role) in [(2, "owner"), (3, "supervisor"), (4, "sentinel")] {
            let stored: Option<String> = row.get(column)?;
            let Some(stored) = stored else { continue };
            let identity: ProcessIdentity = serde_json::from_str(&stored).map_err(|error| {
                StoreError::Corrupt(format!("legacy {run} {role} identity: {error}"))
            })?;
            let current = ident::process_identity(identity.pid);
            match &current {
                Observed::Absent(_) => continue,
                Observed::Verified(fresh, _) if fresh != &identity => continue,
                _ => {}
            }
            let state = ident::process_state(identity.pid);
            // Zombies cannot write or retain user descriptors. Unknown remains
            // a blocker; it never authorizes stop, copying custody, or signalling.
            if matches!(state, Observed::Verified(ProcessState::Zombie, _)) {
                continue;
            }
            report.blockers.push(Blocker {
                scope: gate::scope_name(id.scope()),
                resource: owner.clone(),
                run: Some(run.clone()),
                role: role.into(),
                process: current,
                state,
            });
        }
    }
    Ok(report)
}

fn inspect_resources(
    cx: &ObserveContext,
    conn: &Connection,
) -> Result<BTreeMap<String, ResourceId>, StoreError> {
    let mut query = conn.prepare("SELECT name,canonical,kind,scope,checkout FROM resources")?;
    let mut rows = query.query([])?;
    let mut resources = BTreeMap::new();
    while let Some(row) = rows.next()? {
        cx.check("legacy resource inspection")
            .map_err(StoreError::Interrupted)?;
        if resources.len() == ENTRY_LIMIT {
            return Err(StoreError::Corrupt(
                "legacy resource inspection limit exceeded".into(),
            ));
        }
        let name: String = row.get(0)?;
        let canonical: Vec<u8> = row.get(1)?;
        let id = ResourceId::decode_canonical(&canonical)?;
        let checkout = match id.scope() {
            crate::id::Scope::Machine => None,
            crate::id::Scope::Checkout(checkout) => Some(crate::store::token(&checkout)),
        };
        if name != id.name().to_string()
            || row.get::<_, u8>(2)? != id.kind() as u8
            || row.get::<_, String>(3)? != gate::scope_name(id.scope())
            || row.get::<_, Option<String>>(4)? != checkout
        {
            return Err(StoreError::Corrupt(format!(
                "legacy resource {name} disagrees with its canonical identity"
            )));
        }
        resources.insert(name, id);
    }
    Ok(resources)
}

pub(crate) fn inspect_gates(
    cx: &ObserveContext,
    root: &StateRoot,
) -> Result<Vec<Blocker>, StoreError> {
    let Some(gates) = root.read_family(Family::Gates)? else {
        return Ok(Vec::new());
    };
    let mut blockers = Vec::new();
    let mut count = 0;
    for scope in gates.list(ENTRY_LIMIT)? {
        cx.check("legacy gate scope inspection")
            .map_err(StoreError::Interrupted)?;
        let scope = scope
            .to_str()
            .ok_or_else(|| StoreError::Corrupt("non-UTF8 gate scope".into()))?;
        if let Some(name) = scope.strip_suffix(".scope") {
            if name != "machine" && !gate_name(name) {
                return Err(StoreError::Corrupt("unexpected legacy scope inode".into()));
            }
            // The stable scope inode must be a trusted file too.
            if let Some((file, _)) =
                gates.open_file(&FileName::new(scope)?, libc::O_RDONLY, None, Trust::Private)?
            {
                if crate::fs::try_flock(&file, libc::LOCK_SH).map_err(|error| {
                    crate::fs::FsError::io("legacy scope probe", gates.path(), &error)
                })? {
                    crate::fs::unlock(&file);
                } else {
                    let reason = UnknownReason::Inconsistent("legacy scope is exclusively fenced; holder diagnostics are unavailable on scope locks".into());
                    blockers.push(Blocker {
                        scope: scope.trim_end_matches(".scope").into(),
                        resource: scope.into(),
                        run: None,
                        role: "scope fence".into(),
                        process: Observed::Unknown(reason.clone()),
                        state: Observed::Unknown(reason),
                    });
                }
            }
            continue;
        }
        if scope != "machine" && !gate_name(scope) {
            return Err(StoreError::Corrupt("unexpected legacy gate scope".into()));
        }
        let Some(dir) = gates.child(scope, Trust::Private, false)? else {
            continue;
        };
        for entry in dir.list(ENTRY_LIMIT)? {
            cx.check("legacy gate inspection")
                .map_err(StoreError::Interrupted)?;
            count += 1;
            if count > ENTRY_LIMIT {
                return Err(StoreError::Corrupt(
                    "legacy gate inspection limit exceeded".into(),
                ));
            }
            let rid = entry
                .to_str()
                .and_then(|name| name.strip_suffix(".gate"))
                .filter(|name| gate_name(name))
                .ok_or_else(|| StoreError::Corrupt("unexpected legacy gate entry".into()))?;
            match gate::probe_names(cx, root, scope, rid) {
                Observed::Verified(GateState::Free, _) | Observed::Absent(_) => {}
                Observed::Verified(GateState::Held(holder), _) => {
                    blockers.push(holder_blocker(scope, rid, *holder));
                }
                Observed::Unknown(reason) => {
                    blockers.push(holder_blocker(scope, rid, Observed::Unknown(reason)))
                }
            }
        }
    }
    Ok(blockers)
}

fn gate_name(name: &str) -> bool {
    name.len() == 32
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
}
fn holder_blocker(scope: &str, resource: &str, holder: Observed<HolderDiag>) -> Blocker {
    let (run, process, state) = match holder {
        Observed::Verified(holder, _) => {
            let process = match ident::process_identity(holder.pid) {
                Observed::Verified(identity, evidence)
                    if Some(identity.start.text()) == holder.start
                        && Some(identity.boot.as_str()) == holder.boot.as_deref() =>
                {
                    Observed::Verified(identity, evidence)
                }
                _ => Observed::Unknown(UnknownReason::Inconsistent(
                    "gate holder changed during preflight".into(),
                )),
            };
            (holder.run, process, holder.state)
        }
        Observed::Unknown(reason) => (
            None,
            Observed::Unknown(reason.clone()),
            Observed::Unknown(reason),
        ),
        Observed::Absent(_) => {
            let reason = UnknownReason::Inconsistent("held gate has no verified holder".into());
            (
                None,
                Observed::Unknown(reason.clone()),
                Observed::Unknown(reason),
            )
        }
    };
    Blocker {
        scope: scope.into(),
        resource: resource.into(),
        run,
        role: "gate holder".into(),
        process,
        state,
    }
}
fn inconsistent(reason: UnknownReason) -> StoreError {
    StoreError::Corrupt(format!("legacy inspection: {reason:?}"))
}

/// Metadata-only presence check for routing. An invalid existing file counts
/// as present and must be inspected, never silently treated as fresh state.
pub fn legacy_present(cx: &ObserveContext, root: &StateRoot) -> Result<bool, StoreError> {
    cx.check("legacy database presence")
        .map_err(StoreError::Interrupted)?;
    root.verify_binding()?;
    Ok(root
        .dir()
        .entry(&FileName::new(crate::store::DB_NAME)?)?
        .is_some())
}
