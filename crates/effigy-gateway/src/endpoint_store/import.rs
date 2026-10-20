use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use effigy_kernel::ctx::EffectClass;
use effigy_kernel::fs::{DirFd, FileName};
use effigy_kernel::observe::Observed;
use effigy_kernel::record::{read_record, Record};

use super::{error, EndpointSnapshot};
use crate::routes::RouteTableLock;
use crate::GatewayError;

#[derive(Debug, Clone)]
pub(super) struct Sources {
    pub routes: PathBuf,
    pub loopback: PathBuf,
    pub ports: PathBuf,
}
impl Sources {
    pub fn standard(home: &Path) -> Self {
        let root = home.join(".effigy");
        Self {
            routes: root.join("gateway/routes.json"),
            loopback: root.join("gateway/loopback-ips.json"),
            ports: root.join("ports.json"),
        }
    }
    pub(super) fn paths(&self) -> [(&'static str, &Path); 3] {
        [
            ("routes", &self.routes),
            ("loopback", &self.loopback),
            ("ports", &self.ports),
        ]
    }
    pub fn lock(&self, budget: std::time::Duration) -> Result<Vec<RouteTableLock>, GatewayError> {
        // Fixed ordering quiesces all three old writers for the first commit.
        let deadline = std::time::Instant::now() + budget;
        self.paths()
            .into_iter()
            .map(|(_, path)| RouteTableLock::acquire_until(path, deadline))
            .collect()
    }
    pub fn read(&self) -> Result<EndpointSnapshot, GatewayError> {
        let mut documents = BTreeMap::new();
        let mut hashes = BTreeMap::new();
        for (family, path) in self.paths() {
            if let Some(bytes) = read_input(path)? {
                documents.insert(
                    family.to_owned(),
                    serde_json::from_slice(&bytes).map_err(error)?,
                );
                hashes.insert(family.to_owned(), crate::ports::sha256_hex(&bytes));
            }
        }
        EndpointSnapshot::imported(documents, hashes)
    }
}
struct Bytes(Vec<u8>);
impl Record for Bytes {
    const MAX_BYTES: u64 = 8 << 20;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self(bytes.to_vec()))
    }
}
pub(crate) fn read_input(path: &Path) -> Result<Option<Vec<u8>>, GatewayError> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(error(e)),
        Ok(_) => {}
    }
    let op = crate::operator::context(std::time::Duration::from_secs(3), EffectClass::Observe);
    let cx = op.observe().expect("Observe context");
    let dir = DirFd::open(
        path.parent()
            .ok_or_else(|| error("import input has no parent"))?,
    )
    .map_err(error)?;
    let name = FileName::new(
        path.file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| error("invalid import name"))?,
    )
    .map_err(error)?;
    match read_record::<Bytes>(&cx, &dir, &name) {
        Observed::Verified(bytes, _) => Ok(Some(bytes.0)),
        Observed::Absent(_) => Err(error("import input disappeared while reading; retry")),
        Observed::Unknown(reason) => Err(error(format!(
            "import input {} unknown: {reason:?}",
            path.display()
        ))),
    }
}
