# g09.001 Command-Surface Compaction Preview

Status: Complete
Created: 2026-09-02
Closed: 2026-09-02 — card `1109` shipped the additive preview; strict spec
`116` archived; direct-route removal remains gated on the `v1.0`
consumer-evidence checkpoint
Spec: [`116`](../../specs/116-command-surface-compaction-preview-strict-lane.md)
Architecture: [`026`](../../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../../contracts/043-feature-placement-and-surface-migration-contract.md)

## Purpose

Turn the shipped help taxonomy into one coherent executable command shape while
preserving current automation through an explicit pre-`v1.0` migration window.

## Sequence

1. [`1109`](./batch-cards/1109-add-executable-command-namespaces.md) — Ready:
   add the five canonical namespaces, retain direct routes with structured
   diagnostics, update discovery/completion/docs, and prove routing and output
   parity.

The implementation is one serial lane because parser, dispatch, help,
completion, envelope, and managed-skill surfaces share the same command-route
authority. Direct-route removal is not part of this milestone.

## Acceptance

- `local`, `repo`, `deliver`, `extend`, and `admin` route to existing typed
  child commands
- daily-spine commands, global flags, task selectors, and slash selectors stay
  stable
- retained direct built-ins warn without changing stdout, exits, or inner JSON
  payloads
- grouped routes are primary in help, completions, current docs, and the
  managed skill
- shadowed built-ins have an explicit grouped escape while direct deferral stays
  unchanged
- no displaced route is removed before the explicit `v1.0` gate

## Non-Goals

- release execution or a version decision
- direct-route removal
- automatic edits across consumer repositories
- S3 extraction or extension transport
- command implementation duplication
- changes to task or catalog alias grammar

## Next Task

The additive preview is complete. The next checkpoint is the future `v1.0`
consumer-evidence gate (refreshed consumer inventory plus explicit release
authority); no removal card exists yet.
