# g05.024 - State Domain Extraction Follow-Through

Status: Planned
Depends on: `g05.009`

## Goal

Finish the state-domain extraction so the runner state command is a thin CLI
adapter and `effigy-state` owns durable state concepts in smaller modules.

## Evidence

- `effigy scan god-files --json` flags `src/runner/state_command.rs` at 2237
  lines
- the same scan flags `crates/effigy-state/src/lib.rs` at 1666 lines
- `docs/contracts/027-state-domain-extraction-contract.md` already defines the
  intended boundary
- previous g05 work moved some state logic, but the runner and domain crate are
  still both oversized

## Scope

- re-read `docs/contracts/027-state-domain-extraction-contract.md` before
  editing
- classify `state_command.rs` code into CLI parsing/rendering, path/report
  helpers, history, planning, apply/capture, and test support
- move pure domain/report/path/history/planning behavior into focused
  `effigy-state` modules
- keep side effects, command invocation, and CLI rendering at the runner edge
- split `effigy-state/src/lib.rs` into named modules where ownership is clear
- preserve JSON/text output compatibility

## Out Of Scope

- no new state-stack feature
- no deploy transaction redesign
- no database/media/object-store widening unless already covered by state
  contract
- no CLI flag changes

## Guardrails For A Cheaper Model

- move one cohesive state concept at a time
- add or preserve tests before changing behavior
- do not change schema names or JSON field names
- do not make `effigy-state` depend on runner types
- if a function performs IO plus rendering plus domain calculation, split the
  pure calculation first and leave IO in runner until a later safe slice
- stop and document if behavior is unclear instead of “cleaning up” semantics

## Suggested Implementation Steps

1. Produce a brief ownership map for `state_command.rs` and `effigy-state`.
2. Extract one pure module at a time inside `effigy-state`.
3. Switch runner callers to the new domain API.
4. Keep output rendering snapshots or assertions stable.
5. Run focused state tests after each meaningful batch.
6. Rerun god-file scan and record remaining line counts.

## Acceptance Criteria

- `state_command.rs` is meaningfully smaller and mostly dispatch/rendering
- `effigy-state/src/lib.rs` is split into durable modules
- state JSON/text output remains compatible
- state tests pass
- any retained large module has an explicit reason

## Validation

Minimum focused validation:

```bash
cargo test state
effigy scan god-files --json
```

Run full `cargo test` after the extraction batch is complete.

## Next Task

After state extraction, move to `g05.025` for low-risk duplicate-block cleanup.
