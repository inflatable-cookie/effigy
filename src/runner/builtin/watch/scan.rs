use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use globset::{Glob, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;

use super::super::super::RunnerError;

#[derive(Debug)]
pub(super) struct WatchMatcher {
    include: Option<GlobSet>,
    exclude: GlobSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileStamp {
    modified: Option<SystemTime>,
    size: u64,
}

pub(super) fn build_matcher(
    include: &[String],
    exclude: &[String],
) -> Result<WatchMatcher, RunnerError> {
    let include_set = if include.is_empty() {
        None
    } else {
        Some(build_glob_set(include, "include")?)
    };
    let mut excludes = vec![
        ".git/**".to_owned(),
        "node_modules/**".to_owned(),
        "target/**".to_owned(),
    ];
    excludes.extend(exclude.iter().cloned());
    let exclude_set = build_glob_set(&excludes, "exclude")?;
    Ok(WatchMatcher {
        include: include_set,
        exclude: exclude_set,
    })
}

pub(super) fn wait_for_changes(
    root: &Path,
    matcher: &WatchMatcher,
    snapshot: &mut HashMap<PathBuf, FileStamp>,
    debounce_ms: u64,
) -> Result<Vec<String>, RunnerError> {
    let debounce = Duration::from_millis(debounce_ms);
    let poll_ms = (debounce_ms / 4).clamp(50, 800);
    let poll = Duration::from_millis(poll_ms);
    let mut changed = BTreeSet::<String>::new();
    loop {
        std::thread::sleep(poll);
        let next = collect_snapshot(root, matcher)?;
        for rel in snapshot_diff(snapshot, &next) {
            changed.insert(rel);
        }
        *snapshot = next;
        if changed.is_empty() {
            continue;
        }
        let mut quiet_deadline = std::time::Instant::now() + debounce;
        loop {
            std::thread::sleep(poll);
            let next = collect_snapshot(root, matcher)?;
            let delta = snapshot_diff(snapshot, &next);
            *snapshot = next;
            if delta.is_empty() {
                if std::time::Instant::now() >= quiet_deadline {
                    return Ok(changed.into_iter().collect());
                }
            } else {
                for rel in delta {
                    changed.insert(rel);
                }
                quiet_deadline = std::time::Instant::now() + debounce;
            }
        }
    }
}

pub(super) fn collect_snapshot(
    root: &Path,
    matcher: &WatchMatcher,
) -> Result<HashMap<PathBuf, FileStamp>, RunnerError> {
    let mut snapshot = HashMap::<PathBuf, FileStamp>::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "watch scan failed under {}: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(path);
        if rel.as_os_str().is_empty() {
            continue;
        }
        let rel_for_match = normalize_for_match(rel);
        if !matcher.matches(&rel_for_match) {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = entry.metadata().map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "watch metadata read failed for {}: {error}",
                path.display()
            ))
        })?;
        snapshot.insert(
            rel.to_path_buf(),
            FileStamp {
                modified: metadata.modified().ok(),
                size: metadata.len(),
            },
        );
    }
    Ok(snapshot)
}

fn build_glob_set(patterns: &[String], label: &str) -> Result<GlobSet, RunnerError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            RunnerError::TaskInvocation(format!("invalid `{label}` glob `{pattern}`: {error}"))
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| {
        RunnerError::TaskInvocation(format!("failed to compile `{label}` glob set: {error}"))
    })
}

fn snapshot_diff(
    old: &HashMap<PathBuf, FileStamp>,
    new: &HashMap<PathBuf, FileStamp>,
) -> Vec<String> {
    let mut changed = BTreeSet::<String>::new();
    for (path, stamp) in new {
        if old.get(path) != Some(stamp) {
            changed.insert(path.to_string_lossy().replace('\\', "/"));
        }
    }
    for path in old.keys() {
        if !new.contains_key(path) {
            changed.insert(path.to_string_lossy().replace('\\', "/"));
        }
    }
    changed.into_iter().collect()
}

impl WatchMatcher {
    fn matches(&self, rel_path: &str) -> bool {
        let rel = rel_path.trim_start_matches("./");
        if self.exclude.is_match(rel) {
            return false;
        }
        match self.include.as_ref() {
            Some(include) => include.is_match(rel),
            None => true,
        }
    }
}

fn normalize_for_match(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
