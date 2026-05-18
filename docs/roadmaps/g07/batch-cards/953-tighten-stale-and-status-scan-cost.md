# 953 - Tighten Stale And Status Scan Cost

Roadmap: [`../019-safe-scan-metadata-reuse.md`](../019-safe-scan-metadata-reuse.md)
Strict lane: [`../../../specs/087-graph-scan-cost-reduction-strict-lane.md`](../../../specs/087-graph-scan-cost-reduction-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Carry the scan-cost reductions into stale detection and `graph status`.

## Scope

- reduce repeated scan work in status/stale paths
- keep stale reporting deterministic
- re-measure the command surfaces touched by the scan reuse

## Acceptance

- `graph status --json` gets measurably cheaper, or the retained floor is
  explicitly explained

## Results

- collapsed `graph status` onto one scan snapshot
- removed the repeated `new`/`changed`/`deleted`/`stale` repo walks
- measured clean `graph status --json` improvement: `0.48s -> 0.21s–0.24s`

## Next Task

Execute `954`.
