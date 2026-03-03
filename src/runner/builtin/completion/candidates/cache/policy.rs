const COMPLETION_CANDIDATES_CACHE_TTL_ENV: &str = "EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS";
const DEFAULT_CANDIDATE_CACHE_TTL_MS: u64 = 2_000;
const MIN_CANDIDATE_CACHE_TTL_MS: u64 = 100;
const MAX_CANDIDATE_CACHE_TTL_MS: u64 = 60_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CompletionCandidatesCacheTtlPolicy {
    pub(super) ttl_ms: u64,
    pub(super) source: &'static str,
}

pub(super) fn completion_candidates_cache_ttl_policy() -> CompletionCandidatesCacheTtlPolicy {
    parse_completion_candidates_cache_ttl_policy(
        std::env::var(COMPLETION_CANDIDATES_CACHE_TTL_ENV)
            .ok()
            .as_deref(),
    )
}

pub(super) fn parse_completion_candidates_cache_ttl_policy(
    raw: Option<&str>,
) -> CompletionCandidatesCacheTtlPolicy {
    match raw {
        Some(value) => match value.parse::<u64>() {
            Ok(parsed) => CompletionCandidatesCacheTtlPolicy {
                ttl_ms: parsed.clamp(MIN_CANDIDATE_CACHE_TTL_MS, MAX_CANDIDATE_CACHE_TTL_MS),
                source: "env",
            },
            Err(_) => CompletionCandidatesCacheTtlPolicy {
                ttl_ms: DEFAULT_CANDIDATE_CACHE_TTL_MS,
                source: "env_invalid",
            },
        },
        None => CompletionCandidatesCacheTtlPolicy {
            ttl_ms: DEFAULT_CANDIDATE_CACHE_TTL_MS,
            source: "default",
        },
    }
}
