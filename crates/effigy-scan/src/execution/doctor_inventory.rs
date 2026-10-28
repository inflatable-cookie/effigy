use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use globset::GlobSet;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::duplicate_blocks::{
    candidate_block_count, detect_duplicate_blocks_bounded, DuplicateBlockFile,
};
use super::ScanError;
use crate::model::*;
use crate::support::*;

#[derive(Debug, Clone)]
pub struct DoctorScanInventoryOptions {
    pub god_files: GodFileScanOptions,
    pub duplicate_blocks: DuplicateBlockScanOptions,
}

#[derive(Debug, Clone)]
pub struct DoctorScanInventoryResult {
    pub physical_walks: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub invalid_cache_entries: usize,
    pub warnings: Vec<String>,
    pub god_files: Option<GodFileScanResult>,
    pub duplicate_blocks: Option<DuplicateBlockScanResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Observation {
    rel_str: String,
    identity: String,
    bytes: usize,
    generated: bool,
    code_lines: usize,
    total_lines: usize,
    god_file_eligible: bool,
    duplicate_eligible: bool,
    normalized_lines: Vec<CachedNormalizedLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedNormalizedLine {
    line_number: usize,
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheGeneration {
    schema: String,
    owner: CacheOwner,
    config_identity: String,
    files: BTreeMap<String, Observation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CacheOwner {
    alias: String,
    scope_root: String,
}

const CACHE_BYTE_LIMIT: u64 = 128 * 1024 * 1024;
const CACHE_GENERATION_LIMIT: usize = 32;

struct PreparedOptions {
    god_files: Filter,
    duplicate_blocks: Filter,
}

struct Filter {
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
}

impl Filter {
    fn new(include: &[String], exclude: &[String]) -> Result<Self, ScanError> {
        Ok(Self {
            include: compile_glob_set(include, "include")?,
            exclude: compile_glob_set(exclude, "exclude")?,
        })
    }

    fn skips_code(&self, observation: &Observation) -> bool {
        should_skip_path(
            Path::new(&observation.rel_str),
            &observation.rel_str,
            self.include.as_ref(),
            self.exclude.as_ref(),
        )
    }
}

pub fn run_doctor_scan_inventory(
    workspace_root: &Path,
    scope_alias: &str,
    scope_root: &Path,
    pruned_roots: &[PathBuf],
    options: &DoctorScanInventoryOptions,
    refresh: bool,
    deadline: Option<Instant>,
) -> Result<DoctorScanInventoryResult, ScanError> {
    validate_options(options)?;
    let prepared = prepare_options(options)?;
    let respect_gitignore = enabled_gitignore_postures(options).all(|value| value);
    let config_identity = config_identity(scope_root, pruned_roots, options);
    let cache_paths = cache_paths(workspace_root, scope_alias, scope_root);
    let (cached_files, mut invalid_cache_entries, mut warnings) =
        load_cache(&cache_paths, &config_identity, refresh);
    let git_identities = git_identities(scope_root, deadline)?;
    let mut observations = Vec::new();
    let mut cache_hits = 0usize;
    let mut cache_misses = 0usize;
    walk_scan_files(
        scope_root,
        pruned_roots,
        respect_gitignore,
        &[],
        &[],
        should_skip_inventory_path,
        |path, rel, rel_str| {
            ensure_budget(deadline)?;
            let clean_git_identity = git_identities.get(rel_str).cloned();
            if let Some(identity) = clean_git_identity.as_deref() {
                if let Some(cached) = cached_files
                    .get(rel_str)
                    .filter(|cached| cached.identity == identity)
                {
                    cache_hits += 1;
                    observations.push(cached.clone());
                    return Ok(());
                }
            }
            let bytes = std::fs::read(path).map_err(|error| {
                ScanError::invocation(format!(
                    "doctor inventory read failed for {}: {error}",
                    path.display()
                ))
            })?;
            let identity = clean_git_identity.unwrap_or_else(|| content_identity(&bytes));
            if let Some(cached) = cached_files
                .get(rel_str)
                .filter(|cached| cached.identity == identity)
            {
                cache_hits += 1;
                observations.push(cached.clone());
                return Ok(());
            }
            cache_misses += 1;
            observations.push(analyze_file(rel, rel_str, &bytes, identity, &prepared));
            Ok(())
        },
    )?;
    ensure_budget(deadline)?;

    let god_files = options
        .god_files
        .doctor_enabled
        .then(|| evaluate_god_files(scope_root, &observations, &options.god_files, deadline))
        .transpose()?;
    let duplicate_blocks = options
        .duplicate_blocks
        .doctor_enabled
        .then(|| {
            evaluate_duplicate_blocks(
                scope_root,
                &observations,
                &options.duplicate_blocks,
                deadline,
            )
        })
        .transpose()?;
    ensure_budget(deadline)?;

    let files = observations
        .iter()
        .cloned()
        .map(|observation| (observation.rel_str.clone(), observation))
        .collect::<BTreeMap<_, _>>();
    if let Err(error) = publish_cache(
        &cache_paths,
        &CacheGeneration {
            schema: "effigy.doctor.cache.v2".to_owned(),
            owner: cache_paths.owner.clone(),
            config_identity: config_identity.clone(),
            files,
        },
    ) {
        invalid_cache_entries += 1;
        warnings.push(error);
    }

    Ok(DoctorScanInventoryResult {
        physical_walks: 1,
        cache_hits,
        cache_misses,
        invalid_cache_entries,
        warnings,
        god_files,
        duplicate_blocks,
    })
}

fn prepare_options(options: &DoctorScanInventoryOptions) -> Result<PreparedOptions, ScanError> {
    Ok(PreparedOptions {
        god_files: Filter::new(&options.god_files.include, &options.god_files.exclude)?,
        duplicate_blocks: Filter::new(
            &options.duplicate_blocks.include,
            &options.duplicate_blocks.exclude,
        )?,
    })
}

fn analyze_file(
    rel: &Path,
    rel_str: &str,
    bytes: &[u8],
    identity: String,
    options: &PreparedOptions,
) -> Observation {
    let contents = String::from_utf8_lossy(bytes);
    let generated = is_generated_artifact(rel, &contents);
    let lines = normalized_code_lines(rel, &contents);
    let stub = Observation {
        rel_str: rel_str.to_owned(),
        identity,
        bytes: bytes.len(),
        generated,
        code_lines: lines.len(),
        total_lines: contents.lines().count(),
        god_file_eligible: false,
        duplicate_eligible: false,
        normalized_lines: lines
            .into_iter()
            .map(|line| CachedNormalizedLine {
                line_number: line.line_number,
                text: line.text,
            })
            .collect(),
    };
    Observation {
        god_file_eligible: !options.god_files.skips_code(&stub),
        duplicate_eligible: !options.duplicate_blocks.skips_code(&stub),
        ..stub
    }
}

struct CachePaths {
    owner: CacheOwner,
    directory: PathBuf,
    current: PathBuf,
    lock: PathBuf,
}

fn cache_paths(workspace_root: &Path, scope_alias: &str, scope_root: &Path) -> CachePaths {
    let owner = CacheOwner {
        alias: scope_alias.to_owned(),
        scope_root: scope_root.to_string_lossy().into_owned(),
    };
    let directory = workspace_root
        .join(".effigy/doctor/cache/v2")
        .join(owner.directory_name());
    CachePaths {
        current: directory.join("generation.json"),
        lock: directory.join("publish.lock"),
        owner,
        directory,
    }
}

impl CacheOwner {
    fn directory_name(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.scope_root.as_bytes());
        let digest = hex_digest(hasher.finalize().as_slice());
        let alias = self
            .alias
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                    character
                } else {
                    '_'
                }
            })
            .collect::<String>();
        format!("{alias}-{}", &digest[..12])
    }
}

fn config_identity(
    scope_root: &Path,
    pruned_roots: &[PathBuf],
    options: &DoctorScanInventoryOptions,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"effigy.doctor.cache.v1\0scanner-implementation-v2\0");
    hasher.update(scope_root.to_string_lossy().as_bytes());
    for root in pruned_roots {
        hasher.update(b"\0prune\0");
        hasher.update(root.to_string_lossy().as_bytes());
    }
    hasher.update(format!("\0{options:?}").as_bytes());
    format!("sha256:{}", hex_digest(hasher.finalize().as_slice()))
}

fn content_identity(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!(
        "content:sha256:{}",
        hex_digest(hasher.finalize().as_slice())
    )
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn load_cache(
    paths: &CachePaths,
    config_identity: &str,
    refresh: bool,
) -> (BTreeMap<String, Observation>, usize, Vec<String>) {
    let path = &paths.current;
    if refresh || !std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file()) {
        return (BTreeMap::new(), 0, Vec::new());
    }
    let source = match std::fs::read(path) {
        Ok(source) => source,
        Err(error) => {
            return (
                BTreeMap::new(),
                1,
                vec![format!(
                    "doctor cache read failed for {} and will be rebuilt: {error}",
                    path.display()
                )],
            )
        }
    };
    let generation = match serde_json::from_slice::<CacheGeneration>(&source) {
        Ok(generation) => generation,
        Err(error) => {
            return (
                BTreeMap::new(),
                1,
                vec![format!(
                    "doctor cache content at {} is corrupt and will be rebuilt: {error}",
                    path.display()
                )],
            )
        }
    };
    if generation.schema != "effigy.doctor.cache.v2"
        || generation.owner != paths.owner
        || generation.config_identity != config_identity
    {
        return (
            BTreeMap::new(),
            1,
            vec![format!(
                "doctor cache content at {} is incompatible and will be rebuilt",
                path.display()
            )],
        );
    }
    (generation.files, 0, Vec::new())
}

