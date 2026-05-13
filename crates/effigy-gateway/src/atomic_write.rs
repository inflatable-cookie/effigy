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

#[cfg(test)]
mod tests {
    use super::temp_path;
    use std::path::Path;

    #[test]
    fn temp_path_changes_between_calls_for_same_target() {
        let path = Path::new("/tmp/ports.json");
        let first = temp_path(path, "ports");
        let second = temp_path(path, "ports");

        assert_ne!(first, second);
        assert_eq!(first.parent(), path.parent());
        assert_eq!(second.parent(), path.parent());
    }
}
