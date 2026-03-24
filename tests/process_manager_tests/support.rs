use effigy::process_manager::ProcessSpec;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn temp_workspace(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("effigy-process-{name}-{ts}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    root
}

pub(super) fn process_spec(name: &str, run: &str, cwd: &Path) -> ProcessSpec {
    ProcessSpec {
        name: name.to_owned(),
        run: run.to_owned(),
        cwd: cwd.to_path_buf(),
        start_after_ms: 0,
        shutdown_on_exit: false,
        pty: false,
        env: std::collections::BTreeMap::new(),
    }
}

pub(super) fn process_spec_with_delay(
    name: &str,
    run: &str,
    cwd: &Path,
    start_after_ms: u64,
) -> ProcessSpec {
    ProcessSpec {
        start_after_ms,
        ..process_spec(name, run, cwd)
    }
}
