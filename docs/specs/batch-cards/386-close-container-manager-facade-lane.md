# 386 - Close Container Manager Facade Lane

Lane: [`038-plugin-ready-container-manager-facade-strict-lane.md`](../038-plugin-ready-container-manager-facade-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

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

## Next Task

Close `g03.031`, then choose the next queued roadmap deliberately.
