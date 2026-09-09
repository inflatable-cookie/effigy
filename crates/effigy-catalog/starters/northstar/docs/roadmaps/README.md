# Roadmaps

The generation README owns the roadmap and approved frontier. Each executable
planning unit is one top-level Northstar task.

## Generation model

- `g01` is the first generation.
- Put the generation runway and approved frontier in `g01/README.md` when the
  generation opens.
- Put each task at `g01/NNN-<slug>.md` and reference it as `g01.NNN`.
- Start from [`templates/task-template.md`](./templates/task-template.md).
- Do not create milestone wrappers, nested `batch-cards/`, or dual status
  authority.
- Keep using the current generation until a substantial sequencing era closes.

Use “Northstar task” for the planning unit, “queue task” for a control-plane
execution record, and “Effigy task” for a command selector.

## Current planning state

- Active generation: none.
- Approved frontier: none.

## Backlog layout

Keep exploratory, unscheduled planning outside the active generation. A backlog
note becomes executable only after promotion into a ready top-level task.

## Next Task

Define the first generation runway, create `g01/README.md`, then copy the task
template to `g01/001-<short-slug>.md` when the first outcome is ready.
