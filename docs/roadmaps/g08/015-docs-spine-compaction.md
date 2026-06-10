# g08.015 - Docs Spine Compaction

Status: Planned
Depends on: `g08.014`

## Goal

Restore signal-to-noise in the docs spine. Remediates assessment finding 6.

The tree holds 2141 markdown files / ~11 MB. `docs/roadmaps/` alone is ~1122
files and `docs/logs/` ~677. At that volume, staleness is near-certain and no
human can cross-check the front-door pointers. This milestone compacts the
corpus under the existing generation and docs-policy model without losing
planning history.

This lane is compiled last in the suite deliberately: doing it before the
security milestones would churn planning docs mid-remediation.

## Scope

- audit `docs/roadmaps/` and `docs/logs/` for closed-generation material that
  can be archived or consolidated per the rollover/closeout model
- define a durable retention/compaction convention (what stays inline, what is
  archived, how closed generations collapse to a summary) and record it in the
  docs-policy / roadmaps README so the corpus does not re-bloat
- consolidate per-batch logs for fully closed generations into generation-level
  summaries where the rollover model already permits it
- verify front-door truth after compaction: roadmaps README, generation-index,
  and `docs/README.md` must still agree on live vs closed state
- run `effigy docs` QA (link/index/json-example checks) to prove no dangling
  references were introduced

## Guardrails

- archive, never silently delete, planning history — closeouts and decisions are
  the record of why the system is shaped as it is
- do not touch live or near-live planning surfaces (open milestones, ready
  cards, active specs)
- do not break front-door pointer integrity; `effigy docs` must pass after each
  batch
- preserve the generation model and IDs; compaction summarizes closed
  generations, it does not renumber them
- this is posture work, not behavior — no code or CLI change

## Execution Plan

- [ ] **Batch A — Retention convention.** Define and document the
  compaction/retention rules (inline vs archive vs summary) in docs-policy and
  the roadmaps README. No file moves yet.
- [ ] **Batch B — Closed-generation log consolidation.** Consolidate
  per-batch logs for fully closed generations (`g01`–`g07`) into
  generation-level summaries under the archive convention; update references.
- [ ] **Batch C — Roadmap corpus compaction.** Archive/collapse
  closed-generation roadmap material that the rollover model already permits;
  re-verify front-door truth.
- [ ] **Batch D — Integrity proof.** Run `effigy docs` QA, fix any dangling
  links/indexes, and record the before/after file-count and size delta.

## Governing Contracts

- [`001-working-rules.md`](../../contracts/001-working-rules.md)
- docs-policy reference (`crates/effigy-docs-policy`, `docs/policy/`) and the
  roadmaps generation/rollover model in
  [`docs/roadmaps/README.md`](../README.md)

## Acceptance Criteria

- [ ] a documented retention/compaction convention exists and is referenced from
  the docs front doors
- [ ] closed-generation logs/roadmaps are archived or summarized without loss of
  decision history
- [ ] roadmaps README, generation-index, and `docs/README.md` agree on live vs
  closed state after compaction
- [ ] `effigy docs` QA passes with no dangling links or indexes
- [ ] the closeout records the file-count and size reduction achieved

## Next Task

This is the final milestone of the g08.010 hardening suite. On completion, run
suite closeout in `g08.010` recording per-finding remediation status, then
assess whether the residual scope justifies a `g09` rollover.