fn publish_cache(paths: &CachePaths, generation: &CacheGeneration) -> Result<(), String> {
    publish_cache_with_limit(paths, generation, CACHE_BYTE_LIMIT)
}

fn publish_cache_with_limit(
    paths: &CachePaths,
    generation: &CacheGeneration,
    limit: u64,
) -> Result<(), String> {
    let encoded = serde_json::to_vec(generation)
        .map_err(|error| format!("doctor cache serialization failed: {error}"))?;
    // Reject symlinked or unreadable ancestors before even reading a cache.
    for ancestor in paths.directory.ancestors().take(5) {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) if !meta.file_type().is_dir() => {
                return Err(format!("doctor cache unowned path: {}", ancestor.display()))
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("doctor cache path unreadable: {error}"))
            }
            _ => {}
        }
    }
    match std::fs::symlink_metadata(&paths.current) {
        Ok(meta) if !meta.file_type().is_file() => {
            return Err(format!(
                "doctor cache unowned path: {}",
                paths.current.display()
            ))
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("doctor cache path unreadable: {error}"))
        }
        _ => {}
    }
    // Compare before creating even the publication lock: a warm deep scan is
    // an observation, and identical generations need no filesystem mutation.
    match std::fs::read(&paths.current) {
        Ok(existing) if existing == encoded => return Ok(()),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("doctor cache read failed: {error}"))
        }
        _ => {}
    }
    check_generation_size(encoded.len() as u64, limit)?;
    if paths.directory.exists() {
        let entries = std::fs::read_dir(&paths.directory)
            .map_err(|e| format!("doctor cache directory unreadable: {e}"))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("doctor cache directory unreadable: {e}"))?;
            if (entry.file_name() != "generation.json" && entry.file_name() != "publish.lock")
                || !entry.file_type().map_err(|e| e.to_string())?.is_file()
            {
                return Err(format!(
                    "doctor cache unowned entry: {}",
                    entry.path().display()
                ));
            }
        }
        if paths.current.exists() {
            let source = std::fs::read(&paths.current)
                .map_err(|e| format!("doctor cache read failed: {e}"))?;
            if serde_json::from_slice::<serde_json::Value>(&source)
                .ok()
                .and_then(|value| {
                    value
                        .get("schema")
                        .and_then(|schema| schema.as_str())
                        .map(str::to_owned)
                })
                .as_deref()
                == Some("effigy.doctor.cache.v1")
            {
                return Err(format!(
                    "doctor cache unowned generation: {}",
                    paths.current.display()
                ));
            }
            if let Ok(existing) = serde_json::from_slice::<CacheGeneration>(&source) {
                if existing.owner != paths.owner {
                    return Err(format!(
                        "doctor cache unowned generation: {}",
                        paths.current.display()
                    ));
                }
            }
        }
    }
    std::fs::create_dir_all(&paths.directory).map_err(|error| {
        format!(
            "doctor cache directory creation failed for {}: {error}",
            paths.directory.display()
        )
    })?;
    let lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&paths.lock)
        .map_err(|error| {
            format!(
                "doctor cache publication lock unavailable at {}: {error}",
                paths.lock.display()
            )
        })?;
    let result = publish_cache_locked(paths, &encoded).and_then(|changed| {
        if changed {
            evict_cache(paths, CACHE_BYTE_LIMIT, CACHE_GENERATION_LIMIT)
        } else {
            Ok(())
        }
    });
    drop(lock);
    let _ = std::fs::remove_file(&paths.lock);
    result
}

fn check_generation_size(size: u64, limit: u64) -> Result<(), String> {
    if size > limit {
        Err("doctor cache generation exceeds the 128 MiB limit; not published".to_owned())
    } else {
        Ok(())
    }
}

fn publish_cache_locked(paths: &CachePaths, encoded: &[u8]) -> Result<bool, String> {
    // A concurrent publisher may have installed these facts while we waited.
    if std::fs::read(&paths.current).ok().as_deref() == Some(encoded) {
        return Ok(false);
    }
    let temporary = paths.directory.join(format!(
        ".generation-{}-{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| {
            format!(
                "doctor cache temporary file creation failed for {}: {error}",
                temporary.display()
            )
        })?;
    let result = (|| {
        file.write_all(encoded)
            .map_err(|error| format!("doctor cache temporary write failed: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("doctor cache temporary sync failed: {error}"))?;
        std::fs::rename(&temporary, &paths.current).map_err(|error| {
            format!(
                "doctor cache atomic publication failed from {} to {}: {error}",
                temporary.display(),
                paths.current.display()
            )
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map(|()| true)
}

// Only a regular v2 generation whose embedded owner maps back to this
// directory is eligible. Any extra entry is unknown, not disposable cache.
fn inspect_generation(directory: &Path, allow_lock: bool) -> Result<(u64, SystemTime), String> {
    let metadata = std::fs::symlink_metadata(directory).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_dir() {
        return Err("not a directory".into());
    }
    for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name != "generation.json" && !(allow_lock && name == "publish.lock") {
            return Err("unknown directory entry".into());
        }
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            return Err("non-regular directory entry".into());
        }
    }
    let file = directory.join("generation.json");
    let metadata = std::fs::symlink_metadata(&file).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > CACHE_BYTE_LIMIT {
        return Err("unowned generation file".into());
    }
    let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
    let generation: CacheGeneration = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if generation.schema != "effigy.doctor.cache.v2"
        || directory.file_name() != Some(std::ffi::OsStr::new(&generation.owner.directory_name()))
    {
        return Err("unowned generation identity".into());
    }
    Ok((
        metadata.len(),
        metadata.modified().map_err(|e| e.to_string())?,
    ))
}

fn evict_cache(paths: &CachePaths, byte_limit: u64, generation_limit: usize) -> Result<(), String> {
    let root = paths
        .directory
        .parent()
        .ok_or("doctor cache root unavailable")?;
    let mut entries = Vec::new();
    let mut unknown = 0usize;
    let mut bytes = 0u64;
    for entry in
        std::fs::read_dir(root).map_err(|e| format!("doctor cache enumeration failed: {e}"))?
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                unknown += 1;
                continue;
            }
        };
        let directory = entry.path();
        match inspect_generation(&directory, true) {
            Ok((size, modified)) => {
                bytes = bytes.saturating_add(size);
                entries.push((modified, directory, size));
            }
            Err(_) => unknown += 1,
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut count = entries.len();
    let mut failed = 0usize;
    for (modified, directory, _) in entries {
        if bytes <= byte_limit && count <= generation_limit {
            break;
        }
        if directory == paths.directory {
            continue;
        }
        let lock_path = directory.join("publish.lock");
        let lock = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(lock) => lock,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        // Recheck after taking the publisher's lock: the generation may have
        // changed since enumeration. Never recursively delete a directory.
        let mut removed = false;
        match inspect_generation(&directory, true) {
            Ok((size, current_modified)) if current_modified == modified => {
                if std::fs::remove_file(directory.join("generation.json")).is_ok() {
                    bytes = bytes.saturating_sub(size);
                    count -= 1;
                    removed = true;
                } else {
                    failed += 1;
                }
            }
            _ => failed += 1,
        }
        drop(lock);
        if std::fs::remove_file(&lock_path).is_err() {
            failed += 1;
        }
        // This fails safely if another publisher or an unknown entry appeared.
        if removed {
            let _ = std::fs::remove_dir(&directory);
        }
    }
    let old_root = root.parent().map(|parent| parent.join("v1"));
    let legacy = old_root
        .as_ref()
        .is_some_and(|path| std::fs::symlink_metadata(path).is_ok());
    if unknown > 0 || failed > 0 || bytes > byte_limit || count > generation_limit || legacy {
        return Err(format!("doctor cache retained unowned or locked entries (unknown: {unknown}, unavailable: {failed}, v1 present: {legacy}); limits may not be met"));
    }
    Ok(())
}

/// Exact Git index identities for clean tracked files, bounded by the shared
/// doctor deadline.
///
/// Each owned `git` child runs in its own process group with the remaining
/// budget; on expiry the tree is terminated and reaped and a budget-exhausted
/// error is returned so the caller reports partial evidence without publishing
/// a partial cache generation. Spawn failures and error exits keep the
/// historical graceful fallback to content digests.
fn git_identities(
    scope_root: &Path,
    deadline: Option<Instant>,
) -> Result<BTreeMap<String, String>, ScanError> {
    let Some(tracked) = run_git_bounded(
        &[
            "-C",
            &scope_root.display().to_string(),
            "ls-files",
            "--stage",
            "-z",
            "--",
            ".",
        ],
        deadline,
    )?
    else {
        return Ok(BTreeMap::new());
    };
    if !tracked.status.success() {
        return Ok(BTreeMap::new());
    }
    let dirty = dirty_git_paths(scope_root, deadline)?;
    Ok(tracked
        .stdout
        .split(|byte| *byte == 0)
        .filter_map(|record| {
            let rendered = String::from_utf8_lossy(record);
            let (header, path) = rendered.split_once('\t')?;
            if dirty.contains(path) {
                return None;
            }
            let blob = header.split_whitespace().nth(1)?;
            Some((path.to_owned(), format!("git-blob:{blob}")))
        })
        .collect::<BTreeMap<String, String>>())
}

fn dirty_git_paths(
    scope_root: &Path,
    deadline: Option<Instant>,
) -> Result<BTreeSet<String>, ScanError> {
    let Some(status) = run_git_bounded(
        &[
            "-C",
            &scope_root.display().to_string(),
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ],
        deadline,
    )?
    else {
        return Ok(BTreeSet::new());
    };
    if !status.status.success() {
        return Ok(BTreeSet::new());
    }
    Ok(status
        .stdout
        .split(|byte| *byte == 0)
        .filter_map(|record| {
            let rendered = String::from_utf8_lossy(record);
            if rendered.len() >= 4 && rendered.as_bytes().get(2) == Some(&b' ') {
                Some(rendered[3..].to_owned())
            } else if !rendered.is_empty() {
                Some(rendered.into_owned())
            } else {
                None
            }
        })
        .collect())
}

/// Runs one owned `git` child under the shared doctor deadline.
///
/// `Ok(None)` preserves the historical graceful fallback when `git` is
/// missing or exits non-zero. A deadline expiry terminates and reaps the
/// owned process tree and returns a budget-exhausted [`ScanError`] that the
/// deep inventory maps to the `scan_inventory` phase without publishing a
/// partial cache generation.
fn run_git_bounded(
    args: &[&str],
    deadline: Option<Instant>,
) -> Result<Option<std::process::Output>, ScanError> {
    let Some(deadline) = deadline else {
        return Ok(doctor_git_command().args(args).output().ok());
    };
    if Instant::now() >= deadline {
        return Err(git_budget_error());
    }
    let mut command = doctor_git_command();
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(all(unix, test))]
    let evidence = DoctorGitProcessEvidence::from_test_config(Some(deadline));
    #[cfg(all(unix, test))]
    if evidence.is_some() {
        command
            .env(
                "EFFIGY_TEST_DOCTOR_GIT_CALLER",
                "effigy-scan::execution::doctor_inventory::run_git_bounded",
            )
            .env(
                "EFFIGY_TEST_DOCTOR_GIT_ARGS_JSON",
                serde_json::to_string(args).unwrap_or_else(|_| "[]".to_owned()),
            );
    }
    #[cfg(unix)]
    // SAFETY: `pre_exec` runs this closure in the forked child before `exec`,
    // where only async-signal-safe work is allowed. `setpgid` is
    // async-signal-safe and places the child in a new process group whose id
    // is the child's own pid; the error path uses the allocation-free
    // `io::Error::from` conversion. `terminate_git_tree` signals
    // `kill(-pid, ...)` for that same group when the doctor budget expires.
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))
                .map_err(std::io::Error::from)
        });
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            #[cfg(all(unix, test))]
            if let Some(evidence) = &evidence {
                evidence.record(
                    "spawn_failed",
                    serde_json::json!({
                        "kind": format!("{:?}", error.kind()),
                        "errno": error.raw_os_error(),
                    }),
                );
            }
            #[cfg(not(all(unix, test)))]
            let _ = error;
            return Ok(None);
        }
    };
    #[cfg(all(unix, test))]
    if let Some(evidence) = &evidence {
        let pid = child.id();
        let pgid = i32::try_from(pid)
            .ok()
            .and_then(|pid| nix::unistd::getpgid(Some(nix::unistd::Pid::from_raw(pid))).ok())
            .map(nix::unistd::Pid::as_raw);
        // `Command::spawn` returns a child only when the `pre_exec` closure
        // succeeds, so this records the actual `setpgid` result. The observed
        // PGID independently checks the resulting kernel state.
        evidence.record(
            "leader_spawned",
            serde_json::json!({
                "pid": pid,
                "ppid": std::process::id(),
                "pgid": pgid,
                "caller": "effigy-scan::execution::doctor_inventory::run_git_bounded",
                "args": args,
                "setpgid": {
                    "result": "ok",
                    "return": 0,
                    "errno": null,
                    "inferred_from_successful_spawn": true,
                },
            }),
        );
    }
    #[cfg(all(unix, test))]
    let mut child = OwnedGitChild::new(child, evidence);
    #[cfg(not(all(unix, test)))]
    let mut child = OwnedGitChild::new(child);
    let stdout = child
        .child_mut()
        .stdout
        .take()
        .expect("bounded git stdout is piped");
    let stderr = child
        .child_mut()
        .stderr
        .take()
        .expect("bounded git stderr is piped");
    let stdout_reader = std::thread::spawn(move || read_child_output(stdout));
    let stderr_reader = std::thread::spawn(move || read_child_output(stderr));

    loop {
        if Instant::now() >= deadline {
            child.terminate_and_reap();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(git_budget_error());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_reader.join().ok().and_then(Result::ok);
                let stderr = stderr_reader.join().ok().and_then(Result::ok);
                let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
                    return Ok(None);
                };
                return Ok(Some(std::process::Output {
                    status,
                    stdout,
                    stderr,
                }));
            }
            Ok(None) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                std::thread::sleep(std::time::Duration::from_millis(10).min(remaining));
            }
            Err(_) => {
                child.terminate_and_reap();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Ok(None);
            }
        }
    }
}

