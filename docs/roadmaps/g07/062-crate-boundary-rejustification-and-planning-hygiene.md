# g07.062 - Crate Boundary Rejustification And Planning Hygiene

Status: Planned
Depends on: `g07.061`

## Goal

Review crate boundaries and active planning state after the cleanup work, then
record what should be kept, merged, or deferred.

## Evidence

The audit found 34 workspace crates. That count is not a bug, but it is large
enough that every crate should still justify its boundary.

The audit also found planning hygiene drift: completed or paused strict lanes
remain in the active specs tree with README notes saying they should be
archived in the next cleanup batch.

## Scope

- inventory small or adapter-shaped crates
- check public API size, dependency direction, and actual reuse
- identify crates to keep, merge, or re-audit later
- archive completed/paused spec files that the specs README already marks for
  cleanup
- update roadmap/spec indexes so continuation points are unambiguous

## Guardrails

- do not merge crates during this card unless the merge is tiny, obvious, and
  already proven safe by the earlier cleanup cards
- do not archive active planning needed for current execution
- do not rewrite historical roadmap text
- do not treat crate count as a standalone quality metric

## Suggested Implementation Shape

- produce a short crate-boundary table in docs or the closeout notes
- inspect dependency direction with `cargo metadata` or equivalent
- archive completed specs in a docs-only patch
- leave any real crate merge as a separately planned future lane if it carries
  behavioral risk

## Acceptance Criteria

- each suspicious small crate has a keep/merge/defer note
- active specs and roadmap README state the current lane accurately
- no completed stale ready cards remain active
- any future crate-merge work has enough evidence to be planned separately

## Next Task

After this lands, proceed to [`063-codebase-leanness-closeout.md`](./063-codebase-leanness-closeout.md).
