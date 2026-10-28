//! In-memory guest filesystem for the workspace client's fake-backend proofs.
//!
//! Every node here is what a container-mode guest would see under
//! `/run/effigy` (or at the stable entry path). The fake applies the same
//! effects the Compose backend renders, so a test can prove publication,
//! immutability and that a read-only command changed nothing.

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::runner::error::RunnerError;

use super::context::{GuestReader, GuestWorkspace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FakeNode {
    pub(crate) kind: FakeNodeKind,
    pub(crate) bytes: Vec<u8>,
    pub(crate) mode: u32,
    pub(crate) owner: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FakeNodeKind {
    File,
    Dir,
    /// A symlink; `bytes` holds its target.
    Link,
}

#[derive(Debug, Clone)]
pub(crate) struct FakeGuest {
    nodes: BTreeMap<String, FakeNode>,
    container: super::context::WorkspaceContainerIdentity,
}

impl Default for FakeGuest {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            container: super::context::WorkspaceContainerIdentity {
                container_id: "fake-container".to_owned(),
                vm_boot_id: "fake-boot".to_owned(),
                platform: "x86_64".to_owned(),
            },
        }
    }
}

impl FakeGuest {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_container(
        mut self,
        container: super::context::WorkspaceContainerIdentity,
    ) -> Self {
        self.container = container;
        self
    }

    pub(crate) fn node(&self, path: &str) -> Option<&FakeNode> {
        self.nodes.get(path)
    }

    pub(crate) fn text(&self, path: &str) -> Option<String> {
        self.nodes
            .get(path)
            .filter(|node| node.kind == FakeNodeKind::File)
            .map(|node| String::from_utf8_lossy(&node.bytes).into_owned())
    }

    pub(crate) fn link_target(&self, path: &str) -> Option<String> {
        self.nodes
            .get(path)
            .filter(|node| node.kind == FakeNodeKind::Link)
            .map(|node| String::from_utf8_lossy(&node.bytes).into_owned())
    }

    pub(crate) fn mode(&self, path: &str) -> Option<u32> {
        self.nodes.get(path).map(|node| node.mode)
    }

    pub(crate) fn owner(&self, path: &str) -> Option<&str> {
        self.nodes.get(path)?.owner.as_deref()
    }

    /// Paths the guest holds, in order. Used to prove nothing new appeared.
    pub(crate) fn paths(&self) -> Vec<&str> {
        self.nodes.keys().map(String::as_str).collect()
    }

    /// Digest of every node's path, kind, mode, owner and bytes. Two equal
    /// digests mean the guest filesystem did not change for a read.
    pub(crate) fn tree_digest(&self) -> String {
        let mut digest = Sha256::new();
        for (path, node) in &self.nodes {
            digest.update(path.as_bytes());
            digest.update([node.kind as u8]);
            digest.update(node.mode.to_be_bytes());
            digest.update(node.owner.as_deref().unwrap_or("").as_bytes());
            digest.update((node.bytes.len() as u64).to_be_bytes());
            digest.update(&node.bytes);
        }
        let bytes: [u8; 32] = digest.finalize().into();
        super::context::render_digest(bytes)
    }

    /// Put a node directly, for fixtures that model a previous Prepare.
    pub(crate) fn put_file(&mut self, path: &str, bytes: impl Into<Vec<u8>>, mode: u32) {
        self.nodes.insert(
            path.to_owned(),
            FakeNode {
                kind: FakeNodeKind::File,
                bytes: bytes.into(),
                mode,
                owner: None,
            },
        );
    }

    pub(crate) fn put_link(&mut self, path: &str, target: &str) {
        self.nodes.insert(
            path.to_owned(),
            FakeNode {
                kind: FakeNodeKind::Link,
                bytes: target.as_bytes().to_vec(),
                mode: 0o777,
                owner: None,
            },
        );
    }
}

impl GuestReader for FakeGuest {
    fn read_file(&mut self, path: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        Ok(self
            .nodes
            .get(path)
            .filter(|node| node.kind == FakeNodeKind::File)
            .map(|node| node.bytes.clone()))
    }
}

impl GuestWorkspace for FakeGuest {
    fn ensure_dir(&mut self, path: &str, mode: u32) -> Result<(), RunnerError> {
        self.nodes.insert(
            path.to_owned(),
            FakeNode {
                kind: FakeNodeKind::Dir,
                bytes: Vec::new(),
                mode,
                owner: None,
            },
        );
        Ok(())
    }

    fn write_file(&mut self, path: &str, bytes: &[u8], mode: u32) -> Result<(), RunnerError> {
        self.put_file(path, bytes, mode);
        Ok(())
    }

    fn set_owner(&mut self, path: &str, owner: &str) -> Result<(), RunnerError> {
        let node = self.nodes.get_mut(path).ok_or_else(|| {
            RunnerError::task_invocation(format!("cannot give `{path}` to `{owner}`: not present"))
        })?;
        node.owner = Some(owner.to_owned());
        Ok(())
    }

    fn stage_host_file(&mut self, host_path: &Path, guest_path: &str) -> Result<(), RunnerError> {
        let bytes = std::fs::read(host_path).map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot read staged workspace file `{}`: {error}",
                host_path.display()
            ))
        })?;
        self.put_file(guest_path, bytes, 0o700);
        Ok(())
    }

    fn install_executable(&mut self, staging: &str, path: &str) -> Result<(), RunnerError> {
        let bytes = self
            .nodes
            .remove(staging)
            .map(|node| node.bytes)
            .ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "cannot install `{path}`: staging `{staging}` is missing"
                ))
            })?;
        self.put_file(path, bytes, 0o555);
        Ok(())
    }

    fn replace_link(&mut self, path: &str, target: &str) -> Result<(), RunnerError> {
        self.put_link(path, target);
        Ok(())
    }

    fn probe_container_identity(
        &mut self,
    ) -> Result<super::context::WorkspaceContainerIdentity, RunnerError> {
        Ok(self.container.clone())
    }

    fn replace_pointer(&mut self, path: &str, target: &str) -> Result<(), RunnerError> {
        let node = FakeNode {
            kind: self
                .nodes
                .get(path)
                .map(|node| node.kind)
                .unwrap_or(FakeNodeKind::File),
            bytes: target.as_bytes().to_vec(),
            mode: 0o644,
            owner: None,
        };
        self.nodes.insert(path.to_owned(), node);
        Ok(())
    }
}
