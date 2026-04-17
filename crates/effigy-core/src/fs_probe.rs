use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct PathPresenceCache {
    exists_cache: HashMap<PathBuf, bool>,
    file_cache: HashMap<PathBuf, bool>,
}

impl PathPresenceCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn exists(&mut self, path: &Path) -> bool {
        if let Some(found) = self.exists_cache.get(path) {
            return *found;
        }
        let found = path.exists();
        self.exists_cache.insert(path.to_path_buf(), found);
        found
    }

    pub fn is_file(&mut self, path: &Path) -> bool {
        if let Some(found) = self.file_cache.get(path) {
            return *found;
        }
        let found = path.is_file();
        self.file_cache.insert(path.to_path_buf(), found);
        found
    }

    pub fn child_exists(&mut self, parent: &Path, child: &str) -> bool {
        self.exists(&parent.join(child))
    }

    pub fn child_is_file(&mut self, parent: &Path, child: &str) -> bool {
        self.is_file(&parent.join(child))
    }
}

#[cfg(test)]
#[path = "fs_probe/tests.rs"]
mod tests;
