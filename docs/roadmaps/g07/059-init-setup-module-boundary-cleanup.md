# g07.059 - Init Setup Module Boundary Cleanup

Status: Complete
Depends on: `g07.058`

## Goal

Keep `effigy init` as one coherent setup front door while splitting the setup
inventory and wizard internals into obvious ownership units.

## Evidence

The audit found that `crates/effigy-builtin/src/init/inventory.rs` now owns too
many jobs:

- setup job model
- repo context detection
- action rendering
- command construction
- action execution
- tests

`crates/effigy-builtin/src/init/wizard.rs` also carries prompt flow and test
port plumbing.

## Scope

- split init setup code into local submodules
- preserve current CLI behavior and JSON contracts
- keep TTY prompts simple and human-readable
- keep non-TTY behavior deterministic
- keep setup jobs backed by real Effigy command surfaces
- introduce compact test support for fake builtin ports if useful

## Guardrails

- no second top-level onboarding command
- no new setup jobs unless an existing product surface can execute them
- no release, deploy, state, or distribution mutation from init
- no generic interactive framework
- no behavior change hidden inside a module move

## Suggested Implementation Shape

Use local init modules such as:

- `setup/model.rs`
- `setup/detect.rs`
- `setup/render.rs`
- `setup/execute.rs`
- `setup/commands.rs`
- `setup/test_support.rs`

The exact names can change, but the separation should be visible in the file
tree.

## Acceptance Criteria

- `inventory.rs` no longer mixes model, detection, rendering, command building,
  and execution in one file
- wizard tests read as prompt-flow tests rather than port-boilerplate tests
- existing init checklist/action JSON tests pass
- help/docs remain accurate for TTY and non-TTY usage

## Next Task

No active ready card.
