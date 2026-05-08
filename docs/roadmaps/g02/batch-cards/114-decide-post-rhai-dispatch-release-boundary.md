# 114 Decide Post Rhai Dispatch Release Boundary

Status: archived
Updated: 2026-04-15
Roadmap: `g02.007`
Spec: `docs/specs/007-distribution-release-and-consumer-rollout-strict-lane.md`

## Objective

Decide whether the new in-process Rhai Effigy dispatch surface closes the last
release-prep hardening gap before the actual Effigy release batch.

## In Scope

- assess the new Rhai host API against the release-closure goal
- judge whether the remaining work is now true release execution rather than
  runtime hardening
- leave one explicit next ready card or pause boundary

## Out Of Scope

- executing the actual release
- broad consumer rollout work
- further general Rhai API widening beyond what release/container scripting now
  needs

## Acceptance Criteria

- the lane states clearly whether runtime hardening is now sufficient for
  release closure work
- any residual gap is concrete and bounded
- the next move is explicit

## Validation

- docs/state surfaces updated honestly
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `115-implement-effigy-distribution-release-closure.md` to carry the
shipped distribution surface through the actual Effigy release batch.
