use std::path::{Path, PathBuf};

pub const GRAPH_DIR_NAME: &str = ".effigy/graph";
pub const GRAPH_DB_FILE_NAME: &str = "graph.db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphPaths {
    pub repo_root: PathBuf,
    pub graph_dir: PathBuf,
    pub db_path: PathBuf,
}

impl GraphPaths {
    pub fn for_repo(repo_root: &Path) -> Self {
        let graph_dir = repo_root.join(GRAPH_DIR_NAME);
        let db_path = graph_dir.join(GRAPH_DB_FILE_NAME);
        Self {
            repo_root: repo_root.to_path_buf(),
            graph_dir,
            db_path,
        }
    }
}
