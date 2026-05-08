# 099 Implement Manifest-Driven Distribution Contract Foundation

Status: archived
Updated: 2026-04-14
Roadmap: `g02.005`
Spec: `docs/specs/archive/005-optional-distribution-surface-strict-lane.md`

## Objective

Implement the first minimal manifest-driven distribution contract so Effigy's
current distribution built-ins become optional cross-repo infrastructure rather
than mostly Effigy-self-hosting policy.

## In Scope

- define the minimal optional `[distribution]` manifest shape needed to remove
  the hardest Effigy-specific assumptions from the current distribution built-ins
- wire at least one currently self-hosting-biased command through manifest
  policy instead of hardcoded Effigy defaults
- establish the generic documentation contract for cross-repo adoption
- keep the surface optional and composable rather than forcing a full release
  protocol

## Out Of Scope

- editing `.github/workflows/` without explicit human approval
- solving every distribution policy choice in one batch
- changing unrelated Rhai or demo lanes
- forcing any consumer repo to adopt the new distribution config immediately

## Acceptance Criteria

- a minimal optional `[distribution]` config exists and is documented
- at least one distribution command meaningfully uses manifest-driven policy
  instead of Effigy-specific assumptions
- the docs describe partial adoption as well as fuller adoption
- the lane remains bounded with one clear follow-up card

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Use the next decision batch to choose whether the optional distribution lane
should widen command coverage internally first or move straight to one bounded
consumer-proof adoption.
