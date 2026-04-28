use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_workspace(prefix: &str, name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let seq = TEMP_WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("{prefix}-{name}-{ts}-{seq}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    root
}
