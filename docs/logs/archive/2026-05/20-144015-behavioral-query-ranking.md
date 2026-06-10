# Behavioral Query Ranking

Date: 2026-05-20
Card: [`1024`](../../../roadmaps/g07/batch-cards/1024-improve-behavioral-query-ranking.md)
Strict lane: [`096`](../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md)

## Summary

Reduced graph phrasing sensitivity for behavior-shaped questions without adding
Effigy-only ranking hacks.

The ranking path now does three generic things:

- drops repo-identity tokens from request matching
- adds light singular/plural normalization
- expands a small behavior vocabulary for prompt, shutdown, exit, validation,
  redirect, migration, cache, and index terms

That was enough to make the natural shell-exit prompt query land on the actual
owner file in Effigy, while synthetic non-Effigy fixtures for redirect and
migration-validation behavior also ranked correctly.

## Vision Target Delta

Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`

Baseline:

- owner-shaped queries were strong
- behavior-shaped queries were still sensitive to repo wording and token shape
- the live shell-exit prompt question over-ranked generic container/session
  owners

Current:

- repo-name tokens no longer dominate ranking
- plural tokens such as `containers`, `redirects`, and `migrations` now still
  reach singular owner names
- behavior questions now expand into a bounded, explainable vocabulary instead
  of relying on exact internal identifiers
- the live Effigy shell-exit prompt query now ranks
  `src/runner/container_command/closeout.rs` first

Remaining:

- edit-target and related-test packets still need tightening in `1025`
- measured cross-repo benchmark proof still needs to be formalized in `1026`

## Implementation Notes

Changed surfaces:

- `crates/effigy-codegraph/src/query/profile.rs`
- `crates/effigy-codegraph/src/query/mod.rs`
- `crates/effigy-codegraph/src/tests/context_quality.rs`

Behavior kept intentionally bounded:

- no LLM dependency
- no fuzzy match flood
- no Effigy-only module-name table
- no hidden path boost for this repo

The key generic rule is that the repo root name is treated as identity noise
for ranking. That helps any repo where an agent includes the project name in a
question, not just Effigy.

## Proof

Focused behavior tests:

- shell-exit prompt wording prefers prompt-owner fixture
- redirect wording prefers redirect-owner fixture
- migration validation wording prefers migration-owner fixture

Broader validation:

- `cargo test -p effigy-codegraph --quiet`

Live Effigy query:

```bash
effigy graph explore "where does effigy prompt to shut containers down on shell exit" --json
```

Top owner after the change:

- `src/runner/container_command/closeout.rs`

Interpretation:

- the improvement is real on the live repo
- the proof remains generic because the fixture-backed cases do not depend on
  Effigy-specific names or paths

## Next

Move to `1025`: add sharper edit-target and related-test packets.
