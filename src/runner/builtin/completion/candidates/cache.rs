use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

use super::super::super::super::catalog::discover_catalogs;
use super::super::super::super::RunnerError;
use super::super::scripts::command_names;

const CANDIDATE_CACHE_TTL: Duration = Duration::from_secs(2);

pub(super) fn completion_candidates_cache_ttl_ms() -> u64 {
    CANDIDATE_CACHE_TTL.as_millis() as u64
}

#[derive(Clone)]
struct CompletionCandidatesSnapshot {
    created_at: Instant,
    manifest_stamps: Vec<ManifestStamp>,
    candidates: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
struct ManifestStamp {
    path: PathBuf,
    modified_epoch_ns: Option<u128>,
    len_bytes: Option<u64>,
    content_hash_fnv1a64: Option<u64>,
}

static COMPLETION_CANDIDATES_CACHE: OnceLock<
    Mutex<HashMap<PathBuf, CompletionCandidatesSnapshot>>,
> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CompletionCandidatesCacheState {
    MissInitial,
    Hit,
    MissTtl,
    MissManifestChange,
}

impl CompletionCandidatesCacheState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::MissInitial => "miss_initial",
            Self::Hit => "hit",
            Self::MissTtl => "miss_ttl",
            Self::MissManifestChange => "miss_manifest_change",
        }
    }
}

enum CacheLookup {
    Hit {
        candidates: Vec<String>,
        manifest_count: usize,
        cache_age_ms: u128,
    },
    MissInitial,
    MissTtl,
    MissManifestChange,
}

pub(super) fn load_completion_candidates_with_cache(
    repo_root: &Path,
) -> Result<
    (
        Vec<String>,
        CompletionCandidatesCacheState,
        usize,
        Option<u128>,
    ),
    RunnerError,
> {
    let now = Instant::now();
    let cache = COMPLETION_CANDIDATES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let miss_reason = match read_cached_completion_candidates(repo_root, now, cache)? {
        CacheLookup::Hit {
            candidates,
            manifest_count,
            cache_age_ms,
        } => {
            return Ok((
                candidates,
                CompletionCandidatesCacheState::Hit,
                manifest_count,
                Some(cache_age_ms),
            ))
        }
        CacheLookup::MissInitial => CompletionCandidatesCacheState::MissInitial,
        CacheLookup::MissTtl => CompletionCandidatesCacheState::MissTtl,
        CacheLookup::MissManifestChange => CompletionCandidatesCacheState::MissManifestChange,
    };

    let (candidates, manifest_stamps) = discover_completion_candidates(repo_root)?;
    let manifest_count = manifest_stamps.len();
    let snapshot = CompletionCandidatesSnapshot {
        created_at: now,
        manifest_stamps,
        candidates: candidates.clone(),
    };

    let mut map = cache
        .lock()
        .map_err(|_| RunnerError::Ui("completion candidate cache lock poisoned".to_owned()))?;
    map.insert(repo_root.to_path_buf(), snapshot);
    Ok((candidates, miss_reason, manifest_count, None))
}

fn read_cached_completion_candidates(
    repo_root: &Path,
    now: Instant,
    cache: &Mutex<HashMap<PathBuf, CompletionCandidatesSnapshot>>,
) -> Result<CacheLookup, RunnerError> {
    let map = cache
        .lock()
        .map_err(|_| RunnerError::Ui("completion candidate cache lock poisoned".to_owned()))?;
    let Some(snapshot) = map.get(repo_root) else {
        return Ok(CacheLookup::MissInitial);
    };
    if now.duration_since(snapshot.created_at) > CANDIDATE_CACHE_TTL {
        return Ok(CacheLookup::MissTtl);
    }
    if !manifest_stamps_unchanged(&snapshot.manifest_stamps) {
        return Ok(CacheLookup::MissManifestChange);
    }
    Ok(CacheLookup::Hit {
        candidates: snapshot.candidates.clone(),
        manifest_count: snapshot.manifest_stamps.len(),
        cache_age_ms: now.duration_since(snapshot.created_at).as_millis(),
    })
}

fn discover_completion_candidates(
    repo_root: &Path,
) -> Result<(Vec<String>, Vec<ManifestStamp>), RunnerError> {
    let mut candidates: BTreeSet<String> = command_names().into_iter().map(str::to_owned).collect();
    let mut manifest_stamps: Vec<ManifestStamp> = Vec::new();
    match discover_catalogs(repo_root) {
        Ok(catalogs) => {
            for catalog in catalogs {
                manifest_stamps.push(read_manifest_stamp(&catalog.manifest_path));
                for task_name in catalog.manifest.tasks.keys() {
                    candidates.insert(task_name.clone());
                    candidates.insert(format!("{}/{}", catalog.alias, task_name));
                }
            }
        }
        Err(RunnerError::TaskCatalogsMissing { .. }) => {}
        Err(error) => return Err(error),
    }
    manifest_stamps.sort_by(|a, b| a.path.cmp(&b.path));

    Ok((
        candidates.into_iter().collect::<Vec<String>>(),
        manifest_stamps,
    ))
}

fn manifest_stamps_unchanged(expected: &[ManifestStamp]) -> bool {
    expected
        .iter()
        .all(|stamp| read_manifest_stamp(&stamp.path) == *stamp)
}

fn read_manifest_stamp(path: &Path) -> ManifestStamp {
    let metadata = fs::metadata(path).ok();
    let modified_epoch_ns = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    let contents = fs::read(path).ok();
    let len_bytes = contents
        .as_ref()
        .map(|bytes| bytes.len() as u64)
        .or_else(|| metadata.as_ref().map(|value| value.len()));
    let content_hash_fnv1a64 = contents.as_ref().map(|bytes| fnv1a64(bytes));

    ManifestStamp {
        path: path.to_path_buf(),
        modified_epoch_ns,
        len_bytes,
        content_hash_fnv1a64,
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
