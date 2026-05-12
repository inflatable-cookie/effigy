# 027 Runtime Session Context And Runtime Ownership Hardening Strict Lane

Status: complete
Updated: 2026-05-02
Roadmap: `g03.013`

## Context

Effigy's runtime and interactive lifecycle behavior is now feature-rich, but
 the control surface is still too implicit.

Recent fixes around bootstrap lease suppression and stop-on-exit handoff
 worked, but they worked by threading internal control through ambient env
 flags and caller-local branching. That is good enough to ship, not good
 enough for a `v1.0` runtime core.

The next honest seam is not more container feature work. It is making runtime
 ownership, lease policy, and handoff semantics explicit and typed across the
 main runner entrypoints.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/contracts/005-container-runtime-contract.md`
- `docs/contracts/009-execution-surface-convergence.md`
- `docs/roadmaps/g03/013-runtime-session-context-and-runtime-ownership-hardening.md`
- `docs/roadmaps/g03/README.md`

## Lane Focus

This lane owns:

- one typed activation/session context for runtime ownership and lease policy
- removal of env-driven internal control where it currently steers runtime
  behavior
- shared ownership semantics across bootstrap, workspace, seeded shells, and
  non-shell exec activation
- parity proof for the typed ownership model on the main runtime seams

This lane does not own:

- the typed container assembly model
- broad workspace/runtime module splitting beyond what this first typed
  context needs
- new container/runtime features
- provider/deployment work

## Current Posture

`complete`

The correct implementation order is:

1. define one typed runtime/session policy model for activation, ownership,
   lease, and handoff behavior
2. thread it through bootstrap dispatch, workspace handoff, seeded shells, and
   non-shell exec activation
3. remove the corresponding internal env-flag control where typed policy now
   owns the seam
4. add parity tests for bootstrap no-lease, bootstrap stop-on-exit, direct
   workspace ownership, seeded shell ownership, and non-shell exec activation
5. decide whether a second bounded slice is needed before handing off to the
   container assembly model lane

## Integration Constraint

- keep the first batch narrow and typed-context-first
- prefer explicit ownership and lease policy objects over another helper layer
  around ambient env flags
- do not start container assembly refactors in this lane
- if a caller cannot move fully in the first batch, leave the remaining seam
  explicit rather than hiding it behind compatibility glue

## Continuation Chain

1. `334` — implement the typed activation/session context foundation
2. `335` — decide whether another bounded runtime-ownership slice is needed

## Exit Condition

This strict lane is complete when:

- internal runtime ownership and lease behavior no longer depend on ambient
  env flags as the governing mechanism
- the main runtime entrypoints consume one explicit typed policy/context model
- the typed path is proven for bootstrap, workspace, seeded-shell, and exec
  lifecycle seams

## Next Task

Promote `g03.014`.
