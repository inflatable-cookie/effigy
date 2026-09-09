# 001 Working Rules

Status: active
Updated: 2026-09-09

This contract defines how Effigy plans and executes Northstar work using the
generation-plus-task model.

## Canonical Surfaces

Execution anchors on these surfaces in order:

1. `docs/roadmaps/generation-index.md`
2. `docs/roadmaps/README.md`
3. the active `docs/roadmaps/gNN/README.md`
4. the active top-level `docs/roadmaps/gNN/NNN-<slug>.md` Northstar task
5. current architecture and contracts named by that task
6. `docs/specs/README.md` and any active supporting spec
7. `docs/logs/README.md`

Historical roll-ups, specs, logs, handoffs, and queue records preserve evidence.
They are not live execution authority.

## Planning unit

The sole executable planning unit is a Northstar task at
`docs/roadmaps/gNN/NNN-<slug>.md`, referenced as `gNN.NNN`.

- The generation README owns the roadmap and approved frontier.
- Do not create milestone wrappers or nested `batch-cards/`.
- “Queue task” means a control-plane execution record.
- “Effigy task” means a command selector.

## Ready-State Rule

Implementation proceeds only from a bounded task whose status is `ready` and
whose ready-state rubric passes. It must name:

- owner, outcome, and governing refs;
- dependencies and dispatch boundaries;
- ordered work and mutable paths;
- acceptance/review oracle and validation;
- evidence requirements, continuation, and stop conditions.

If no ready task exists, the project remains in planning. Do not improvise
execution from a generation summary, triage note, archived roll-up, spec, log,
or old handoff.

## Continue Rule

In a strict Effigy lane, bare `continue` resolves through the previous
closeout's `Next Task`. That pointer should name the current ready Northstar
task or a planning route. Repair stale active surfaces before continuing.

## Closeout Rule

When a Northstar task closes:

1. update the task outcome, evidence, and status;
2. update its generation README and approved frontier;
3. update any governing spec and stale front-door pointer;
4. write one evidence log with validation actually run;
5. leave one explicit `Next Task` in the highest-authority active surface;
6. delete the completed lane's dispatch handoff under `docs/handoffs/`; Git and
   Northstar Queue retain that execution record.

A completed task must never remain in the approved frontier.

## Generation Rollover Rule

Generations are substantial sequencing eras, normally 20–50 meaningful tasks.
Rollover is a full closeout:

- every task is closed, superseded, or rehomed;
- front doors agree the generation is no longer active;
- open commitments have current homes;
- stale generation-specific specs are archived or removed;
- the expanded generation is compacted into a non-procedural roll-up under
  `docs/roadmaps/archive/` through the lifecycle preservation procedure.

Repair the current generation instead of opening a new one to escape cleanup.

## Intent Checkpoint Rule

When the next direction is materially ambiguous, stop and ask the operator.
Do not infer a generation, task, release, or breaking change.

## Scope Rule

Keep each task bounded to one honest owner and one reviewable outcome. Do not
bundle unrelated product work, release chores, and docs cleanup into a vague
polish lane.

## Next Task

No generation or ready task is active. Run Northstar Atlas with the operator
before opening `g10`. Effigy release and S3 retirement remain separately gated.
