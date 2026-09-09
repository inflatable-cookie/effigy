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

- Active generation: none.
- Approved frontier: none.
- Active strict lane: none.
- Dispatch handoff: none.
- `g01` through `g09` are closed and compacted into
  [`archive/`](./archive/).

No product execution is authorized from the archived roll-ups, backlog, logs,
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

## Backlog

Deferred planning lives in [`backlog/README.md`](./backlog/README.md). Backlog
items are not executable. Promotion requires operator intent, current canonical
refs, an active generation, and a ready top-level Northstar task.

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

Use Northstar Atlas with the operator to choose the next strategic runway. Do
not open `g10` or compile a task before that direction is settled. Effigy
release execution and S3 retirement remain separately gated.

## Historical language boundary

Closed logs, specs, roll-ups, and Git history may retain “milestone”, “card”,
and older phase language when reporting what existed. New and actively
maintained planning uses generation-plus-task terminology.

## Historical command boundary

Historical evidence may preserve retired wrapper scripts or command spellings.
Current operator guidance comes from active guides and contracts.
