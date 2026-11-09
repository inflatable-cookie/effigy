//! Scratch directories for the loader tests. Each one lives under the process
//! temporary directory (`TMPDIR` in an Effigy run) and is removed on drop.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct Scratch {
    root: PathBuf,
}

impl Scratch {
    pub fn new(label: &str) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "effigy-changelog-{label}-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("scratch directory");
        Self { root }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("scratch parent");
        }
        std::fs::write(&path, contents).expect("scratch write");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
