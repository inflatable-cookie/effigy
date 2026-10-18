//! A graph always owns one whole checkout, including declared catalog members.
use crate::{paths::GraphPaths, CodeGraphError};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphScope {
    root: PathBuf,
}

impl GraphScope {
    pub fn repo_root(root: &Path) -> Result<Self, CodeGraphError> {
        Ok(Self {
            root: root.canonicalize()?,
        })
    }

    /// Documentation context and code navigation share the same corpus.
    pub fn workspace(root: &Path) -> Result<Self, CodeGraphError> {
        Self::repo_root(root)
    }

    pub fn workspace_root(&self) -> &Path {
        &self.root
    }

    pub fn paths(&self) -> GraphPaths {
        GraphPaths::for_repo(&self.root)
    }

    pub(crate) fn key(&self) -> &str {
        ""
    }
}
