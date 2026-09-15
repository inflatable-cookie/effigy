# Roadmaps

Northstar generations are substantial sequencing eras. The generation README
owns the roadmap and approved frontier; each executable unit is one top-level
Northstar task.

## Generation model

- Open `docs/roadmaps/gNN/` only through an explicit operator-owned rollover.
- Put the generation roadmap and approved frontier in `gNN/README.md`.
- Put each task at `gNN/NNN-<slug>.md` and reference it as `gNN.NNN`.
- Use [`templates/task-template.md`](./templates/task-template.md) for every new
  task.
- Do not create milestone wrappers, nested `batch-cards/`, or a second status
  surface for the same executable unit.
- Size tasks around coherent, reviewable outcomes. A generation should normally
  carry roughly 20–50 tasks before rollover is worth discussing.

Use “Northstar task” for this planning unit. “Queue task” means a Northstar
Queue execution record. “Effigy task” means a command selector.

## Current planning state

- Active generation: [`g10`](./g10/README.md) — agent-native skill execution.
- Approved frontier:
  [`g10.007`](./g10/007-refresh-vulnerable-and-yanked-cargo-lock.md), the sole
  ready dependency-maintenance task. `g10.006` is already dispatched and
  blocked in verification; `g10.001` through `g10.005` are complete.
- Active strict lane: none.
- Dispatch handoff: none pending hook closeout.
- `g01` through `g09` are closed and compacted into
  [`archive/`](./archive/).

No product execution is authorized from the archived roll-ups, triage notes, logs,
or closed specs.

## Generation history

- [`g01`](./archive/g01.md) — original implementation and consolidation
- [`g02`](./archive/g02.md) — release and local-runtime expansion
- [`g03`](./archive/g03.md) — production export and runtime hardening
- [`g04`](./archive/g04.md) — runtime architecture simplification
- [`g05`](./archive/g05.md) — secret and reusable-core hardening
- [`g06`](./archive/g06.md) — codebase lean-down
- [`g07`](./archive/g07.md) — code graph and agent adoption
- [`g08`](./archive/g08.md) — scan, dependency, and documentation intelligence
- [`g09`](./archive/g09.md) — operator and consumer contract clarity

The roll-ups preserve durable outcomes, current destinations, material
evidence, retained risks, and succession. Git remains the detailed milestone,
batch-card, audit, and validation archive.

## Triage

Unresolved or deferred candidates live in [`../triage/README.md`](../triage/README.md).
Triage notes are non-authoritative and never executable. Promotion requires
operator intent, current canonical refs, an active generation, and a ready
top-level Northstar task.

## Research program

Comparative research remains under [`../research/README.md`](../research/README.md).
Research must be promoted into current architecture/contracts before a task can
depend on it.

## Logging rule

Create one log per completed task or coherent update cycle. Do not create one
log per agent turn.

## Retention and archival convention

- Closed-generation logs move under `docs/logs/archive/<month>/`; never delete
  them for compaction.
- A safely closed expanded generation becomes one non-procedural
  `docs/roadmaps/archive/gNN.md` roll-up. Remove its expanded tree only after the
  preservation manifest and link rewrite pass.
- Current front doors must remain usable without opening archived material.
- Archived roll-ups and Git are evidence, never live execution authority.

## Rollover guardrail

Before opening the next generation:

- every task in the closing generation is closed, superseded, or rehomed;
- front doors agree the old generation is no longer active;
- open commitments have current homes;
- stale generation-specific specs are archived or removed;
- the closing generation is compacted through the lifecycle procedure.

Do not open a new generation to escape cleanup or manufacture a ready task.

## Next Task

Dispatch `g10.007` through Northstar Queue. After hook-owned closeout, resume
the existing g10.006 Queue task and retained worker to rebase and revalidate
PR #109. Effigy release execution and S3 retirement remain separately gated.

## Historical language boundary

Closed logs, specs, roll-ups, and Git history may retain “milestone”, “card”,
and older phase language when reporting what existed. New and actively
maintained planning uses generation-plus-task terminology.

## Historical command boundary

Historical evidence may preserve retired wrapper scripts or command spellings.
Current operator guidance comes from active guides and contracts.
<!-- northstar:lifecycle:begin schema=northstar.lifecycle.projection.v2 digest=sha256:74b7940539f4ed87e62b25a9b35339afeb81b9a2dcbdc6e8ddfcbce9e9abe72c -->
| Generation | Disposition | Runway state |
| --- | --- | --- |
| g10 | open | planning_required |
| Task | Status | Stage | Revision | Record digest |
| --- | --- | --- | --- | --- |
| g10.002 | complete | none | 8 | sha256:acc40985e9f1420efa1189899daec76abc3be7d6f143bbc28dac2a8f66dcd833 |
| g10.003 | complete | none | 8 | sha256:f51ae9d2f79a84dfa1aa970575d37e10442574d5025015bf725258c72f2bd9a5 |
| g10.004 | complete | none | 8 | sha256:493fff197276ab92aab90e09838bed45ba1de970a3a5d11cbc5fa8091e719e5d |
| g10.005 | complete | none | 8 | sha256:bbdc3e4907f88ceb64be891fff87847854e123e4d552c565080fcda70a4d8881 |
| g10.006 | complete | none | 8 | sha256:b0134c685fb320d7a8acdcd4f0db4a6765270c3dd8b8bb61496cad0682e6dc49 |
| g10.007 | complete | none | 8 | sha256:04342987457e1bea973556297a4bef39e1592794e0dd3d52ebcb4457ae915995 |
<!-- northstar:lifecycle:end -->
