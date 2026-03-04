use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use super::digest::{digest_directory, fnv1a_hex};
use super::resolve::render_relative_or_absolute;
use super::PathStamp;
use crate::runner::RunnerError;

pub(super) fn stamp_path(catalog_root: &Path, path: &Path) -> Result<PathStamp, RunnerError> {
    let rendered = render_relative_or_absolute(catalog_root, path);
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(PathStamp {
            path: rendered,
            kind: "missing",
            exists: false,
            size: None,
            modified_epoch_ms: None,
            digest: None,
        });
    };

    if metadata.is_file() {
        let body = fs::read(path).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed reading cache input {}: {error}",
                path.display()
            ))
        })?;
        return Ok(PathStamp {
            path: rendered,
            kind: "file",
            exists: true,
            size: Some(metadata.len()),
            modified_epoch_ms: metadata_modified_epoch_ms(&metadata),
            digest: Some(fnv1a_hex(&body)),
        });
    }

    if metadata.is_dir() {
        let digest = digest_directory(path)?;
        return Ok(PathStamp {
            path: rendered,
            kind: "dir",
            exists: true,
            size: None,
            modified_epoch_ms: metadata_modified_epoch_ms(&metadata),
            digest: Some(digest),
        });
    }

    Ok(PathStamp {
        path: rendered,
        kind: "other",
        exists: true,
        size: None,
        modified_epoch_ms: metadata_modified_epoch_ms(&metadata),
        digest: None,
    })
}

fn metadata_modified_epoch_ms(metadata: &fs::Metadata) -> Option<u128> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
}
