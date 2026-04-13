# Demo Release Execution Readiness Decision

Date: 2026-04-13
Roadmap: `g02.003`
Card: [`084-decide-demo-release-execution-readiness.md`](../../specs/batch-cards/084-decide-demo-release-execution-readiness.md)

## Summary

Decided that Effigy can move from release prep into actual release-execution
work next, but only after the working tree is clean and a human explicitly asks
to execute the release protocol.

## Vision Target Delta

- Tags: `DEMO`, `RELEASE`, `OPERATE`
- Moved: `release-prep checkpoint complete but execution boundary still
  undecided` -> `release execution justified next with explicit safety
  preconditions`
- Remaining: run the release protocol only when the tree is clean and a human
  explicitly requests release execution

## Decision

- no more demo-surface implementation or validation work is required before the
  next release boundary
- actual release-execution work is the correct next batch
- release execution is still gated by protocol, not by momentum

## Preconditions

- clean working tree
- explicit human release instruction
- release gates green at time of execution

## Evidence

- `qa:docs`: pass
- `qa:ci`: pass
- `release simulate`: pass
- release dry-run suggests:
  - version `0.2.13`
  - tag `v0.2.13`
  - ready to prepare and execute: `yes`

## Outcome

Opened ready card
[`085-execute-demo-release-protocol.md`](../../specs/batch-cards/085-execute-demo-release-protocol.md).

## Next Task

- Execute `085-execute-demo-release-protocol.md` once the working tree is clean
  and a human explicitly asks to proceed
