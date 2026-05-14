# 746 - Reduce High Duplicate Blocks

Roadmap: [`../025-low-risk-deduplication-follow-through.md`](../025-low-risk-deduplication-follow-through.md)
Strict lane: [`../../../specs/083-reusable-core-hardening-strict-lane.md`](../../../specs/083-reusable-core-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Reduce the highest-value duplicate blocks without widening scope into broad test
or help-system rewrites.

## Scope

- converge repeated CLI help topic descriptor structure
- add local fixture builders where bootstrap/release duplication is obviously
  shared setup rather than distinct proof
- rerun duplicate-block scan and record retained categories

## Acceptance

- high duplicate findings are reduced materially or explicitly justified
- help output and test intent remain stable

## Outcome

- extracted shared bootstrap root-repo fixtures into
  `crates/effigy-bootstrap/tests/support.rs` and reused them from both domain
  and runner test suites
- extracted shared release version-file assertions into
  `crates/effigy-release/src/test_support.rs`
- reused those release assertions from both `effigy-release` domain tests and
  runner release-command tests
- intentionally left the remaining large CLI help topic data blocks in place;
  they are still explicit, reviewable topic-local data and do not justify a
  larger help-system abstraction in this card

## Retained High Findings

- CLI help topic arrays across `bootstrap`, `container`, `docs`, and `release`
  remain as the main high-severity duplicate category
- one container temp-repo helper pair remains in
  `src/runner/container_command/lifecycle.rs` and
  `src/runner/container_command/shell_prep.rs`
- these are retained to avoid over-abstracting topic-local help content or
  unrelated container test/setup seams in this lane

## Stop Conditions

- stop if deduplication starts pulling unrelated crates into a global harness

## Validation

- `cargo test -p effigy-bootstrap`
- `cargo test release`
- `cargo fmt --all -- --check`
- `git diff --check`
- `effigy scan duplicate-blocks --json`

## Next Task

Execute `747`.
