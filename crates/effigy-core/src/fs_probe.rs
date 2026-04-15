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
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    fn make_temp_dir(test_name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        dir.push(format!("effigy-fs-probe-{test_name}-{now}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn child_exists_and_is_file_checks_are_stable() {
        let root = make_temp_dir("exists-file");
        let file_path = root.join("package.json");
        let dir_path = root.join(".git");
        fs::write(&file_path, "{}\n").expect("write package json");
        fs::create_dir_all(&dir_path).expect("create .git dir");

        let mut probe = PathPresenceCache::new();
        assert!(probe.child_exists(&root, "package.json"));
        assert!(probe.child_is_file(&root, "package.json"));
        assert!(probe.child_exists(&root, ".git"));
        assert!(!probe.child_is_file(&root, ".git"));
        assert!(!probe.child_exists(&root, "Cargo.toml"));

        let _ = fs::remove_dir_all(root);
    }
}
