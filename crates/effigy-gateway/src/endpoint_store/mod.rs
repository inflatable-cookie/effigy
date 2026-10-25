//! Gateway-owned endpoint authority: one table and one transaction.
//!
//! The Store holds the gateway's routes and loopback aliases. Host ports are
//! not part of it: the OS or Compose assigns them, and a route names the port
//! that is actually publishing.
mod snapshot;

pub use snapshot::EndpointSnapshot;

use std::net::Ipv4Addr;
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

    /// The committed snapshot, or the empty one before the first write.
    /// Unreadable state is reported, never replaced.
    pub fn snapshot(&self) -> Result<EndpointSnapshot, GatewayError> {
        match self.read() {
            Observed::Verified(snapshot, _) => Ok(snapshot),
            Observed::Absent(_) => Ok(EndpointSnapshot::default()),
            Observed::Unknown(reason) => Err(error(format!("observation unknown: {reason}"))),
        }
    }

    /// The one write path: read the snapshot under the owner gate, apply one
    /// edit, publish it and its endpoint claims together.
    pub fn transaction<T, E: From<GatewayError>>(
        &self,
        edit: impl FnOnce(&mut EndpointSnapshot) -> Result<T, E>,
    ) -> Result<T, E> {
        let op = operator::context(OPERATION_BUDGET, EffectClass::Mutate);
        let cx = op
            .effects()
            .map_err(|failure| error(format!("{failure}")))?;
        let cleanup = op
            .cleanup(CLEANUP_BUDGET)
            .map_err(|failure| error(format!("{failure}")))?;
        let root = StateRoot::runtime_machine_effects(&cx, &self.home).map_err(error)?;
        let mut facts = MachineFacts::begin(
            &cx,
            &root,
            FactOptions {
                owner: owner(),
                wait: Wait::Until(Instant::now() + OPERATION_BUDGET),
                next: operator::next(),
            },
            &cleanup,
            &mut operator::Adapter,
        )
        .map_err(error)?;
        let mut snapshot = match facts.state().map_err(error)? {
            Some(value) => EndpointSnapshot::from_value(value)?,
            None => EndpointSnapshot::default(),
        };
        let before = snapshot.clone();
        let value = edit(&mut snapshot)?;
        if snapshot == before {
            return Ok(value);
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

    /// Allocate the alias IP for `identity` when its environment comes up.
    ///
    /// Assignments whose checkout path is gone are reclaimed first, so a
    /// deleted worktree's address is reused on the next allocation. The
    /// assignment lives until `down` releases it.
    pub fn allocate_loopback(&self, identity: &str, scope: &str) -> Result<Ipv4Addr, GatewayError> {
        self.transaction(|snapshot| {
            let mut registry = snapshot.loopback()?;
            registry.reclaim_absent_paths();
            let ip = registry.allocate(identity, scope)?.ip;
            snapshot.set_loopback(&registry)?;
            Ok(ip)
        })
    }

    /// Release one alias when its environment goes down.
    pub fn release_loopback(&self, identity: &str) -> Result<bool, GatewayError> {
        self.transaction(|snapshot| {
            let mut registry = snapshot.loopback()?;
            let removed = registry.deallocate(identity).is_some();
            snapshot.set_loopback(&registry)?;
            Ok(removed)
        })
    }

    /// Release every alias owned by one checkout path.
    pub fn release_loopback_for_path(&self, scope: &str) -> Result<usize, GatewayError> {
        self.transaction(|snapshot| {
            let mut registry = snapshot.loopback()?;
            let released = registry.release_scope(scope);
            snapshot.set_loopback(&registry)?;
            Ok(released)
        })
    }
}

#[cfg(test)]
mod tests;
