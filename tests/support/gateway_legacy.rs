//! Test-only 0.14.1 inputs. Runtime writers use EndpointStore exclusively.
use std::io::Write;
use std::path::Path;

use effigy_gateway::endpoint_store::{EndpointSnapshot, EndpointStore};
use effigy_gateway::{
    loopback::LoopbackRegistry, ports::PortRegistry, routes::RouteTable, GatewayError,
};

pub trait LegacyRegistryFixture: serde::Serialize {
    fn store(path: &Path) -> EndpointStore;
    fn apply(&self, snapshot: &mut EndpointSnapshot) -> Result<(), GatewayError>;
    fn save(&self, path: &Path) -> Result<(), GatewayError> {
        let store = Self::store(path);
        if store.read().verified().is_some() {
            return store.edit(None, |snapshot| self.apply(snapshot));
        }
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            let parent = path.parent().ok_or("fixture needs a parent")?;
            std::fs::create_dir_all(parent)?;
            let mut document = serde_json::to_value(self)?;
            if document.get("routes").is_some() {
                document["_managed_by"] =
                    serde_json::json!(effigy_gateway::routes::ROUTE_TABLE_MANAGED_MARKER);
            }
            let mut file = tempfile::NamedTempFile::new_in(parent)?;
            file.write_all(serde_json::to_string_pretty(&document)?.as_bytes())?;
            file.persist(path)?;
            Ok(())
        };
        write().map_err(|error| GatewayError::EndpointStore {
            reason: error.to_string(),
        })
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
impl LegacyRegistryFixture for PortRegistry {
    fn store(path: &Path) -> EndpointStore {
        EndpointStore::for_ports(path)
    }
    fn apply(&self, snapshot: &mut EndpointSnapshot) -> Result<(), GatewayError> {
        snapshot.set_ports(self)
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
