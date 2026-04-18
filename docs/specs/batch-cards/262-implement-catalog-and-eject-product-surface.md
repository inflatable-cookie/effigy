# 262 Implement Catalog And Eject Product Surface

Status: landed
Updated: 2026-04-17
Roadmap: `g02.011`
Spec: `docs/specs/011-service-catalog-and-compose-assembly-strict-lane.md`

## Objective

Land the operator-facing product surface for the shipped catalog foundation so
`g02.011` is no longer missing its visible CLI entrypoints.

## Scope

- add `effigy catalog list`
- add `effigy catalog extract <service>`
- add `effigy container eject`
- keep the runner-side work adapter-shaped over `effigy-catalog` and
  `effigy-containers`
- update command help and focused product tests

Primary write set:

- `crates/effigy-cli/**`
- bounded command handlers under `src/runner/**`
- any narrow `effigy-containers` helper/report surface needed for eject

## Acceptance

- `effigy catalog list` shows available catalog fragments with source-layer
  information
- `effigy catalog extract` writes a bundled fragment to the override location
  without inventing a new ownership model
- `effigy container eject` converts generated compose output into direct
  compose ownership through the product surface
- focused CLI and runner tests prove the commands, help text, and happy-path
  behavior

## Outcome

This batch is landed.

What shipped:

- `effigy catalog list` now exposes the layered catalog through the product
  surface.
- `effigy catalog extract <service>` now writes bundled fragments into an
  override directory from the product surface.
- `effigy container eject` now promotes generated catalog-backed compose into
  direct `compose_file` ownership through the product surface.
- help text, parser coverage, runner adapters, and focused tests all moved
  with the command surface.

## Continuation Envelope

If this card lands cleanly, continue to card `263`. Do not skip straight to
`g02.012`; `g02.011` still needs its real-project proof first.

## Next Task

Card `263` is ready next: prove the full generated-compose loop in one real
project.
