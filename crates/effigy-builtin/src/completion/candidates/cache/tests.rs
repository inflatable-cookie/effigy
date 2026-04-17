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
