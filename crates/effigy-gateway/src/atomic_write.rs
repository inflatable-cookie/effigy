use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_path(path: &Path, purpose: &str) -> PathBuf {
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("effigy");

    path.with_file_name(format!(
        ".{filename}.{purpose}.{}.{}.tmp",
        std::process::id(),
        nanos.saturating_add(counter as u128)
    ))
}
