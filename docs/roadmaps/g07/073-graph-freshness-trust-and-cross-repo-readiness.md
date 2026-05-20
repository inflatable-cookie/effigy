# g07.073 - Graph Freshness Trust And Cross-Repo Readiness

Status: Complete
Depends on: `g07.072`

## Goal

Make graph freshness cheap enough that agents trust `graph explore` before
falling back to broad file scans.

The graph must work in ordinary Effigy-adopting repos, not just in this repo.
The trust model should handle large Rust repos, PHP/JS legacy apps, Underlay
apps, and small projects with sparse language support.

## Problem

The current status surface is accurate but too noisy for agent flow. During the
audit, `graph status --json` exposed large `changed_paths`, `new_paths`,
`deleted_paths`, and `stale_paths` sets. After `graph index --json`, `explore`
reported ready, but the operator still had to interpret a lot of raw freshness
detail.

That creates friction. Agents tend to skip tools whose trust state requires
manual diagnosis.

## Scope

- inspect the current freshness model and JSON contract
- distinguish repo-wide detail from the one-line trust decision agents need
- make `graph explore` surface freshness confidence in a compact way
- keep detailed changed/stale path lists available for debugging
- verify behavior on at least:
  - Effigy
  - one Underlay repo
  - one decodelabs app or library repo
  - one small fixture repo without Rust-heavy structure

## Guardrails

- do not hide stale state
- do not auto-index in a command where users expect read-only behavior unless
  that behavior is explicitly designed and documented
- do not assume `.effigy/graph` is present or warm
- do not make a repo-specific ignore list to make Effigy look better
- do not change the DB schema unless a focused proof requires it

## Acceptance Criteria

- agents can decide whether to trust `graph explore` from a compact freshness
  field
- stale index states remain diagnosable through detailed JSON
- fresh, stale, missing-index, and partially failed-index cases are covered by
  tests or fixtures
- cross-repo manual proof records whether the signal is usable outside Effigy

## Next Task

Execute `1023`.
