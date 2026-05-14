# 724 - Shrink Runner State Rendering And Manifest Follow-Through

Roadmap: [`../009-state-command-thin-shell-follow-through.md`](../009-state-command-thin-shell-follow-through.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)
Contract: [`../../../contracts/027-state-domain-extraction-contract.md`](../../../contracts/027-state-domain-extraction-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Continue the `state_command` shrink after `723` by removing the next runner-local
pure-domain residue that does not need side effects.

## Scope

- reduce remaining runner-local rendering and decode helpers where ownership is
  now obvious
- keep file writes and execution adapters in the runner

## Acceptance

- `state_command.rs` shrinks again
- no command behavior drift

## Completed

- Moved runner-owned state text rendering out of `state_command.rs` into a
  dedicated `state_command_render` owner.
- Kept current state text output stable while reducing whole-file context load in
  the main state command owner.
- Left remaining state manifest-loading and side-effect work in the runner.

## Next Task

Execute `725` now.
