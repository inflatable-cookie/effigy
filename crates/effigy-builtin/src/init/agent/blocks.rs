//! Managed file and block editing: exact files, marker-delimited blocks, and
//! the `.gitignore` normalisation that keeps one local-state block.

use std::path::Path;

use super::check;
use super::{AgentCheck, AgentCheckStatus, AgentInitJob};
use crate::BuiltinError;

pub(super) fn ensure_exact_file(
    root: &Path,
    job: AgentInitJob,
    relative_path: &str,
    desired: &str,
    apply: bool,
) -> Result<AgentCheck, BuiltinError> {
    let path = root.join(relative_path);
    match std::fs::read_to_string(&path) {
        Ok(existing) if existing == desired => Ok(check(
            job,
            relative_path,
            AgentCheckStatus::Present,
            "none",
            None,
        )),
        Ok(_) => Ok(check(
            job,
            relative_path,
            AgentCheckStatus::Present,
            "preserve_existing",
            Some("existing file left untouched".to_owned()),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && apply => {
            write_file(&path, desired)?;
            Ok(check(
                job,
                relative_path,
                AgentCheckStatus::Created,
                "create_file",
                None,
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let status = if matches!(job, AgentInitJob::Manifest | AgentInitJob::Readme) {
                AgentCheckStatus::Missing
            } else {
                AgentCheckStatus::WouldCreate
            };
            Ok(check(job, relative_path, status, "create_file", None))
        }
        Err(error) => Err(BuiltinError::task_invocation_failed_read(&path, error)),
    }
}

pub(super) fn ensure_managed_block(
    root: &Path,
    job: AgentInitJob,
    relative_path: &str,
    start_marker: &str,
    end_marker: &str,
    desired_block: &str,
    apply: bool,
) -> Result<AgentCheck, BuiltinError> {
    let path = root.join(relative_path);
    match std::fs::read_to_string(&path) {
        Ok(existing) => {
            let next = if relative_path == ".gitignore"
                && desired_block == super::contract::GITIGNORE_BLOCK
            {
                normalize_effigy_gitignore_file(&existing, start_marker, end_marker, desired_block)
            } else if existing.contains(desired_block) {
                Some(existing.clone())
            } else {
                replace_or_append_block(&existing, start_marker, end_marker, desired_block)
            };
            let Some(next) = next else {
                return Ok(check(
                    job,
                    relative_path,
                    AgentCheckStatus::Stale,
                    "manual_repair",
                    Some(
                        "managed block start marker exists without matching end marker".to_owned(),
                    ),
                ));
            };
            if next == existing {
                return Ok(check(
                    job,
                    relative_path,
                    AgentCheckStatus::Present,
                    "none",
                    None,
                ));
            }
            if apply {
                write_file(&path, &next)?;
                return Ok(check(
                    job,
                    relative_path,
                    AgentCheckStatus::Updated,
                    "upsert_block",
                    None,
                ));
            }
            Ok(check(
                job,
                relative_path,
                AgentCheckStatus::WouldUpdate,
                "upsert_block",
                None,
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && apply => {
            write_file(&path, desired_block)?;
            Ok(check(
                job,
                relative_path,
                AgentCheckStatus::Created,
                "create_file",
                None,
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(check(
            job,
            relative_path,
            AgentCheckStatus::WouldCreate,
            "create_file",
            None,
        )),
        Err(error) => Err(BuiltinError::task_invocation_failed_read(&path, error)),
    }
}

fn replace_or_append_block(
    existing: &str,
    start_marker: &str,
    end_marker: &str,
    desired_block: &str,
) -> Option<String> {
    if let Some(start) = existing.find(start_marker) {
        let search_from = start + start_marker.len();
        let end = existing[search_from..].find(end_marker)? + search_from + end_marker.len();
        let mut next = String::new();
        next.push_str(existing[..start].trim_end());
        if !next.is_empty() {
            next.push_str("\n\n");
        }
        next.push_str(desired_block.trim_end());
        let suffix = existing[end..].trim_start();
        if !suffix.is_empty() {
            next.push_str("\n\n");
            next.push_str(suffix);
        } else {
            next.push('\n');
        }
        return Some(next);
    }

    let mut next = existing.trim_end().to_owned();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(desired_block.trim_end());
    next.push('\n');
    Some(next)
}

/// Keep exactly one managed local-state block in `.gitignore`, and fold the
/// loose `.effigy/` line a previous init may have written into it.
fn normalize_effigy_gitignore_file(
    existing: &str,
    start_marker: &str,
    end_marker: &str,
    desired_block: &str,
) -> Option<String> {
    let mut kept = Vec::new();
    let mut in_managed_block = false;
    let mut saw_managed_block = false;
    let mut removed_loose = 0usize;

    for line in existing.lines() {
        if line == start_marker {
            if in_managed_block {
                return None;
            }
            saw_managed_block = true;
            in_managed_block = true;
            continue;
        }
        if line == end_marker {
            if !in_managed_block {
                return None;
            }
            in_managed_block = false;
            continue;
        }
        if in_managed_block {
            continue;
        }
        if line.trim() == ".effigy/" {
            removed_loose += 1;
            continue;
        }
        kept.push(line);
    }
    if in_managed_block {
        return None;
    }

    let mut next = kept.join("\n").trim_end().to_owned();
    if !next.is_empty() && (saw_managed_block || removed_loose > 0) {
        next.push_str("\n\n");
    }
    if saw_managed_block || removed_loose > 0 {
        next.push_str(desired_block.trim_end());
        next.push('\n');
        return Some(next);
    }

    replace_or_append_block(existing, start_marker, end_marker, desired_block)
}

pub(super) fn write_file(path: &Path, contents: &str) -> Result<(), BuiltinError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| BuiltinError::task_invocation_failed_write(parent, error))?;
        }
    }
    std::fs::write(path, contents.as_bytes())
        .map_err(|error| BuiltinError::task_invocation_failed_write(path, error))
}
