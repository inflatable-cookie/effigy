# g07.067 - Script Command Boundary Reduction

Status: Planned
Depends on: `g07.066`

## Goal

Reduce `src/runner/script_command/mod.rs` into owned pieces without turning the
script runner into a broad architecture rewrite.

## Evidence

The current god-file scan still reports:

- `src/runner/script_command/mod.rs`

The file is large enough to obscure which logic is:

- domain planning
- script catalog resolution
- file staging and temp-path management
- process spawn glue
- output shaping and failure handling

## Scope

- extract pure or mostly-pure helpers first
- keep shell/process/TTY glue in the runner owner where it belongs
- preserve behavior and error text unless a focused test proves a defect
- add targeted tests around any moved planner or validator path

## Guardrails

- no runner-wide rewrite
- no cross-crate movement unless a natural owner already exists
- no behavior changes hidden in a file split

## Suggested Implementation Shape

- split into local modules such as:
  - planning
  - resolution
  - staging
  - execution glue
  - tests/support

## Acceptance Criteria

- `script_command/mod.rs` is no longer a god-file finding, or the remaining
  size is explicitly justified
- touched command behavior stays stable under focused tests
- the module split makes it obvious where future script-command work belongs

## Next Task

After this lands, proceed to [`068-high-duplicate-help-fragment-reduction.md`](./068-high-duplicate-help-fragment-reduction.md).
