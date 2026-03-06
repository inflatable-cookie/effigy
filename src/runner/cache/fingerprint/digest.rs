use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::runner::error::RunnerError;

pub(super) fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hasher = Fnv1a64::new();
    hasher.update(bytes);
    hasher.finish_hex()
}

pub(super) fn digest_directory(root: &Path) -> Result<String, RunnerError> {
    let mut hasher = Fnv1a64::new();
    for entry in WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let rel_rendered = relative.to_string_lossy().replace('\\', "/");
        hasher.update(rel_rendered.as_bytes());

        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        if metadata.is_file() {
            hasher.update(b"f");
            let body = fs::read(path).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed reading cache directory input {}: {error}",
                    path.display()
                ))
            })?;
            hasher.update(&body);
        } else if metadata.is_dir() {
            hasher.update(b"d");
        }
    }
    Ok(hasher.finish_hex())
}

struct Fnv1a64 {
    state: u64,
}

impl Fnv1a64 {
    fn new() -> Self {
        Self {
            state: 0xcbf29ce484222325,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }

    fn finish_hex(&self) -> String {
        format!("{:016x}", self.state)
    }
}