#[cfg(all(unix, test))]
#[derive(Clone)]
struct DoctorGitTestConfig {
    program: PathBuf,
    fixture_root: PathBuf,
    index_response: PathBuf,
    slow_index: bool,
    fixture_executable: PathBuf,
    source_events: PathBuf,
    fixture_events: PathBuf,
    shell_events: PathBuf,
    invocation: String,
    initial_ready: PathBuf,
    late_ready: PathBuf,
    initial_marker: PathBuf,
    late_marker: PathBuf,
}

#[cfg(all(unix, test))]
thread_local! {
    static DOCTOR_GIT_TEST_CONFIG: std::cell::RefCell<Option<DoctorGitTestConfig>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(all(unix, test))]
fn doctor_git_test_config() -> Option<DoctorGitTestConfig> {
    DOCTOR_GIT_TEST_CONFIG.with(|config| config.borrow().clone())
}

fn doctor_git_command() -> Command {
    #[cfg(all(unix, test))]
    if let Some(config) = doctor_git_test_config() {
        let mut command = Command::new(config.program);
        command
            .env("EFFIGY_TEST_DOCTOR_GIT_ROOT", config.fixture_root)
            .env("EFFIGY_TEST_DOCTOR_GIT_INDEX", config.index_response)
            .env(
                "EFFIGY_TEST_DOCTOR_GIT_SLOW_INDEX",
                if config.slow_index { "1" } else { "0" },
            )
            .env("EFFIGY_TEST_DOCTOR_FIXTURE_EXE", config.fixture_executable)
            .env("EFFIGY_TEST_DOCTOR_GIT_INVOCATION", config.invocation)
            .env(
                "EFFIGY_TEST_DOCTOR_GIT_FIXTURE_EVENTS",
                config.fixture_events,
            )
            .env("EFFIGY_TEST_DOCTOR_GIT_SHELL_EVENTS", config.shell_events)
            .env("EFFIGY_TEST_DOCTOR_GIT_INITIAL_READY", config.initial_ready)
            .env("EFFIGY_TEST_DOCTOR_GIT_LATE_READY", config.late_ready)
            .env(
                "EFFIGY_TEST_DOCTOR_GIT_INITIAL_MARKER",
                config.initial_marker,
            )
            .env("EFFIGY_TEST_DOCTOR_GIT_LATE_MARKER", config.late_marker);
        return command;
    }
    Command::new("git")
}

fn read_child_output(mut stream: impl std::io::Read) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    stream.read_to_end(&mut output)?;
    Ok(output)
}

/// Keeps the direct child unreaped while its process group is terminated.
/// The leader's unreaped PID pins the group identity through TERM/KILL
/// escalation, so a reused numeric PID or process-group ID cannot be signaled.
struct OwnedGitChild {
    child: std::process::Child,
    pid: u32,
    settled: bool,
    #[cfg(all(unix, test))]
    evidence: Option<DoctorGitProcessEvidence>,
}

impl OwnedGitChild {
    #[cfg(all(unix, test))]
    fn new(child: std::process::Child, evidence: Option<DoctorGitProcessEvidence>) -> Self {
        let pid = child.id();
        Self {
            child,
            pid,
            settled: false,
            evidence,
        }
    }

    #[cfg(not(all(unix, test)))]
    fn new(child: std::process::Child) -> Self {
        let pid = child.id();
        Self {
            child,
            pid,
            settled: false,
        }
    }

    fn child_mut(&mut self) -> &mut std::process::Child {
        &mut self.child
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        let result = self.child.try_wait()?;
        if result.is_some() {
            self.settled = true;
            #[cfg(all(unix, test))]
            if let Some(status) = result.as_ref() {
                if let Some(evidence) = &self.evidence {
                    evidence.record(
                        "leader_exited",
                        serde_json::json!({
                            "pid": self.pid,
                            "success": status.success(),
                            "code": status.code(),
                            "signal": std::os::unix::process::ExitStatusExt::signal(status),
                        }),
                    );
                }
            }
        }
        Ok(result)
    }

    fn terminate_and_reap(&mut self) {
        if self.settled {
            return;
        }
        // Keep the child unreaped until both process-group signals have been
        // sent. Once the numeric identity has been used, do not signal it
        // again even if `wait` reports an error.
        self.settled = true;
        #[cfg(all(unix, test))]
        terminate_git_tree(self.pid, self.evidence.as_ref());
        #[cfg(not(all(unix, test)))]
        terminate_git_tree(self.pid);
        #[cfg(not(unix))]
        let _ = self.child.kill();
        #[cfg(all(unix, test))]
        {
            let wait_result = self.child.wait();
            if let Some(evidence) = &self.evidence {
                evidence.record(
                    "leader_reaped",
                    serde_json::json!({
                        "pid": self.pid,
                        "status": wait_result.as_ref().ok().map(|status| {
                            serde_json::json!({
                                "success": status.success(),
                                "code": status.code(),
                                "signal": std::os::unix::process::ExitStatusExt::signal(status),
                            })
                        }),
                        "errno": wait_result.as_ref().err().and_then(std::io::Error::raw_os_error),
                    }),
                );
            }
        }
        #[cfg(not(all(unix, test)))]
        {
            let _ = self.child.wait();
        }
    }
}

