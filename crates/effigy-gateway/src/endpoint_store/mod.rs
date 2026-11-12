//! Gateway-owned endpoint authority: one table and one transaction.
//!
//! The Store holds the gateway's routes and loopback aliases. Host ports are
//! not part of it: the OS or Compose assigns them, and a route names the port
//! that is actually publishing.
mod snapshot;

pub use snapshot::EndpointSnapshot;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_kernel::ctx::EffectClass;
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{Generation, ResourceId, Scope};
use effigy_kernel::observe::{Evidence, Observed, UnknownReason};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{FactOptions, MachineFacts};
use effigy_kernel::store::Store;

use crate::{operator, GatewayError};

/// One endpoint write's own budget; the caller never waits longer than this.
const OPERATION_BUDGET: Duration = Duration::from_secs(120);
const CLEANUP_BUDGET: Duration = Duration::from_secs(60);

pub(crate) fn error(reason: impl std::fmt::Display) -> GatewayError {
    GatewayError::EndpointStore {
        kind: crate::EndpointStoreRefusal::Refused,
        reason: reason.to_string(),
    }
}

/// The store could not be observed: nothing was read or changed, so absence is
/// unproven. The kind travels with the error; the text only names the reason.
pub(crate) fn observation_unknown(reason: impl std::fmt::Display) -> GatewayError {
    GatewayError::EndpointStore {
        kind: crate::EndpointStoreRefusal::ObservationUnknown,
        reason: reason.to_string(),
    }
}
pub(crate) fn owner() -> ResourceId {
    ResourceId::store(Scope::Machine, "gateway-endpoints").expect("fixed store identity")
}
fn evidence(revision: u64) -> Evidence {
    Evidence::new(
        "gateway endpoint store".into(),
        Some((owner(), Generation::new(revision))),
    )
    .expect("fixed evidence source")
}

/// One operator's gateway engine. Private fixtures use a separate home and
/// therefore a separate kernel store, without a second persistence format.
#[derive(Debug, Clone)]
pub struct EndpointStore {
    home: PathBuf,
}

impl EndpointStore {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self { home: home.into() }
    }

    /// Resolve the existing Effigy-home convention, including isolated
    /// private fixtures whose root intentionally is not named `.effigy`.
    pub fn for_effigy_home(root: &Path) -> Self {
        if root.file_name().is_some_and(|name| name == ".effigy") {
            return Self::new(root.parent().expect("Effigy home has a parent"));
        }
        Self::new(root)
    }

    /// Resolve the store from the route table path the gateway config carries.
    pub fn for_routes(path: &Path) -> Self {
        let directory = path.parent().expect("route source has a parent");
        if directory.file_name().is_some_and(|name| name == "gateway") {
            return Self::for_effigy_home(directory.parent().expect("gateway has a parent"));
        }
        Self::new(directory)
    }

    pub fn routes<T, E: From<GatewayError>>(
        &self,
        edit: impl FnOnce(&mut crate::routes::RouteTable) -> Result<T, E>,
    ) -> Result<T, E> {
        self.transaction(|snapshot| {
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

    /// Create the store's own directory (`<home>/.effigy/state`, 0700) before
    /// anything has been published. It is the directory the kernel store
    /// creates on its first transaction, and a gateway that starts first
    /// watches it for that transaction; no record is written here.
    pub fn prepare_directory(&self) -> Result<(), GatewayError> {
        let op = operator::context(Duration::from_secs(3), EffectClass::Mutate);
        let cx = op
            .effects()
            .map_err(|failure| error(format!("{failure}")))?;
        StateRoot::machine_effects(&cx, &self.home).map_err(error)?;
        Ok(())
    }

    /// Pure observation: no gate, recovery, directory or file creation.
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

    /// The committed snapshot, or the empty one before the first write.    /// Unreadable state is reported, never replaced.
    pub fn snapshot(&self) -> Result<EndpointSnapshot, GatewayError> {
        match self.read() {
            Observed::Verified(snapshot, _) => Ok(snapshot),
            Observed::Absent(_) => Ok(EndpointSnapshot::default()),
            Observed::Unknown(reason) => Err(observation_unknown(reason)),
        }
    }

    /// The one write path: read the snapshot under the owner gate, apply one
    /// edit, publish it and its endpoint claims together.
    pub fn transaction<T, E: From<GatewayError>>(
        &self,
        edit: impl FnOnce(&mut EndpointSnapshot) -> Result<T, E>,
    ) -> Result<T, E> {
        self.transaction_with_probe(edit, crate::registration::dead_claim_reason)
    }

    fn transaction_with_probe<T, E: From<GatewayError>>(
        &self,
        edit: impl FnOnce(&mut EndpointSnapshot) -> Result<T, E>,
        mut probe: impl FnMut(&crate::routes::Route, Instant) -> Result<Option<String>, GatewayError>,
    ) -> Result<T, E> {
        let deadline = Instant::now() + OPERATION_BUDGET;
        let op = operator::context(OPERATION_BUDGET, EffectClass::Mutate);
        let cx = op
            .effects()
            .map_err(|failure| error(format!("{failure}")))?;
        let cleanup = op
            .cleanup(CLEANUP_BUDGET)
            .map_err(|failure| error(format!("{failure}")))?;
        let root = StateRoot::runtime_machine_effects(&cx, &self.home).map_err(error)?;
        let begin = || {
            MachineFacts::begin(
                &cx,
                &root,
                FactOptions {
                    owner: owner(),
                    wait: Wait::Until(deadline),
                    next: operator::next(),
                },
                &cleanup,
                &mut operator::Adapter,
            )
            .map_err(error)
        };
        let mut facts = begin()?;
        let mut snapshot = match facts.state().map_err(error)? {
            Some(value) => EndpointSnapshot::from_value(value)?,
            None => EndpointSnapshot::default(),
        };
        let before = snapshot.clone();
        let value = edit(&mut snapshot)?;
        if snapshot == before {
            return Ok(value);
        }
        let current = before.routes()?;
        let next = snapshot.routes()?;
        let competing = next
            .all_routes()
            .into_iter()
            .filter_map(|route| {
                current
                    .lookup(&route.domain)
                    .filter(|existing| existing.project != route.project)
            })
            .collect::<Vec<_>>();
        if !competing.is_empty() {
            drop(facts);
            let probe_deadline =
                deadline.min(Instant::now() + crate::registration::TARGET_PROBE_TIMEOUT);
            let mut takeovers = Vec::new();
            for existing in competing {
                let Some(reason) = probe(existing, probe_deadline)? else {
                    return Err(GatewayError::ForeignRoute {
                        domain: existing.domain.clone(),
                        project: existing.project.clone(),
                    }
                    .into());
                };
                takeovers.push((existing, reason));
            }
            facts = begin()?;
            let latest = match facts.state().map_err(error)? {
                Some(value) => EndpointSnapshot::from_value(value)?,
                None => EndpointSnapshot::default(),
            };
            if latest != before {
                return Err(
                    error("gateway routes changed during observation; retry registration").into(),
                );
            }
            for (existing, reason) in takeovers {
                eprintln!(
                    "[info] gateway domain `{}` taken over from checkout `{}`: {}",
                    existing.domain, existing.project, reason
                );
            }
        }
        let desired = snapshot.claims()?;
        let generation = facts.next_generation().map_err(error)?;
        snapshot.revision = generation.get();
        facts
            .publish_retained(
                generation,
                &desired.into_values().collect::<Vec<_>>(),
                &serde_json::to_value(snapshot).map_err(error)?,
                Duration::ZERO,
            )
            .map_err(error)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
