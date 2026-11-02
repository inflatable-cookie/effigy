use super::policy::{
    parse_completion_candidates_cache_ttl_policy, CompletionCandidatesCacheTtlPolicy,
};

/// The completion cache TTL maps its environment override through every
/// branch: the default when unset, the parsed value when valid, the floor
/// and ceiling when clamped, and the default (marked invalid) when
/// unparsable.
#[test]
fn completion_candidates_cache_ttl_maps_every_override_branch() {
    let cases = [
        (
            None,
            CompletionCandidatesCacheTtlPolicy {
                ttl_ms: 2_000,
                source: "default",
            },
        ),
        (
            Some("750"),
            CompletionCandidatesCacheTtlPolicy {
                ttl_ms: 750,
                source: "env",
            },
        ),
        (
            Some("5"),
            CompletionCandidatesCacheTtlPolicy {
                ttl_ms: 100,
                source: "env",
            },
        ),
        (
            Some("999999"),
            CompletionCandidatesCacheTtlPolicy {
                ttl_ms: 60_000,
                source: "env",
            },
        ),
        (
            Some("not-a-number"),
            CompletionCandidatesCacheTtlPolicy {
                ttl_ms: 2_000,
                source: "env_invalid",
            },
        ),
    ];

    for (raw, expected) in cases {
        assert_eq!(
            parse_completion_candidates_cache_ttl_policy(raw),
            expected,
            "override {raw:?}"
        );
    }
}
