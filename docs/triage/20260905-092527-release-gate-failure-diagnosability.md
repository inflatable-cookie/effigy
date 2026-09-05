# Release Gate Keep-On-Failure Decision

Status: open — unscheduled remainder
Created: 2026-09-05
Owner: chatterbox
Source: Swallowtail Chatterbox handoff (2026-09-05), request 3 of 4
Promoted: requests 1, 2, and 4 became [`g09.004`](../roadmaps/g09/004-release-gate-diagnosability.md)
/ strict spec `119` / card `1112` on 2026-09-05

## Issue

Swallowtail asked for a `--keep-on-failure` mode on `release prepare` that
leaves the mutated `Cargo.toml`, changelog, and lock in place with a marker
instead of rolling back, so an operator can rerun the failing gate by hand
against the exact prepared state.

## Known

- Rollback on gate failure is a deliberate invariant: a mutated tree must not
  survive behind a `Prepared: no` report (comment in
  `crates/effigy-release/src/lib.rs`, gate-failure path).
- Prepare refuses to run without `--check-gates` when gates are configured, so
  there is no sanctioned way to inspect the prepared tree today.
- Persisted gate logs and environment capture (`g09.004`) may remove most of
  the need; Swallowtail ranked this request third.

## Unknown

- What `release status`, the state file, and `execute` say about a kept but
  unprepared tree, and how it is cleaned up.
- Whether a read-only alternative (render the prepared mutations as a diff
  without applying) satisfies the same need with no invariant change.

## Next Task

Revisit after `g09.004` ships and Swallowtail's next authorized attempt shows
whether persisted logs were enough. Do not schedule before then.
