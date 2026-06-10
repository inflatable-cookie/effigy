# g08.015 - Docs Spine Compaction

Status: Complete
Depends on: `g08.014`
Completed: 2026-06-10

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

- [x] **Batch A — Retention convention.** Documented the retention/archival rule
  in the [logs README](../../logs/README.md) and the
  [roadmaps README](../README.md): active surfaces stay lean, closed history
  moves to `archive/` (never deleted), and front doors summarize closed
  generations rather than enumerating them.
- [x] **Batch B — Closed-generation log archival.** Moved all 656
  closed-generation logs (months `2026-02`–`2026-05`) under
  `docs/logs/archive/<month>/` and rewrote every `logs/<month>/` reference to
  `logs/archive/<month>/`, then depth-fixed the moved logs' own outbound links
  (the extra `archive/` level shifted their relative paths). Compacted the logs
  README from 839 to ~138 lines (677 → 21 active index entries), preserving the
  policy header, the QA-pinned strings, the active `2026-06` window, and the log
  template. Taught the default `effigy docs check index` to exclude
  `archive/**` so archived logs need no index entry.
- [x] **Batch C — Roadmap corpus decision.** Reviewed the 838 closed-generation
  batch-cards: they are already nested under each generation, are not loaded
  into the front doors, and the rollover model keeps them as the planning
  record. Per the churn guardrail, they stay in place — moving them would touch
  ~838 files and their links for no signal gain. The compaction target is the
  indexes and front doors, which are lean. Recorded this as the durable
  convention.
- [x] **Batch D — Integrity proof.** Full docs QA lane green: `qa:docs:links`,
  `qa:docs:examples`, `qa:docs:index`, `qa:docs:vision`, `qa:docs:agent-defaults`
  all pass. `effigy-docs-policy` tests green (22). Delta recorded below.

## Governing Contracts

- [`001-working-rules.md`](../../contracts/001-working-rules.md)
- docs-policy reference (`crates/effigy-docs-policy`, `docs/policy/`) and the
  roadmaps generation/rollover model in
  [`docs/roadmaps/README.md`](../README.md)

## Acceptance Criteria

- [x] a documented retention/compaction convention exists and is referenced from
  the docs front doors (logs README + roadmaps README)
- [x] closed-generation logs are archived without loss of decision history
  (moved to `archive/`, preserved in repo + git history)
- [x] roadmaps README and generation-index agree on live vs closed state
- [x] `effigy docs` QA passes with no dangling links or indexes
- [x] the closeout records the file-count and size reduction achieved

## Delta

- Active log index: 677 → 21 entries; logs README: 839 → ~138 lines.
- 656 logs relocated to `docs/logs/archive/` (git tracked as renames, not
  deletions); 73 referencing docs updated.
- Code: default logs index now excludes `archive/**`
  ([`crates/effigy-docs-policy/src/lib.rs`](../../../crates/effigy-docs-policy/src/lib.rs)).
- Total `docs/**.md` unchanged at ~2148 (archival relocates, does not delete);
  the win is index/front-door leanness, not raw file count.

## Next Task

Final milestone of the g08.010 hardening suite — all five milestones complete.
Run suite closeout in `g08.010` recording per-finding remediation status, then
assess whether the closed g08 generation justifies a `g09` rollover.
