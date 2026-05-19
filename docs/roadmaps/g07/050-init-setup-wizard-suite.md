# g07.050 - Init Setup Wizard Suite

Status: Complete
Depends on: `g07.049`

## Goal

Turn `effigy init` into one coherent setup front door for humans and agents:

- plain `effigy init` in a TTY becomes an interactive yes/no setup wizard
- non-interactive `effigy init` can cover the same setup surface deterministically
- `effigy init --checklist --json` exposes the same candidate actions as a
  machine-readable plan for agents

## Why This Exists

The current idempotent initializer is useful, but narrow. It handles baseline
repo files and agent surfaces, yet a real first-run setup session often also
needs graph indexing, secrets bootstrap, task migration, local runtime checks,
bundle sync, and a first validation pass.

Those setup jobs already exist across Effigy's command surface. The missing
piece is one orchestrator that:

- detects what is relevant in the current repo
- presents only safe, contextual setup work
- preserves non-interactive determinism
- gives agents a checklist instead of forcing prompt-driven TTY flows

## Scope

- define one shared init setup-job inventory and detection model
- add a checklist contract for non-interactive / agent consumption
- add a TTY-only interactive prompt flow for plain `effigy init`
- let non-interactive init execute the same setup jobs through explicit flags or
  deterministic defaults
- cover existing setup-relevant surfaces only; do not invent fake setup work
- document the wizard, checklist, and safety rules clearly

## Non-Goals

- no generic interactive framework for unrelated commands
- no release mutation flow inside init
- no default deploy/state/distribution mutation from init
- no hidden background daemons or long-running supervisor process
- no product claim that init can fully configure secrets or containers without
  the repo declaring those surfaces

## Ordered Follow-Up Lanes

1. [`051-init-context-inventory-and-checklist-contract.md`](./051-init-context-inventory-and-checklist-contract.md)
2. [`052-tty-init-wizard-engine-and-prompt-flow.md`](./052-tty-init-wizard-engine-and-prompt-flow.md)
3. [`053-setup-job-adapters-and-mutation-boundaries.md`](./053-setup-job-adapters-and-mutation-boundaries.md)
4. [`054-noninteractive-init-action-execution-and-migration-paths.md`](./054-noninteractive-init-action-execution-and-migration-paths.md)
5. [`055-init-wizard-proof-docs-and-closeout.md`](./055-init-wizard-proof-docs-and-closeout.md)

## Acceptance Criteria

- one setup-job inventory drives both checklist mode and TTY mode
- plain TTY `effigy init` can walk a repo through relevant setup jobs with
  bounded yes/no prompts
- `effigy init --checklist --json` returns a complete actionable plan for
  agents without writing
- non-interactive execution can apply the same setup actions deterministically
- setup jobs remain bounded by existing product surfaces and safety rules
- docs/help/contracts make the new split obvious

## Batch Cards

- [`1000-open-init-setup-wizard-lane.md`](./batch-cards/1000-open-init-setup-wizard-lane.md)
- [`1001-define-init-context-and-checklist-contract.md`](./batch-cards/1001-define-init-context-and-checklist-contract.md)
- [`1002-build-tty-init-wizard-engine.md`](./batch-cards/1002-build-tty-init-wizard-engine.md)
- [`1003-wire-setup-job-adapters-and-safety-bounds.md`](./batch-cards/1003-wire-setup-job-adapters-and-safety-bounds.md)
- [`1004-add-noninteractive-action-execution-and-migration-flows.md`](./batch-cards/1004-add-noninteractive-action-execution-and-migration-flows.md)
- [`1005-close-init-setup-wizard-lane.md`](./batch-cards/1005-close-init-setup-wizard-lane.md)

## Next Task

No active ready card.
