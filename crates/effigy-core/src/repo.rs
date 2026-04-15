use std::path::PathBuf;

pub fn current_working_dir() -> Result<PathBuf, std::io::Error> {
    std::env::current_dir()
}

pub fn canonicalize_or_original(path: &PathBuf) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or(path.clone())
}
