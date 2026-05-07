# 386 - Close Container Manager Facade Lane

Lane: [`038-plugin-ready-container-manager-facade-strict-lane.md`](../038-plugin-ready-container-manager-facade-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Close `g03.031` by recording the remaining compatibility boundary and adding
lightweight drift checks for runner-local backend branching.

## Scope

- update `012-container-manager-contract.md` with the shipped migration state
- update roadmap and spec front doors to mark `g03.031` complete
- add or document the runner drift checks for direct backend branching
- leave lower-level `effigy-containers` compatibility wrappers in place for
  follow-up cleanup
- do not start `g03.033` implementation in this card

## Exit Condition

This card is complete when the lane has no active ready card, the roadmap is
closed, and the next move is explicitly `g03.033` or a deliberate planning stop.

## Closeout

`g03.031` is closed.

`012-container-manager-contract.md` now records the shipped manager state, the
temporary lower-level compatibility wrappers, and the runner drift check.

No active ready card remains in lane `038`.

## Validation

- `rg "resolve_compose_backend|ComposeBackend" src/runner/exec_command src/runner/container_command crates/effigy-runtime/src/write.rs -n`
- `git diff --check`

## Next Task

Choose the next queued roadmap deliberately. The likely next roadmap is
`g03.033`.
