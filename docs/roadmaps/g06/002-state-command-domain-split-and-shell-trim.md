# g06.002 - State Command Domain Split And Shell Trim

Status: Active
Depends on: `g06.001`

## Goal

Finish the state-domain extraction enough that `state_command.rs` becomes a
thin runner adapter instead of a mixed domain/orchestration/rendering module.

## Evidence

- `src/runner/state_command.rs` remained a warning-level god file after `g05`
- `g05.024` moved some state models and helpers into `effigy-state`, but the
  runner still owns too much planning, path handling, report shaping, hook
  orchestration, and rendering glue
- state behavior is release-sensitive, so size here directly raises regression
  cost

## Scope

- classify `state_command.rs` by durable responsibility:
  plan/build, apply orchestration, capture orchestration, report IO, rendering,
  and manifest adaptation
- move pure state-domain helpers into `effigy-state`
- move reusable report/path/context shaping into domain-owned modules
- reduce runner-owned logic to CLI parsing, option adaptation, and final
  rendering dispatch
- add focused tests around any moved state contract surface

## Out Of Scope

- no redesign of the state model itself
- no stack feature expansion
- no new SQL/apply behavior
- no rewriting state rendering unless it falls out of ownership cleanup

## Guardrails For A Cheaper Model

- split by state concepts, not arbitrary line count
- do not move CLI argument validation into domain crates unless it is truly
  shared
- keep text/JSON output stable unless a contract explicitly changes
- preserve existing hook execution order and failure behavior exactly
- do not intermingle capture/apply/report refactors in one unbounded batch

## Suggested Implementation Steps

1. Build a responsibility map of `state_command.rs`.
2. Extract pure path/report/context helpers first.
3. Extract plan/build logic that has no runner-specific output concerns.
4. Extract apply/capture domain pieces only where ownership is clear.
5. Leave final CLI rendering in runner unless a shared renderer already exists.
6. Re-run focused state tests after each batch, then one wider pass.

## Acceptance Criteria

- `state_command.rs` is materially smaller
- `effigy-state` owns more durable state-domain behavior
- state contracts and hook behavior stay stable
- retained runner-owned state logic is explicitly justified

## Validation

Run focused validation at minimum:

```bash
cargo test state_command
cargo test -p effigy-state
effigy scan god-files --json
```

## Next Task

First slice landed. Continue with the next durable state-domain extraction only
if it removes real runner-private ownership; otherwise move to `g06.003`.
