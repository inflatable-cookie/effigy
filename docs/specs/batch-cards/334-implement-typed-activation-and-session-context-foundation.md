# 334 Implement Typed Activation And Session Context Foundation

Status: complete
Updated: 2026-05-02
Roadmap: `g03.013`
Spec: `docs/specs/027-runtime-session-context-and-runtime-ownership-hardening-strict-lane.md`

## Objective

Introduce one typed activation/session context for runtime ownership and lease
 policy, then move the main runtime entrypoints onto that path.

## In Scope

- define shared internal types for:
  - activation purpose
  - interactive versus non-interactive activation
  - ownership mode
  - lease policy
  - handoff policy
- thread the typed context through the first high-value runtime seams:
  - bootstrap task dispatch
  - bootstrap start handoff
  - public workspace session entry
  - seeded task shells where they overlap the same ownership model
  - non-shell exec activation
- remove the corresponding internal env-flag control where the typed path now
  owns the behavior
- add targeted parity tests for:
  - bootstrap setup phases that must not refresh the host-container lease
  - bootstrap shell handoff that must stop the runtime on exit
  - direct workspace ownership versus adopted-runtime preservation
  - seeded-shell ownership parity
  - explicit exec activation using the same typed ownership/lease contract

## Out Of Scope

- container assembly or compose-model refactors
- broad workspace/runtime file splitting
- new user-facing runtime features
- large docs or architecture cleanup beyond the strict-lane surfaces

## Acceptance Criteria

- the first runtime entrypoints no longer need ambient env flags as the main
  control path for ownership and lease behavior
- bootstrap, workspace, seeded-shell, and non-shell exec activation all
  consume one typed policy/context model
- the typed context is the primary owner of stop-on-exit and lease-refresh
  semantics on those paths
- focused tests prove the typed path on the main lifecycle seams

## Validation

- targeted runtime ownership and lease tests
- targeted bootstrap handoff tests
- targeted workspace/session ownership tests
- targeted non-shell exec activation tests
- targeted DecodeLabs deferred-container proof
- `./target/debug/effigy docs check-paths docs/specs/027-runtime-session-context-and-runtime-ownership-hardening-strict-lane.md docs/specs/batch-cards/334-implement-typed-activation-and-session-context-foundation.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/013-runtime-session-context-and-runtime-ownership-hardening.md`

## Next Task

Execute `335`.
