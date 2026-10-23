//! Gateway-owned endpoint authority. Legacy JSON changes only at explicit cutovers.
mod downgrade;
pub(crate) mod import;
mod reservations;
mod snapshot;

pub use snapshot::{EndpointSnapshot, Quarantine};

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{EffectClass, OperationContext};
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
    /// Resolve the existing Effigy-home convention, including isolated
    /// private fixtures whose root intentionally is not named `.effigy`.
    pub fn for_effigy_home(root: &Path) -> Self {
        if root.file_name().is_some_and(|name| name == ".effigy") {
            return Self::new(root.parent().expect("Effigy home has a parent"));
        }
        Self {
            home: root.to_owned(),
            sources: import::Sources {
                routes: root.join("gateway/routes.json"),
                loopback: root.join("gateway/loopback-ips.json"),
                ports: root.join("ports.json"),
            },
        }
    }
    /// The path identifies legacy cutover input/output, never a routine writer.
    pub fn for_routes(path: &Path) -> Self {
        let directory = path.parent().expect("route source has a parent");
        if directory.file_name().is_some_and(|name| name == "gateway") {
            return Self::for_effigy_home(directory.parent().expect("gateway has a parent"));
        }
        Self {
            home: directory.to_owned(),
            sources: import::Sources {
                routes: path.to_owned(),
                loopback: directory.join("loopback-ips.json"),
                ports: directory.join("ports.json"),
            },
        }
    }
    pub fn for_ports(path: &Path) -> Self {
        Self::for_effigy_home(path.parent().expect("port source has a parent"))
    }
    pub fn routes<T, E: From<GatewayError>>(
        &self,
        edit: impl FnOnce(&mut crate::routes::RouteTable) -> Result<T, E>,
    ) -> Result<T, E> {
        self.transaction(None, |snapshot| {
            let mut table = snapshot.routes()?;
            let result = edit(&mut table)?;
            snapshot.set_routes(&table)?;
            Ok(result)
        })
    }
    pub fn home(&self) -> &Path {
        &self.home
    }
    pub fn database_path(&self) -> PathBuf {
        self.home.join(".effigy/state/machine.db")
    }

    /// Pure observation: no import, gate, recovery, directory or file creation.
    pub fn read(&self) -> Observed<EndpointSnapshot> {
        let op = operator::context(Duration::from_secs(3), EffectClass::Observe);
        let cx = op.observe().expect("Observe context");
        if matches!(std::fs::symlink_metadata(&self.home), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
        {
            return Observed::Absent(effigy_kernel::observe::AbsenceProof::NoStateRoot);
        }
        let root = match StateRoot::runtime_machine_observe(&cx, &self.home) {
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

    /// Before import and after explicit handback, legacy reads remain pure. Unknown kernel state
    /// never falls back to a legacy file or an empty registry.
    pub fn read_compatible(&self) -> Result<EndpointSnapshot, GatewayError> {
        match self.read() {
            Observed::Verified(snapshot, _)
                if snapshot.authority == snapshot::Authority::Legacy =>
            {
                self.sources.read()
            }
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
        self.transaction(expected_revision, edit)
    }

    pub fn transaction<T, E: From<GatewayError>>(
        &self,
        expected_revision: Option<u64>,
        edit: impl FnOnce(&mut EndpointSnapshot) -> Result<T, E>,
    ) -> Result<T, E> {
        self.transaction_with_backup(expected_revision, |snapshot| {
            edit(snapshot).map(|value| (value, None))
        })
    }
    pub(crate) fn transaction_with_backup<T, E: From<GatewayError>>(
        &self,
        expected_revision: Option<u64>,
        edit: impl FnOnce(&mut EndpointSnapshot) -> Result<(T, Option<(PathBuf, Vec<u8>)>), E>,
    ) -> Result<T, E> {
        self.import()?;
        self.transact(expected_revision, edit)
    }
    fn transact<T, E: From<GatewayError>>(
        &self,
        expected_revision: Option<u64>,
        edit: impl FnOnce(&mut EndpointSnapshot) -> Result<(T, Option<(PathBuf, Vec<u8>)>), E>,
    ) -> Result<T, E> {
        self.with_run(|run, op| {
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
            if snapshot.authority != snapshot::Authority::Store {
                return Err(error("gateway endpoints were exported or export is incomplete; finish `effigy gateway setup --remove`, or run `effigy gateway setup` to re-import after stopping v0.14.1").into());
            }
            if expected_revision.is_some_and(|revision| revision != snapshot.revision) {
                return Err(error(format!(
                    "endpoint revision changed (current {}); read and retry",
                    snapshot.revision
                ))
                .into());
            }
            let before = snapshot.clone();
            let before_claims = if importing {
                Default::default()
            } else {
                before.claims()?
            };
            let (value, backup) = edit(&mut snapshot)?;
            if !importing && snapshot == before && backup.is_none() {
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
                Some(Claims::reserve(run, &wanted, Duration::ZERO).map_err(error)?)
            };
            // Reserve before any journaled external publication, so settle
            // resolves file effects before transferring the claim reservation.
            if let Some((path, bytes)) = backup {
                let dir = effigy_kernel::fs::DirFd::open(
                    path.parent().ok_or_else(|| error("backup has no parent"))?,
                )
                .map_err(error)?;
                let name = effigy_kernel::fs::FileName::new(
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .ok_or_else(|| error("invalid backup name"))?,
                )
                .map_err(error)?;
                run.replace_file(&dir, &name, &bytes, Default::default())
                    .map_err(error)?;
            }
            snapshot.revision = run.fence().generation.get();
            Claims::publish_retained(
                run,
                token.as_ref(),
                &desired.into_values().collect::<Vec<_>>(),
                &serde_json::to_value(snapshot).map_err(error)?,
                REUSE_QUARANTINE,
            )
            .map_err(error)?;
            Ok(value)
        })
    }

    fn with_run<T, E: From<GatewayError>>(
        &self,
        work: impl FnOnce(&mut Run<'_>, &OperationContext) -> Result<T, E>,
    ) -> Result<T, E> {
        let op = operator::context(OPERATION_BUDGET, EffectClass::Mutate);
        let cx = op.effects().map_err(|e| error(format!("{e:?}")))?;
        let recovery = op
            .cleanup(OPERATION_BUDGET + CLEANUP_BUDGET)
            .map_err(|e| error(format!("{e:?}")))?;
        let root = StateRoot::runtime_machine_effects(&cx, &self.home).map_err(error)?;
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
        let result = work(&mut run, &op).and_then(|value| {
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
        // Settle imported claims before a caller can release or republish
        // them. Even a first-command retirement must quarantine old endpoints.
        self.transact(None, |_| Ok::<_, GatewayError>(((), None)))?;
        self.read_compatible()
    }
}

#[cfg(test)]
mod tests;
