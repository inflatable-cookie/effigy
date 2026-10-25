//! Test-only endpoint fixtures. Runtime writers use EndpointStore exclusively.
use std::path::Path;

use effigy_gateway::endpoint_store::{EndpointSnapshot, EndpointStore};
use effigy_gateway::{loopback::LoopbackRegistry, routes::RouteTable, GatewayError};

pub trait LegacyRegistryFixture: serde::Serialize {
    fn store(path: &Path) -> EndpointStore;
    fn apply(&self, snapshot: &mut EndpointSnapshot) -> Result<(), GatewayError>;
    fn save(&self, path: &Path) -> Result<(), GatewayError> {
        Self::store(path).transaction(|snapshot| self.apply(snapshot))
    }
}
impl LegacyRegistryFixture for RouteTable {
    fn store(path: &Path) -> EndpointStore {
        EndpointStore::for_routes(path)
    }
    fn apply(&self, snapshot: &mut EndpointSnapshot) -> Result<(), GatewayError> {
        snapshot.set_routes(self)
    }
}
impl LegacyRegistryFixture for LoopbackRegistry {
    fn store(path: &Path) -> EndpointStore {
        EndpointStore::for_routes(&path.with_file_name("routes.json"))
    }
    fn apply(&self, snapshot: &mut EndpointSnapshot) -> Result<(), GatewayError> {
        snapshot.set_loopback(self)
    }
}
