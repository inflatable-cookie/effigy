# 030 Container And Runtime Error Taxonomy And Diagnostics Strict Lane

Status: complete
Updated: 2026-05-02
Roadmap: `g03.016`

## Context

`g03.015` closed strongly enough to stop treating workspace/runtime module
splitting as the main hardening seam.

The next honest weakness is failure shape:

- runtime and container failures still collapse too often into generic
  `task_invocation` strings
- category boundaries are still too weak for stable contract tests
- diagnostics still hide the real subsystem seams that should harden before
  `v1.0`

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/contracts/005-container-runtime-contract.md`
- `docs/roadmaps/g03/016-container-and-runtime-error-taxonomy-and-diagnostics.md`
- `docs/roadmaps/g03/README.md`

## Lane Focus

This lane owns:

- typed error families for the runtime/container core
- sharper category boundaries for activation, handoff, lease, and generated
  container policy failures
- targeted category-level regression coverage for brittle runtime seams

This lane does not own:

- new runtime/container features
- broad CLI wording polish
- TUI redesign
- architecture-map repair beyond what error ownership needs

## Current Posture

`strict-active`

## Continuation Chain

1. `344` — implement typed runtime/container error foundation
2. `345` — decide whether another bounded error-taxonomy slice is still needed
3. `346` — implement typed container surface and policy translation errors
4. `347` — decide whether the lane can hand off after the container slice
5. `348` — implement typed workspace handoff and lease error translation
6. `349` — decide whether the lane can hand off after the handoff/lease slice
7. `350` — implement typed gateway reconciliation and route translation errors
8. `351` — decide whether the lane can hand off after the gateway slice
9. `352` — implement typed gateway loopback and runtime-target translation errors
10. `353` — decide whether the lane can finally hand off after the gateway closeout slice
11. `354` — implement typed gateway runtime-row and port-binding translation errors
12. `355` — decide whether `g03.016` can close

## Exit Condition

This strict lane is complete when:

- the core runtime/container path has typed error families instead of
  string-first failure handling
- generic `task_invocation` strings are no longer the dominant failure path
  for these seams
- the main brittle runtime/container failures have category-level tests

## Next Task

Closed. Promote `g03.017`.