impl Drop for OwnedGitChild {
    fn drop(&mut self) {
        self.terminate_and_reap();
    }
}

fn git_budget_error() -> ScanError {
    ScanError::invocation("doctor scan budget exhausted during git identity probe")
}

#[cfg(unix)]
fn terminate_git_tree(pid: u32, #[cfg(test)] evidence: Option<&DoctorGitProcessEvidence>) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    if pid <= 0 {
        return;
    }
    // `OwnedGitChild` keeps the group leader unreaped across this escalation,
    // preventing the process-group ID from being reused between signals.
    let target = Pid::from_raw(-pid);
    let term = kill(target, Signal::SIGTERM);
    #[cfg(test)]
    if let Some(evidence) = evidence {
        evidence.record_signal("TERM", target.as_raw(), &term);
    }
    #[cfg(not(test))]
    let _ = term;
    std::thread::sleep(std::time::Duration::from_millis(500));
    let kill_result = kill(target, Signal::SIGKILL);
    #[cfg(test)]
    if let Some(evidence) = evidence {
        evidence.record_signal("KILL", target.as_raw(), &kill_result);
    }
    #[cfg(not(test))]
    let _ = kill_result;
}

#[cfg(not(unix))]
fn terminate_git_tree(_pid: u32) {}

#[cfg(all(unix, test))]
/// Private debug-build recording enabled only by the doctor process fixture.
/// With no test evidence path set, it has no filesystem or process effects.
struct DoctorGitProcessEvidence {
    path: PathBuf,
    invocation: String,
    ready_marker: Option<PathBuf>,
    late_ready_marker: Option<PathBuf>,
    deadline: Option<Instant>,
    started: Instant,
}

#[cfg(all(unix, test))]
impl DoctorGitProcessEvidence {
    fn from_test_config(deadline: Option<Instant>) -> Option<Self> {
        let config = doctor_git_test_config()?;
        Some(Self {
            path: config.source_events,
            invocation: config.invocation,
            ready_marker: Some(config.initial_ready),
            late_ready_marker: Some(config.late_ready),
            deadline,
            started: Instant::now(),
        })
    }

    fn record(&self, event: &str, details: serde_json::Value) {
        let record = serde_json::json!({
            "invocation": self.invocation,
            "elapsed_ns": self.started.elapsed().as_nanos(),
            "deadline_remaining_ns": self.deadline.map(|deadline| {
                deadline.saturating_duration_since(Instant::now()).as_nanos()
            }),
            "event": event,
            "details": details,
        });
        let Ok(line) = serde_json::to_vec(&record) else {
            return;
        };
        let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        let _ = file.write_all(&line);
        let _ = file.write_all(b"\n");
    }

    fn record_signal(&self, signal: &str, target: i32, result: &Result<(), nix::errno::Errno>) {
        self.record(
            "signal",
            serde_json::json!({
                "signal": signal,
                "target": target,
                "result": if result.is_ok() { "ok" } else { "error" },
                "errno": result.as_ref().err().map(|error| *error as i32),
                "ready_at_signal": self.ready_marker.as_ref().is_some_and(|path| path.exists()),
                "late_ready_at_signal": self.late_ready_marker.as_ref().is_some_and(|path| path.exists()),
            }),
        );
    }
}

fn validate_options(options: &DoctorScanInventoryOptions) -> Result<(), ScanError> {
    if options.god_files.doctor_enabled {
        options.god_files.validate()?;
    }
    if options.duplicate_blocks.doctor_enabled {
        options.duplicate_blocks.validate()?;
    }
    Ok(())
}

fn enabled_gitignore_postures(
    options: &DoctorScanInventoryOptions,
) -> impl Iterator<Item = bool> + '_ {
    [
        (
            options.god_files.doctor_enabled,
            options.god_files.respect_gitignore,
        ),
        (
            options.duplicate_blocks.doctor_enabled,
            options.duplicate_blocks.respect_gitignore,
        ),
    ]
    .into_iter()
    .filter_map(|(enabled, posture)| enabled.then_some(posture))
}

fn ensure_budget(deadline: Option<Instant>) -> Result<(), ScanError> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(ScanError::invocation(
            "doctor scan budget exhausted during inventory",
        ));
    }
    Ok(())
}

