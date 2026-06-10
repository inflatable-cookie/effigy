# 1015 - Split Manifest Semantic Ownership

Roadmap: [`../065-manifest-semantic-owner-split.md`](../065-manifest-semantic-owner-split.md)
Strict lane: [`../../../specs/095-residual-maintainability-follow-through-strict-lane.md`](../../../specs/095-residual-maintainability-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Reduce `language/manifest/semantic.rs` into clearer semantic owners.

## Work

- inspect the current fact families inside `semantic.rs`
- split the file by real manifest domains or relation families
- keep public graph behavior stable
- run focused codegraph validation before moving on

## Guardrails

- no graph storage changes
- no ranking rewrite
- no public JSON changes
- no generic `part1` / `part2` style split

## Acceptance

- `semantic.rs` is no longer one large mixed owner, or any remaining size is
  explicitly justified
- focused graph tests pass

## Evidence

- [`../../../logs/archive/2026-05/19-230843-manifest-semantic-owner-split.md`](../../../logs/archive/2026-05/19-230843-manifest-semantic-owner-split.md)

## Next Task

Execute `1016`.
