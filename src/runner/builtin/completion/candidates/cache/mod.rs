use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use self::manifests::{discover_completion_candidates, manifest_stamps_unchanged, ManifestStamp};
use self::policy::completion_candidates_cache_ttl_policy;
use super::super::super::super::RunnerError;

mod manifests;
mod policy;

pub(super) fn completion_candidates_cache_ttl_ms() -> u64 {
    completion_candidates_cache_ttl_policy().ttl_ms
}

pub(super) fn completion_candidates_cache_ttl_source() -> &'static str {
    completion_candidates_cache_ttl_policy().source
}

#[derive(Clone)]
struct CompletionCandidatesSnapshot {
    created_at: Instant,
    manifest_stamps: Vec<ManifestStamp>,
    candidates: Vec<String>,
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
    if now.duration_since(snapshot.created_at)
        > Duration::from_millis(completion_candidates_cache_ttl_ms())
    {
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

#[cfg(test)]
mod tests {
    use super::policy::{
        parse_completion_candidates_cache_ttl_policy, CompletionCandidatesCacheTtlPolicy,
    };

    #[test]
    fn completion_candidates_cache_ttl_defaults_when_unset() {
        assert_eq!(
            parse_completion_candidates_cache_ttl_policy(None),
            CompletionCandidatesCacheTtlPolicy {
                ttl_ms: 2_000,
                source: "default",
            }
        );
    }

    #[test]
    fn completion_candidates_cache_ttl_uses_valid_override() {
        assert_eq!(
            parse_completion_candidates_cache_ttl_policy(Some("750")),
            CompletionCandidatesCacheTtlPolicy {
                ttl_ms: 750,
                source: "env",
            }
        );
    }

    #[test]
    fn completion_candidates_cache_ttl_clamps_below_min() {
        assert_eq!(
            parse_completion_candidates_cache_ttl_policy(Some("5")),
            CompletionCandidatesCacheTtlPolicy {
                ttl_ms: 100,
                source: "env",
            }
        );
    }

    #[test]
    fn completion_candidates_cache_ttl_clamps_above_max() {
        assert_eq!(
            parse_completion_candidates_cache_ttl_policy(Some("999999")),
            CompletionCandidatesCacheTtlPolicy {
                ttl_ms: 60_000,
                source: "env",
            }
        );
    }

    #[test]
    fn completion_candidates_cache_ttl_ignores_invalid_override() {
        assert_eq!(
            parse_completion_candidates_cache_ttl_policy(Some("not-a-number")),
            CompletionCandidatesCacheTtlPolicy {
                ttl_ms: 2_000,
                source: "env_invalid",
            }
        );
    }
}
