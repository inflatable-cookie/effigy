//! Gateway-owned endpoint authority. JSON registries are import inputs only.
mod import;
mod reservations;
mod snapshot;

pub use snapshot::{EndpointSnapshot, Quarantine};

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_kernel::ctx::EffectClass;
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{ResourceId, Scope};
use effigy_kernel::observe::{Evidence, Observed, UnknownReason};
use effigy_kernel::outcome::WorkloadOutcome;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{BeginOpts, Claims, Run};
use effigy_kernel::store::Store;

use crate::{operator, GatewayError};

/// DNS TTL (60 seconds) plus the proxy's ten-second drain budget.
pub const REUSE_QUARANTINE: Duration = Duration::from_secs(70);
const OPERATION_BUDGET: Duration = Duration::from_secs(120);
const CLEANUP_BUDGET: Duration = Duration::from_secs(60);

pub(crate) fn error(reason: impl std::fmt::Display) -> GatewayError {
    GatewayError::EndpointStore {
        reason: reason.to_string(),
    }
}
pub(crate) fn owner() -> ResourceId {
    ResourceId::store(Scope::Machine, "gateway-endpoints").expect("fixed store identity")
}
fn evidence(revision: u64) -> Evidence {
    Evidence::new(
        "gateway endpoint store".into(),
        Some((owner(), effigy_kernel::id::Generation::new(revision))),
    )
    .expect("fixed evidence source")
}

/// One operator's gateway engine. Private fixtures use a separate home and
/// therefore a separate kernel store, without a second persistence format.
#[derive(Debug, Clone)]
pub struct EndpointStore {
    home: PathBuf,
    sources: import::Sources,
}
impl EndpointStore {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        let home = home.into();
        Self {
            sources: import::Sources::standard(&home),
            home,
        }
    }
    pub fn home(&self) -> &Path {
        &self.home
    }
    pub fn database_path(&self) -> PathBuf {
        self.home.join(".effigy/state/kernel.db")
    }

    /// Pure observation: no import, gate, recovery, directory or file creation.
    pub fn read(&self) -> Observed<EndpointSnapshot> {
        let op = operator::context(Duration::from_secs(3), EffectClass::Observe);
        let cx = op.observe().expect("Observe context");
        let root = match StateRoot::observe(&cx, &self.home) {
            Observed::Verified(root, _) => root,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        let reader = match Store::read(&cx, &root) {
            Observed::Verified(reader, _) => reader,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        match reader.resource(&owner()) {
            Observed::Verified(row, _) => match row.state {
                Some(state) => match EndpointSnapshot::decode(&state) {
                    Ok(snapshot) => {
                        let revision = snapshot.revision;
                        Observed::Verified(snapshot, evidence(revision))
                    }
                    Err(e) => Observed::Unknown(UnknownReason::StoreUnreadable(e.to_string())),
                },
                None => Observed::Absent(effigy_kernel::observe::AbsenceProof::NoRow {
                    table: "gateway endpoint payload".into(),
                }),
            },
            Observed::Absent(proof) => Observed::Absent(proof),
            Observed::Unknown(reason) => Observed::Unknown(reason),
        }
    }

    /// Before first mutation, legacy reads remain pure. Unknown kernel state
    /// never falls back to a legacy file or an empty registry.
    pub fn read_compatible(&self) -> Result<EndpointSnapshot, GatewayError> {
        match self.read() {
            Observed::Verified(snapshot, _) => Ok(snapshot),
            Observed::Absent(_) => self.sources.read(),
            Observed::Unknown(reason) => Err(error(format!("observation unknown: {reason:?}"))),
        }
    }

    /// Import automatically, then mutate one complete snapshot under the
    /// kernel owner gate. A supplied revision provides optimistic fencing for
    /// callers that prepared their edit outside the owner gate.
    pub fn edit<T>(
        &self,
        expected_revision: Option<u64>,
        edit: impl FnOnce(&mut EndpointSnapshot) -> Result<T, GatewayError>,
    ) -> Result<T, GatewayError> {
        let op = operator::context(OPERATION_BUDGET, EffectClass::Mutate);
        let cx = op.effects().map_err(|e| error(format!("{e:?}")))?;
        let recovery = op
            .cleanup(OPERATION_BUDGET + CLEANUP_BUDGET)
            .map_err(|e| error(format!("{e:?}")))?;
        let root = StateRoot::effects(&cx, &self.home).map_err(error)?;
        let mut adapter = operator::Adapter;
        let mut run = Run::begin(
            &cx,
            &root,
            &operator::invocation(owner()),
            BeginOpts {
                id: operator::run_id().map_err(error)?,
                wait: Wait::Until(Instant::now() + OPERATION_BUDGET),
                acknowledge: vec![],
                next: operator::next(),
                custody: None,
            },
            &recovery,
            &mut adapter,
        )
        .map_err(error)?;
        // All returns after admission pass through the kernel's one settle.
        let result = (|| {
            let state = run.resource_state().map_err(error)?;
            let importing = state.is_none();
            let _legacy_locks = if importing {
                Some(self.sources.lock(op.remaining().unwrap_or_default())?)
            } else {
                None
            };
            let mut snapshot = match state {
                Some(value) => EndpointSnapshot::from_value(value)?,
                None => self.sources.read()?,
            };
            if expected_revision.is_some_and(|revision| revision != snapshot.revision) {
                return Err(error(format!(
                    "endpoint revision changed (current {}); read and retry",
                    snapshot.revision
                )));
            }
            let before = snapshot.clone();
            let before_claims = if importing {
                Default::default()
            } else {
                before.claims()?
            };
            let value = edit(&mut snapshot)?;
            if !importing && snapshot == before {
                return Ok(value);
            }
            let desired = snapshot.claims()?;
            snapshot.update_quarantine(&before_claims, &desired)?;
            let wanted = desired
                .iter()
                .filter(|(name, _)| !before_claims.contains_key(*name))
                .map(|(_, (resource, _))| resource.clone())
                .collect::<Vec<_>>();
            let token = if wanted.is_empty() {
                None
            } else {
                // No external consumer sees these rows before publication.
                // Failed unpublished reservations can therefore release at
                // once; published releases use REUSE_QUARANTINE below.
                Some(Claims::reserve(&mut run, &wanted, Duration::ZERO).map_err(error)?)
            };
            snapshot.revision = run.fence().generation.get();
            Claims::publish_retained(
                &mut run,
                token.as_ref(),
                &desired.into_values().collect::<Vec<_>>(),
                &serde_json::to_value(snapshot).map_err(error)?,
                REUSE_QUARANTINE,
            )
            .map_err(error)?;
            Ok(value)
        })();
        let result = result.and_then(|value| {
            run.finish(WorkloadOutcome::Succeeded).map_err(error)?;
            Ok(value)
        });
        // Gate admission and the edit must not consume settlement's budget.
        let cleanup = op.cleanup(CLEANUP_BUDGET).expect("fixed cleanup budget");
        let settled = run.settle(&cleanup, &mut adapter).map_err(error)?;
        settled
            .result()
            .as_ref()
            .map_err(|e| error(format!("{e:?}")))?;
        result
    }

    pub fn import(&self) -> Result<EndpointSnapshot, GatewayError> {
        self.edit(None, |_| Ok(()))?;
        self.read_compatible()
    }
}

#[cfg(test)]
mod tests;
