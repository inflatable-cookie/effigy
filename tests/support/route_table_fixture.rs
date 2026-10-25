//! Test-only endpoint fixtures. Runtime writers use EndpointStore exclusively.
use std::path::Path;

use effigy_gateway::endpoint_store::{EndpointSnapshot, EndpointStore};
use effigy_gateway::{routes::RouteTable, GatewayError};

pub trait RouteTableFixture: serde::Serialize {
    fn store(path: &Path) -> EndpointStore;
    fn apply(&self, snapshot: &mut EndpointSnapshot) -> Result<(), GatewayError>;
    fn save(&self, path: &Path) -> Result<(), GatewayError> {
        Self::store(path).transaction(|snapshot| self.apply(snapshot))
    }
}
impl RouteTableFixture for RouteTable {
    fn store(path: &Path) -> EndpointStore {
        EndpointStore::for_routes(path)
    }
    fn apply(&self, snapshot: &mut EndpointSnapshot) -> Result<(), GatewayError> {
        snapshot.set_routes(self)
    }
}
