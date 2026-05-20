use std::fs;
use std::path::PathBuf;

pub(super) fn temp_repo(prefix: &str, name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-{prefix}-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    root
}