fn evaluate_god_files(
    root: &Path,
    observations: &[Observation],
    options: &GodFileScanOptions,
    deadline: Option<Instant>,
) -> Result<GodFileScanResult, ScanError> {
    let mut findings = Vec::new();
    let mut scanned_files = 0;
    let mut skipped_generated = 0;
    for observation in observations {
        ensure_budget(deadline)?;
        if !observation.god_file_eligible {
            continue;
        }
        if observation.generated {
            skipped_generated += 1;
            continue;
        }
        scanned_files += 1;
        let code_lines = observation.code_lines;
        let path = Path::new(&observation.rel_str);
        let thresholds = options.thresholds_for(path);
        if let Some(severity) = classify_severity(code_lines, &thresholds) {
            findings.push(GodFileFinding {
                path: observation.rel_str.clone(),
                code_lines,
                total_lines: observation.total_lines,
                severity,
                language: god_file_language(path),
            });
        }
    }
    findings.sort_by(|left, right| {
        severity_rank(right.severity)
            .cmp(&severity_rank(left.severity))
            .then_with(|| right.code_lines.cmp(&left.code_lines))
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(GodFileScanResult {
        root: root.display().to_string(),
        scanned_files,
        skipped_generated,
        findings,
        thresholds: options.thresholds,
        thresholds_source: options.thresholds_source(),
    })
}

fn evaluate_duplicate_blocks(
    root: &Path,
    observations: &[Observation],
    options: &DuplicateBlockScanOptions,
    deadline: Option<Instant>,
) -> Result<DuplicateBlockScanResult, ScanError> {
    let mut scanned_files = 0;
    let mut files = Vec::new();
    for observation in observations {
        ensure_budget(deadline)?;
        if !observation.duplicate_eligible || observation.generated {
            continue;
        }
        scanned_files += 1;
        let lines = observation
            .normalized_lines
            .iter()
            .map(|line| NormalizedCodeLine {
                line_number: line.line_number,
                text: line.text.clone(),
            })
            .collect::<Vec<_>>();
        if lines.len() >= options.thresholds.warn {
            files.push(DuplicateBlockFile {
                path: observation.rel_str.clone(),
                lines,
            });
        }
    }
    let candidate_blocks = candidate_block_count(&files, options.thresholds.warn);
    let mut findings = detect_duplicate_blocks_bounded(&files, options, deadline)?;
    findings.sort_by(|left, right| {
        duplicate_block_severity_rank(right.severity)
            .cmp(&duplicate_block_severity_rank(left.severity))
            .then_with(|| right.block_lines.cmp(&left.block_lines))
            .then_with(|| right.occurrences.cmp(&left.occurrences))
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
    Ok(DuplicateBlockScanResult {
        root: root.display().to_string(),
        scanned_files,
        candidate_blocks,
        findings,
        thresholds: options.thresholds.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{run_duplicate_block_scan_workspace, run_god_file_scan_workspace};

    fn options() -> DoctorScanInventoryOptions {
        DoctorScanInventoryOptions {
            god_files: GodFileScanOptions::default(),
            duplicate_blocks: DuplicateBlockScanOptions {
                doctor_enabled: true,
                ..DuplicateBlockScanOptions::default()
            },
        }
    }

    fn fixture_generation(paths: &CachePaths) -> CacheGeneration {
        CacheGeneration {
            schema: "effigy.doctor.cache.v2".into(),
            owner: paths.owner.clone(),
            config_identity: "fixture".into(),
            files: BTreeMap::new(),
        }
    }

    fn fixture_cache(root: &Path, alias: &str) -> CachePaths {
        let paths = cache_paths(root, alias, &root.join(alias));
        std::fs::create_dir_all(&paths.directory).unwrap();
        std::fs::write(
            &paths.current,
            serde_json::to_vec(&fixture_generation(&paths)).unwrap(),
        )
        .unwrap();
        paths
    }

    #[test]
    fn eviction_checks_ownership_and_respects_locks_and_unknown_entries() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let old = fixture_cache(root, "old");
        let locked = fixture_cache(root, "locked");
        let current = fixture_cache(root, "current");
        let mismatch = fixture_cache(root, "mismatch");
        let mut generation = fixture_generation(&mismatch);
        generation.owner.alias = "someone-else".into();
        std::fs::write(&mismatch.current, serde_json::to_vec(&generation).unwrap()).unwrap();
        let unknown = fixture_cache(root, "unknown");
        std::fs::write(unknown.directory.join("user-data"), b"keep").unwrap();
        let legacy = root.join(".effigy/doctor/cache/v1/old");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("generation.json"), b"v1").unwrap();
        std::fs::write(&locked.lock, b"busy").unwrap();
        let result = evict_cache(&current, 0, 0);
        assert!(result.is_err());
        assert!(!old.current.exists());
        for path in [
            &locked.current,
            &current.current,
            &mismatch.current,
            &unknown.current,
            &legacy.join("generation.json"),
        ] {
            assert!(path.exists(), "{}", path.display());
        }
    }

    #[test]
    fn eviction_uses_mtime_then_directory_name_and_byte_bound() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let a = fixture_cache(root, "a");
        let b = fixture_cache(root, "b");
        let current = fixture_cache(root, "current");
        let stamp = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        for path in [&a.current, &b.current] {
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(stamp))
                .unwrap();
        }
        evict_cache(&current, u64::MAX, 2).unwrap();
        assert!(!a.current.exists());
        assert!(b.current.exists());
        let current_size = std::fs::metadata(&current.current).unwrap().len();
        evict_cache(&current, current_size, usize::MAX).unwrap();
        assert!(!b.current.exists());
        assert!(current.current.exists());
    }

    #[cfg(unix)]
    #[test]
    fn eviction_retains_symlinks() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let current = fixture_cache(temp.path(), "current");
        let target = fixture_cache(temp.path(), "target");
        let link = current.directory.parent().unwrap().join("unowned-link");
        symlink(&target.directory, &link).unwrap();
        assert!(evict_cache(&current, u64::MAX, usize::MAX).is_err());
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(target.current.exists());
    }

    #[test]
    fn changed_publication_evicts_without_changing_findings() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
        let options = options();
        let cold =
            run_doctor_scan_inventory(root, "root", root, &[], &options, false, None).unwrap();
        for i in 0..32 {
            fixture_cache(root, &format!("other-{i:02}"));
        }
        std::fs::write(root.join("main.rs"), "fn main() { println!(\"hi\"); }\n").unwrap();
        let changed =
            run_doctor_scan_inventory(root, "root", root, &[], &options, false, None).unwrap();
        assert!(changed.warnings.is_empty(), "{:?}", changed.warnings);
        let root_paths = cache_paths(root, "root", root);
        assert_eq!(
            std::fs::read_dir(root_paths.directory.parent().unwrap())
                .unwrap()
                .count(),
            32
        );
        let warm =
            run_doctor_scan_inventory(root, "root", root, &[], &options, false, None).unwrap();
        assert_eq!(changed.god_files, warm.god_files);
        assert_eq!(changed.duplicate_blocks, warm.duplicate_blocks);
        assert_eq!(cold.physical_walks, warm.physical_walks);
    }

    #[test]
    fn oversized_generation_is_not_published() {
        let temp = tempfile::tempdir().unwrap();
        let paths = cache_paths(temp.path(), "root", temp.path());
        let result = publish_cache_with_limit(&paths, &fixture_generation(&paths), 1);
        assert!(result.unwrap_err().contains("exceeds"));
        assert!(!paths.directory.exists());
    }

    #[test]
    fn shared_inventory_matches_enabled_standalone_scans_with_one_walk() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::write(
            root.join("src/lib.rs"),
            "// TODO: split this module\nfn alpha() { println!(\"alpha\"); }\n",
        )
        .expect("source");
        std::fs::write(
            root.join("src/generated.rs"),
            "// generated file\nfn x() {}\n",
        )
        .expect("generated");
        let options = options();

        let shared = run_doctor_scan_inventory(root, "root", root, &[], &options, true, None)
            .expect("shared inventory");

        assert_eq!(shared.physical_walks, 1);
        assert_eq!(
            shared.god_files,
            Some(
                run_god_file_scan_workspace(root, &[root.to_path_buf()], &options.god_files)
                    .expect("god files")
            )
        );
        assert_eq!(
            shared.duplicate_blocks,
            Some(
                run_duplicate_block_scan_workspace(
                    root,
                    &[root.to_path_buf()],
                    &options.duplicate_blocks,
                )
                .expect("duplicate blocks")
            )
        );
    }

    #[test]
    fn cache_reuses_exact_facts_and_same_size_edits_miss() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        let source = root.join("src/lib.rs");
        std::fs::write(&source, "fn alpha() {}\n").expect("source");
        let options = options();

        let cold = run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("cold inventory");
        assert_eq!(cold.cache_hits, 0);
        assert_eq!(cold.cache_misses, 1);

        let warm = run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("warm inventory");
        assert_eq!(warm.cache_hits, 1);
        assert_eq!(warm.cache_misses, 0);

        let refreshed = run_doctor_scan_inventory(root, "root", root, &[], &options, true, None)
            .expect("refreshed inventory");
        assert_eq!(refreshed.cache_hits, 0);
        assert_eq!(refreshed.cache_misses, 1);

        std::fs::write(&source, "fn bravo() {}\n").expect("same-size edit");
        let changed = run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("changed inventory");
        assert_eq!(changed.cache_hits, 0);
        assert_eq!(changed.cache_misses, 1);
    }

    #[test]
    fn unchanged_generation_needs_no_publication_lock_or_replacement() {
        use std::os::unix::fs::MetadataExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let source = root.join("main.rs");
        std::fs::write(&source, "fn alpha() {}\n").unwrap();
        let options = options();
        run_doctor_scan_inventory(root, "root", root, &[], &options, false, None).unwrap();
        let paths = cache_paths(root, "root", root);
        let before = std::fs::metadata(&paths.current).unwrap();
        let directory = std::fs::metadata(&paths.directory).unwrap();
        for refresh in [false, true] {
            let result =
                run_doctor_scan_inventory(root, "root", root, &[], &options, refresh, None)
                    .unwrap();
            assert!(result.warnings.is_empty(), "{:?}", result.warnings);
            let after = std::fs::metadata(&paths.current).unwrap();
            assert_eq!(
                (before.ino(), before.mtime(), before.mtime_nsec()),
                (after.ino(), after.mtime(), after.mtime_nsec())
            );
            let after_dir = std::fs::metadata(&paths.directory).unwrap();
            assert_eq!(
                (directory.mtime(), directory.mtime_nsec()),
                (after_dir.mtime(), after_dir.mtime_nsec()),
                "even transient files mutate the directory"
            );
        }
        // An unchanged generation also succeeds while another writer owns the
        // lock; changed content still follows the locked, atomic path.
        std::fs::write(&paths.lock, "other publisher").unwrap();
        let warm =
            run_doctor_scan_inventory(root, "root", root, &[], &options, false, None).unwrap();
        assert!(warm.warnings.is_empty());
        std::fs::write(&source, "fn bravo() {}\n").unwrap();
        let blocked =
            run_doctor_scan_inventory(root, "root", root, &[], &options, false, None).unwrap();
        assert!(blocked
            .warnings
            .iter()
            .any(|w| w.contains("lock unavailable")));
        assert_eq!(
            std::fs::metadata(&paths.current).unwrap().ino(),
            before.ino()
        );
        std::fs::remove_file(&paths.lock).unwrap();
        let changed =
            run_doctor_scan_inventory(root, "root", root, &[], &options, false, None).unwrap();
        assert!(changed.warnings.is_empty());
        assert_ne!(
            std::fs::metadata(&paths.current).unwrap().ino(),
            before.ino()
        );
    }

    #[test]
    fn corrupt_cache_warns_and_rebuilds_without_panicking() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("source");
        let options = options();
        run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("initial inventory");
        let paths = cache_paths(root, "root", root);
        std::fs::write(&paths.current, b"not-json").expect("corrupt cache");

        let rebuilt = run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("rebuilt inventory");
        assert_eq!(rebuilt.invalid_cache_entries, 1);
        assert_eq!(rebuilt.cache_misses, 1);
        assert!(rebuilt.warnings[0].contains("corrupt"));
    }

    #[test]
    fn incompatible_cache_version_warns_and_rebuilds() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("source");
        let options = options();
        run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("initial inventory");
        let paths = cache_paths(root, "root", root);
        let mut generation = serde_json::from_slice::<CacheGeneration>(
            &std::fs::read(&paths.current).expect("read cache"),
        )
        .expect("decode cache");
        generation.schema = "effigy.doctor.cache.v0".to_owned();
        std::fs::write(
            &paths.current,
            serde_json::to_vec(&generation).expect("encode incompatible cache"),
        )
        .expect("write incompatible cache");

        let rebuilt = run_doctor_scan_inventory(root, "root", root, &[], &options, false, None)
            .expect("rebuilt inventory");
        assert_eq!(rebuilt.invalid_cache_entries, 1);
        assert_eq!(rebuilt.cache_misses, 1);
        assert!(rebuilt.warnings[0].contains("incompatible"));
    }

    #[test]
    fn pruned_sibling_is_not_observed_or_cached() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let sibling = root.join("apps/sibling");
        std::fs::create_dir_all(&sibling).expect("sibling");
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("root source");
        std::fs::write(sibling.join("secret.rs"), "fn sibling() {}\n").expect("sibling source");
        let options = options();

        let result = run_doctor_scan_inventory(
            root,
            "root",
            root,
            std::slice::from_ref(&sibling),
            &options,
            true,
            None,
        )
        .expect("root inventory");
        assert_eq!(result.god_files.expect("god files").scanned_files, 1);
    }

    #[test]
    fn expired_deadline_does_not_publish_partial_cache() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("source");
        let options = options();
        let error = run_doctor_scan_inventory(
            root,
            "root",
            root,
            &[],
            &options,
            false,
            Some(Instant::now()),
        )
        .expect_err("deadline should fail");
        assert!(error.to_string().contains("budget exhausted"));
        assert!(!cache_paths(root, "root", root).current.exists());
    }

    #[cfg(all(unix, test))]
    #[test]
    fn doctor_git_timeout_process_group_evidence() {
        doctor_git_inventory_fixture_proof(false);
    }

    #[cfg(all(unix, test))]
    #[test]
    fn doctor_git_timeout_before_status_is_incomplete_evidence() {
        doctor_git_inventory_fixture_proof(true);
    }

    #[cfg(all(unix, test))]
    fn doctor_git_inventory_fixture_proof(slow_index: bool) {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).expect("source directory");
        std::fs::write(root.join("src/lib.rs"), "pub fn cold() {}\n").expect("source");
        let real_git = path_binary("git");
        run_git(&real_git, root, &["init", "--quiet"]);
        run_git(&real_git, root, &["add", "--", "."]);

        // Materialize the real index as fixture data before the measured
        // inventory call. This does not run doctor or populate a scan cache.
        // The bounded production ls-files capture still executes and parses
        // this response; only its unrelated real-Git startup is isolated.
        let prepared = run_git_bounded(
            &[
                "-C",
                &root.display().to_string(),
                "ls-files",
                "--stage",
                "-z",
                "--",
                ".",
            ],
            Some(Instant::now() + std::time::Duration::from_secs(10)),
        )
        .expect("prepare bounded private Git index")
        .expect("Git index response");
        assert!(prepared.status.success());
        assert!(prepared.stdout.ends_with(b"\tsrc/lib.rs\0"));
        assert!(
            prepared.stdout.len() < 4096,
            "unexpected fixture index size"
        );
        let index_response = root.join("prepared-index");
        std::fs::write(&index_response, prepared.stdout).expect("write private index response");

        let shim = root.join("git-timeout-shim");
        write_doctor_git_timeout_shim(&shim);
        let child_events = root.join("fixture-processes.jsonl");
        let source_events = root.join("doctor-processes.jsonl");
        let shell_events = root.join("shell-events.txt");
        let initial_ready = root.join("initial-ready");
        let late_ready = root.join("late-ready");
        let initial_marker = root.join("initial-survived");
        let late_marker = root.join("late-survived");
        let fixture_guard = DoctorGitFixtureGuard::new(
            child_events.clone(),
            source_events.clone(),
            shell_events.clone(),
        );
        let mut sibling = DoctorProcessSibling::spawn();
        let _config = DoctorGitTestConfigGuard::install(DoctorGitTestConfig {
            program: shim,
            fixture_root: root.to_path_buf(),
            index_response,
            slow_index,
            fixture_executable: std::env::current_exe().expect("test executable"),
            source_events: source_events.clone(),
            fixture_events: child_events.clone(),
            shell_events: shell_events.clone(),
            invocation: "direct-inventory".to_owned(),
            initial_ready: initial_ready.clone(),
            late_ready: late_ready.clone(),
            initial_marker: initial_marker.clone(),
            late_marker: late_marker.clone(),
        });

        let started = Instant::now();
        let deadline = started + std::time::Duration::from_millis(800);
        let error =
            run_doctor_scan_inventory(root, "root", root, &[], &options(), false, Some(deadline))
                .expect_err("the bounded Git status probe should exhaust its call budget");
        assert!(error.to_string().contains("budget exhausted"), "{error}");
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
        assert!(
            !cache_paths(root, "root", root).directory.exists(),
            "a timed-out scan published partial cache state"
        );

        if slow_index {
            let source =
                std::fs::read_to_string(&source_events).expect("slow-prelude source receipt");
            let events = source
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("source event"))
                .collect::<Vec<_>>();
            let leaders = events
                .iter()
                .filter(|event| event["event"] == "leader_spawned")
                .collect::<Vec<_>>();
            assert_eq!(
                leaders.len(),
                1,
                "porcelain must not spawn after prelude timeout"
            );
            let leader = leaders[0]["details"]["pid"]
                .as_u64()
                .expect("owned prelude PID");
            assert_eq!(leaders[0]["details"]["args"][2], "ls-files");
            let term = events
                .iter()
                .find(|event| event["event"] == "signal" && event["details"]["signal"] == "TERM")
                .expect("real prelude timeout TERM");
            assert_eq!(
                term["details"]["target"].as_i64(),
                Some(-i64::try_from(leader).unwrap())
            );
            assert_eq!(term["deadline_remaining_ns"], 0);
            assert_eq!(term["details"]["ready_at_signal"], false);
            assert!(events.iter().any(|event| event["event"] == "leader_reaped"
                && event["details"]["pid"].as_u64() == Some(leader)));
            assert!(!child_events.exists() && !initial_ready.exists() && !late_ready.exists());
            assert!(!initial_marker.exists() && !late_marker.exists());
            let shell =
                std::fs::read_to_string(&shell_events).expect("slow-prelude fixture receipt");
            assert!(shell.contains("event=index_readiness_deliberately_held "));
            assert!(!shell.contains("event=shim_entered "));
            sibling.assert_survived();
            assert_ne!(
                sibling.pgid,
                i32::try_from(leader).expect("prelude PID fits process API")
            );
            // Incomplete pre-target evidence is a negative control, not the
            // ready initial/late-descendant termination proof below.
            drop(fixture_guard);
            return;
        }

        let target = assert_doctor_git_process_evidence(
            &source_events,
            &child_events,
            &shell_events,
            "direct-inventory",
        );
        sibling.assert_survived();
        assert_ne!(
            sibling.pgid,
            i32::try_from(target).expect("target fits i32")
        );

        let remaining = std::time::Duration::from_millis(4600).saturating_sub(started.elapsed());
        std::thread::sleep(remaining);
        assert!(
            !initial_marker.exists(),
            "initial marker writer survived cleanup"
        );
        assert!(!late_marker.exists(), "late marker writer survived cleanup");
        assert_doctor_fixture_processes_reaped(&child_events, "direct-inventory");
        drop(fixture_guard);
    }

    #[cfg(all(unix, test))]
    #[test]
    fn doctor_git_timeout_process_fixture_child() {
        use nix::sys::signal::Signal;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let Ok(role) = std::env::var("EFFIGY_TEST_DOCTOR_FIXTURE_ROLE") else {
            return;
        };
        assert!(role == "initial" || role == "late", "unknown role {role}");
        let events = PathBuf::from(
            std::env::var_os("EFFIGY_TEST_DOCTOR_GIT_FIXTURE_EVENTS").expect("fixture event path"),
        );
        let invocation =
            std::env::var("EFFIGY_TEST_DOCTOR_GIT_INVOCATION").expect("fixture invocation");
        let started = Instant::now();
        record_doctor_fixture_event(
            &events,
            started,
            serde_json::json!({
                "invocation": invocation,
                "event": "child_entered_fixture",
                "role": role,
                "pid": std::process::id(),
                "ppid": nix::unistd::getppid().as_raw(),
            }),
        );
        let (ready, marker) = if role == "initial" {
            (
                PathBuf::from(
                    std::env::var_os("EFFIGY_TEST_DOCTOR_GIT_INITIAL_READY")
                        .expect("initial readiness path"),
                ),
                PathBuf::from(
                    std::env::var_os("EFFIGY_TEST_DOCTOR_GIT_INITIAL_MARKER")
                        .expect("initial marker path"),
                ),
            )
        } else {
            (
                PathBuf::from(
                    std::env::var_os("EFFIGY_TEST_DOCTOR_GIT_LATE_READY")
                        .expect("late readiness path"),
                ),
                PathBuf::from(
                    std::env::var_os("EFFIGY_TEST_DOCTOR_GIT_LATE_MARKER")
                        .expect("late marker path"),
                ),
            )
        };
        let terminated = Arc::new(AtomicBool::new(false));
        if let Err(error) =
            signal_hook::flag::register(Signal::SIGTERM as i32, Arc::clone(&terminated))
        {
            record_doctor_fixture_event(
                &events,
                started,
                serde_json::json!({
                    "invocation": invocation,
                    "event": "startup_error",
                    "phase": "register_term_handler",
                    "role": role,
                    "error": error.to_string(),
                }),
            );
            panic!("register fixture TERM handler: {error}");
        }
        record_doctor_fixture_process(&events, &invocation, &role, started);
        record_doctor_fixture_event(
            &events,
            started,
            serde_json::json!({
                "invocation": invocation,
                "event": "readiness_publish_started",
                "role": role,
                "pid": std::process::id(),
                "path": ready,
            }),
        );
        if let Err(error) = std::fs::write(&ready, b"ready\n") {
            record_doctor_fixture_event(
                &events,
                started,
                serde_json::json!({
                    "invocation": invocation,
                    "event": "startup_error",
                    "phase": "publish_readiness",
                    "role": role,
                    "error": error.to_string(),
                }),
            );
            panic!("publish fixture readiness: {error}");
        }
        record_doctor_fixture_event(
            &events,
            started,
            serde_json::json!({
                "invocation": invocation,
                "event": "process_ready",
                "role": role,
                "pid": std::process::id(),
                "path": ready,
            }),
        );

        let until = Instant::now() + std::time::Duration::from_secs(3);
        while Instant::now() < until {
            if terminated.swap(false, Ordering::AcqRel) {
                record_doctor_fixture_event(
                    &events,
                    started,
                    serde_json::json!({
                        "invocation": invocation,
                        "event": "descendant_received_term",
                        "role": role,
                        "pid": std::process::id(),
                    }),
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if let Err(error) =
            std::fs::write(&marker, format!("pid={} role={role}\n", std::process::id()))
        {
            record_doctor_fixture_event(
                &events,
                started,
                serde_json::json!({
                    "invocation": invocation,
                    "event": "fixture_error",
                    "phase": "publish_survival_marker",
                    "role": role,
                    "pid": std::process::id(),
                    "error": error.to_string(),
                }),
            );
            panic!("write surviving marker: {error}");
        }
        record_doctor_fixture_event(
            &events,
            started,
            serde_json::json!({
                "invocation": invocation,
                "event": "survival_marker_published",
                "role": role,
                "pid": std::process::id(),
                "path": marker,
            }),
        );
    }

    #[cfg(all(unix, test))]
    fn path_binary(name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        for directory in std::env::split_paths(&std::env::var_os("PATH").expect("PATH")) {
            let candidate = directory.join(name);
            let Ok(metadata) = std::fs::metadata(&candidate) else {
                continue;
            };
            if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
                return candidate;
            }
        }
        panic!("could not find executable {name} on PATH");
    }

    #[cfg(all(unix, test))]
    fn run_git(program: &Path, root: &Path, args: &[&str]) {
        let status = Command::new(program)
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .expect("run real Git fixture setup");
        assert!(status.success(), "Git fixture setup failed: {args:?}");
    }

    #[cfg(all(unix, test))]
    fn write_doctor_git_timeout_shim(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let script = r##"#!/bin/sh
shell_start_seconds=${SECONDS-}
record_phase() {
  shell_elapsed_seconds=unavailable
  if [ -n "$shell_start_seconds" ] && [ -n "${SECONDS-}" ]; then
    shell_elapsed_seconds=$((SECONDS - shell_start_seconds))
  fi
  printf 'event=%s pid=%s ppid=%s child_pid=%s shell_elapsed_seconds=%s\n' "$1" "$$" "${PPID:-unknown}" "${2:-none}" "$shell_elapsed_seconds" >> "$EFFIGY_TEST_DOCTOR_GIT_SHELL_EVENTS"
}
[ "$1" = "-C" ] && [ "$2" = "$EFFIGY_TEST_DOCTOR_GIT_ROOT" ] || exit 2
if [ "$3" = "ls-files" ]; then
  [ "$#" -eq 7 ] && [ "$4" = "--stage" ] && [ "$5" = "-z" ] && [ "$6" = "--" ] && [ "$7" = "." ] || exit 2
  if [ "$EFFIGY_TEST_DOCTOR_GIT_SLOW_INDEX" = "1" ]; then
    record_phase index_readiness_deliberately_held
    sleep 2
  else
    record_phase prepared_index_reply
  fi
  exec /bin/cat "$EFFIGY_TEST_DOCTOR_GIT_INDEX"
fi
[ "$3" = "status" ] && [ "$#" -eq 8 ] && [ "$4" = "--porcelain=v1" ] && [ "$5" = "-z" ] && [ "$6" = "--untracked-files=all" ] && [ "$7" = "--" ] && [ "$8" = "." ] || exit 2
case " $* " in
  *" status --porcelain=v1 "*)
    record_phase shim_entered
    trap 'record_phase term_handler_entered; EFFIGY_TEST_DOCTOR_FIXTURE_ROLE=late "$EFFIGY_TEST_DOCTOR_FIXTURE_EXE" doctor_git_timeout_process_fixture_child --nocapture >/dev/null 2>>"$EFFIGY_TEST_DOCTOR_GIT_SHELL_EVENTS" & late_pid=$!; record_phase late_child_spawn_requested "$late_pid"; while [ ! -f "$EFFIGY_TEST_DOCTOR_GIT_LATE_READY" ]; do /bin/sleep 0.05; done; record_phase late_child_ready_observed "$late_pid"; record_phase term_handler_exit "$late_pid"; exit 0' TERM
    EFFIGY_TEST_DOCTOR_FIXTURE_ROLE=initial "$EFFIGY_TEST_DOCTOR_FIXTURE_EXE" doctor_git_timeout_process_fixture_child --nocapture >/dev/null 2>>"$EFFIGY_TEST_DOCTOR_GIT_SHELL_EVENTS" &
    initial_pid=$!
    record_phase initial_child_spawn_requested "$initial_pid"
    record_phase initial_readiness_wait_started "$initial_pid"
    while [ ! -f "$EFFIGY_TEST_DOCTOR_GIT_INITIAL_READY" ]; do /bin/sleep 0.05; done
    record_phase initial_child_ready_observed "$initial_pid"
    record_phase busy_wait_started "$initial_pid"
    while :; do /bin/sleep 0.05; done
    ;;
esac
exit 2
"##;
        std::fs::write(path, script).expect("write process-group Git shim");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("make Git shim executable");
    }

    #[cfg(all(unix, test))]
    fn record_doctor_fixture_process(
        events: &Path,
        invocation: &str,
        role: &str,
        started: Instant,
    ) {
        let pid = std::process::id();
        let pid_raw = i32::try_from(pid).expect("fixture PID fits process API");
        let pgid = match nix::unistd::getpgid(Some(nix::unistd::Pid::from_raw(pid_raw))) {
            Ok(pgid) => pgid.as_raw(),
            Err(error) => {
                record_doctor_fixture_event(
                    events,
                    started,
                    serde_json::json!({
                        "invocation": invocation,
                        "event": "startup_error",
                        "phase": "read_process_group",
                        "role": role,
                        "pid": pid,
                        "error": error.to_string(),
                    }),
                );
                panic!("read fixture process group for PID {pid}: {error}");
            }
        };
        let Some(start_identity) = effigy_process::process_start_identity(pid) else {
            record_doctor_fixture_event(
                events,
                started,
                serde_json::json!({
                    "invocation": invocation,
                    "event": "startup_error",
                    "phase": "read_process_generation",
                    "role": role,
                    "pid": pid,
                    "pgid": pgid,
                    "error": "process start identity unavailable",
                }),
            );
            panic!("read fixture process generation for PID {pid}");
        };
        let ppid = nix::unistd::getppid().as_raw();
        let parent_start_identity = u32::try_from(ppid)
            .ok()
            .and_then(effigy_process::process_start_identity);
        let parent_generation_missing = parent_start_identity.is_none();
        record_doctor_fixture_event(
            events,
            started,
            serde_json::json!({
                "invocation": invocation,
                "event": "process_started",
                "role": role,
                "pid": pid,
                "ppid": ppid,
                "pgid": pgid,
                "start_identity": start_identity,
                "parent_start_identity": parent_start_identity,
            }),
        );
        if parent_generation_missing {
            record_doctor_fixture_event(
                events,
                started,
                serde_json::json!({
                    "invocation": invocation,
                    "event": "startup_error",
                    "phase": "read_parent_process_generation",
                    "role": role,
                    "pid": pid,
                    "ppid": ppid,
                    "error": "parent process start identity unavailable",
                }),
            );
        }
    }

    #[cfg(all(unix, test))]
    fn record_doctor_fixture_event(events: &Path, started: Instant, mut event: serde_json::Value) {
        use std::io::Write;

        event
            .as_object_mut()
            .expect("fixture evidence is an object")
            .insert(
                "elapsed_ns".to_owned(),
                serde_json::json!(started.elapsed().as_nanos()),
            );
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(events)
            .expect("open fixture evidence");
        let mut line = serde_json::to_vec(&event).expect("serialize fixture evidence");
        line.push(b'\n');
        file.write_all(&line).expect("write fixture evidence");
    }

    #[cfg(all(unix, test))]
    struct DoctorGitTestConfigGuard;

    #[cfg(all(unix, test))]
    impl DoctorGitTestConfigGuard {
        fn install(config: DoctorGitTestConfig) -> Self {
            DOCTOR_GIT_TEST_CONFIG.with(|slot| {
                assert!(slot.borrow().is_none(), "nested doctor Git test config");
                *slot.borrow_mut() = Some(config);
            });
            Self
        }
    }

    #[cfg(all(unix, test))]
    impl Drop for DoctorGitTestConfigGuard {
        fn drop(&mut self) {
            DOCTOR_GIT_TEST_CONFIG.with(|slot| {
                slot.borrow_mut().take();
            });
        }
    }

    #[cfg(all(unix, test))]
    struct DoctorGitFixtureGuard {
        events: PathBuf,
        receipts: Vec<(&'static str, PathBuf)>,
    }

    #[cfg(all(unix, test))]
    impl DoctorGitFixtureGuard {
        fn new(events: PathBuf, source_events: PathBuf, shell_events: PathBuf) -> Self {
            Self {
                events: events.clone(),
                receipts: vec![
                    ("source", source_events),
                    ("fixture", events.clone()),
                    ("shim", shell_events),
                ],
            }
        }

        fn print_evidence_before_cleanup(&self) {
            for (phase, path) in &self.receipts {
                match std::fs::read_to_string(path) {
                    Ok(contents) => eprintln!(
                        "doctor Git process proof {phase} receipt ({}):\n{contents}",
                        path.display()
                    ),
                    Err(error) => eprintln!(
                        "doctor Git process proof {phase} receipt unavailable ({}): {error}",
                        path.display()
                    ),
                }
            }
        }

        fn reap_owned_processes(&self) {
            let Ok(contents) = std::fs::read_to_string(&self.events) else {
                return;
            };
            let mut owned = BTreeMap::new();
            for line in contents.lines() {
                let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                let (Some(pid), Some(identity)) = (
                    event["pid"]
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok()),
                    event["start_identity"].as_str(),
                ) else {
                    continue;
                };
                owned.insert(pid, identity.to_owned());
                if event["event"] == "process_started" && event["role"] == "initial" {
                    if let (Some(parent_pid), Some(parent_identity)) = (
                        event["ppid"]
                            .as_u64()
                            .and_then(|value| u32::try_from(value).ok()),
                        event["parent_start_identity"].as_str(),
                    ) {
                        owned.insert(parent_pid, parent_identity.to_owned());
                    }
                }
            }
            for (pid, identity) in owned {
                if !effigy_process::process_start_identity_matches(pid, &identity) {
                    continue;
                }
                let Ok(pid_raw) = i32::try_from(pid) else {
                    continue;
                };
                match nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(pid_raw),
                    nix::sys::signal::Signal::SIGKILL,
                ) {
                    Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
                    Err(error) => eprintln!("could not reap fixture PID {pid}: {error}"),
                }
                let until = Instant::now() + std::time::Duration::from_secs(2);
                while effigy_process::process_start_identity_matches(pid, &identity)
                    && Instant::now() < until
                {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                if effigy_process::process_start_identity_matches(pid, &identity) {
                    eprintln!("fixture PID {pid} remained after cleanup deadline");
                }
            }
        }
    }

    #[cfg(all(unix, test))]
    impl Drop for DoctorGitFixtureGuard {
        fn drop(&mut self) {
            self.print_evidence_before_cleanup();
            self.reap_owned_processes();
        }
    }

    #[cfg(all(unix, test))]
    struct DoctorProcessSibling {
        child: std::process::Child,
        pid: u32,
        identity: String,
        pgid: i32,
    }

    #[cfg(all(unix, test))]
    impl DoctorProcessSibling {
        fn spawn() -> Self {
            let child = Command::new("sleep")
                .arg("30")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("spawn unrelated sibling");
            let pid = child.id();
            let pgid = nix::unistd::getpgid(Some(nix::unistd::Pid::from_raw(
                i32::try_from(pid).expect("PID fits i32"),
            )))
            .expect("read unrelated sibling process group")
            .as_raw();
            let identity = effigy_process::process_start_identity(pid)
                .unwrap_or_else(|| panic!("read sibling generation for PID {pid}"));
            Self {
                child,
                pid,
                identity,
                pgid,
            }
        }

        fn assert_survived(&mut self) {
            assert!(effigy_process::process_start_identity_matches(
                self.pid,
                &self.identity
            ));
            assert!(self.child.try_wait().expect("inspect sibling").is_none());
        }
    }

    #[cfg(all(unix, test))]
    impl Drop for DoctorProcessSibling {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[cfg(all(unix, test))]
    fn assert_doctor_git_process_evidence(
        source_path: &Path,
        fixture_path: &Path,
        shell_path: &Path,
        invocation: &str,
    ) -> u64 {
        let source = std::fs::read_to_string(source_path).expect("read source evidence");
        let source_events = source
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("source event"))
            .filter(|event| event["invocation"] == invocation)
            .collect::<Vec<_>>();
        let fixture = std::fs::read_to_string(fixture_path).expect("read fixture evidence");
        let fixture_events = fixture
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("fixture event"))
            .filter(|event| event["invocation"] == invocation)
            .collect::<Vec<_>>();

        let term_index = source_events
            .iter()
            .position(|event| event["event"] == "signal" && event["details"]["signal"] == "TERM")
            .expect("TERM evidence");
        let kill_index = source_events
            .iter()
            .position(|event| event["event"] == "signal" && event["details"]["signal"] == "KILL")
            .expect("KILL evidence");
        let target = source_events[term_index]["details"]["target"]
            .as_i64()
            .expect("TERM target");
        let leader_pid = u64::try_from(-target).expect("positive leader PID");
        let leader_index = source_events
            .iter()
            .position(|event| {
                event["event"] == "leader_spawned"
                    && event["details"]["pid"].as_u64() == Some(leader_pid)
            })
            .expect("spawned leader for signal target");
        let reaped_index = source_events
            .iter()
            .position(|event| {
                event["event"] == "leader_reaped"
                    && event["details"]["pid"].as_u64() == Some(leader_pid)
            })
            .expect("reaped signal target leader");
        assert!(leader_index < term_index && term_index < kill_index && kill_index < reaped_index);

        let leader = &source_events[leader_index]["details"];
        assert_eq!(leader["pgid"].as_u64(), Some(leader_pid));
        assert!(leader["ppid"].as_u64().is_some());
        assert!(source_events[leader_index]["elapsed_ns"].as_u64().is_some());
        assert_eq!(leader["setpgid"]["return"].as_i64(), Some(0));
        assert!(leader["setpgid"]["errno"].is_null());
        let args = leader["args"].as_array().expect("recorded Git args");
        assert!(args.iter().any(|arg| arg == "status"));
        assert!(args.iter().any(|arg| arg == "--porcelain=v1"));

        let term = &source_events[term_index]["details"];
        let kill = &source_events[kill_index]["details"];
        assert_eq!(
            term["target"].as_i64(),
            Some(-i64::try_from(leader_pid).expect("leader PID fits i64"))
        );
        assert_eq!(kill["target"], term["target"]);
        assert_eq!(term["result"], "ok");
        assert_eq!(kill["result"], "ok");
        assert!(term["errno"].is_null() && kill["errno"].is_null());
        assert_eq!(
            source_events[term_index]["deadline_remaining_ns"].as_u64(),
            Some(0)
        );
        assert_eq!(
            source_events[kill_index]["deadline_remaining_ns"].as_u64(),
            Some(0)
        );
        let leader_elapsed = source_events[leader_index]["elapsed_ns"]
            .as_u64()
            .expect("leader monotonic timestamp");
        let term_elapsed = source_events[term_index]["elapsed_ns"]
            .as_u64()
            .expect("TERM monotonic timestamp");
        let kill_elapsed = source_events[kill_index]["elapsed_ns"]
            .as_u64()
            .expect("KILL monotonic timestamp");
        let reaped_elapsed = source_events[reaped_index]["elapsed_ns"]
            .as_u64()
            .expect("reap monotonic timestamp");
        assert!(leader_elapsed <= term_elapsed && term_elapsed <= kill_elapsed);
        assert!(kill_elapsed <= reaped_elapsed);
        assert_eq!(term["ready_at_signal"], true);
        assert_eq!(term["late_ready_at_signal"], false);
        assert_eq!(kill["ready_at_signal"], true);
        assert_eq!(kill["late_ready_at_signal"], true);
        let status = &source_events[reaped_index]["details"]["status"];
        assert_eq!(status["code"].as_i64(), Some(0));
        assert!(status["signal"].is_null());

        let shell_events = std::fs::read_to_string(shell_path).expect("read shell events");
        assert!(shell_events.lines().any(|line| {
            line.starts_with(&format!("event=term_handler_entered pid={leader_pid} "))
        }));
        let mut parent_generation = None;
        for role in ["initial", "late"] {
            let process = fixture_events
                .iter()
                .find(|event| event["event"] == "process_started" && event["role"] == role)
                .unwrap_or_else(|| panic!("missing {role} process generation: {fixture_events:?}"));
            let pid = process["pid"].as_u64().expect("descendant PID");
            assert!(process["start_identity"]
                .as_str()
                .is_some_and(|id| !id.is_empty()));
            assert_eq!(process["ppid"].as_u64(), Some(leader_pid));
            assert_eq!(process["pgid"].as_u64(), Some(leader_pid));
            let generation = process["parent_start_identity"]
                .as_str()
                .filter(|identity| !identity.is_empty())
                .expect("leader process generation");
            if let Some(initial_generation) = parent_generation {
                assert_eq!(generation, initial_generation);
            } else {
                parent_generation = Some(generation);
            }
            assert!(fixture_events.iter().any(|event| {
                event["event"] == "process_ready"
                    && event["role"] == role
                    && event["pid"].as_u64() == Some(pid)
            }));
            let fixture_elapsed = |event: &serde_json::Value| {
                event["elapsed_ns"]
                    .as_u64()
                    .expect("fixture monotonic timestamp")
            };
            let entered = fixture_events
                .iter()
                .find(|event| event["event"] == "child_entered_fixture" && event["role"] == role)
                .expect("fixture entry phase");
            let process_started = fixture_events
                .iter()
                .find(|event| event["event"] == "process_started" && event["role"] == role)
                .expect("process identity phase");
            let readiness_started = fixture_events
                .iter()
                .find(|event| {
                    event["event"] == "readiness_publish_started" && event["role"] == role
                })
                .expect("readiness publish phase");
            let process_ready = fixture_events
                .iter()
                .find(|event| event["event"] == "process_ready" && event["role"] == role)
                .expect("ready phase");
            assert!(fixture_elapsed(entered) <= fixture_elapsed(process_started));
            assert!(fixture_elapsed(process_started) <= fixture_elapsed(readiness_started));
            assert!(fixture_elapsed(readiness_started) <= fixture_elapsed(process_ready));
        }
        assert!(fixture_events.iter().any(|event| {
            event["event"] == "descendant_received_term" && event["role"] == "initial"
        }));
        assert!(!fixture_events.iter().any(|event| {
            event["event"] == "descendant_received_term" && event["role"] == "late"
        }));
        leader_pid
    }

    #[cfg(all(unix, test))]
    fn assert_doctor_fixture_processes_reaped(events_path: &Path, invocation: &str) {
        let events = std::fs::read_to_string(events_path).expect("read process generations");
        for event in events
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("fixture event"))
            .filter(|event| event["invocation"] == invocation)
            .filter(|event| event["event"] == "process_started")
        {
            let pid = event["pid"].as_u64().expect("fixture PID");
            let pid = u32::try_from(pid).expect("fixture PID fits process API");
            let identity = event["start_identity"]
                .as_str()
                .expect("fixture process generation");
            assert!(
                !effigy_process::process_start_identity_matches(pid, identity),
                "fixture process {pid} generation {identity} survived group KILL"
            );
        }
    }
}
