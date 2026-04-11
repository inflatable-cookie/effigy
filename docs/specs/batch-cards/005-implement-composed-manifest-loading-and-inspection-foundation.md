# 005 Implement Composed-Manifest Loading And Inspection Foundation

Status: ready
Updated: 2026-04-11
Roadmap: `g02.002`
Spec: `docs/specs/002-manifest-composition-and-override-strict-lane.md`

## Objective

Ship the first narrow composition implementation slice:

- composed-manifest loading
- conflict/override enforcement
- minimal effective-manifest inspection
- one cross-feature proof slice

## In Scope

- load `effigy.toml` plus recursively included partial fragments
- enforce path-scoped override and conflict rules during load
- route composition failures through normal manifest parse/doctor paths
- add one `effigy config` inspection surface for include graph, evaluation
  order, effective sources, and overridden paths
- add fixture coverage proving a split such as `tasks + docs_policy` or
  `tasks + release`

## Out Of Scope

- init/migrate support for composition
- broad real-repo fragment migrations
- demo-harness design or implementation
- richer config-editing or visualization surfaces

## Acceptance Criteria

- Effigy can load and compose included manifest fragments
- invalid composition fails clearly and deterministically
- operators can inspect the effective composition result through one native
  config surface
- one non-task-only proof slice passes

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa:docs`

## Stop Conditions

- composition touches more runtime surfaces than one bounded batch can support
- the inspection surface proves materially broader than the minimum contract

## Next Task

Implement this batch, then leave the next move explicit as either a follow-up
composition hardening batch or activation of `g02.003` planning on top of the
new foundation.
