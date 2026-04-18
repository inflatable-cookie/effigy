# 258 Implement Container Command Directory Split

Status: landed
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Split `src/runner/container_command.rs` into a module directory so dispatch,
lifecycle orchestration, attached-session handling, signal plumbing, and raw
process helpers stop sharing one runner file.

## Context

`effigy-containers` already owns the real domain surface. The remaining
runner work is mostly shell, but it is still bundled tightly enough to make
ownership and follow-up edits noisy.

This is a root-shell cleanup card, not a crate extraction card.

## In Scope

- Convert `src/runner/container_command.rs` into
  `src/runner/container_command/`.
- Split the current file into local modules such as:
  - dispatch
  - lifecycle
  - session
  - signals
- Keep current output and container behavior unchanged.

## Out Of Scope

- New container features.
- Integration of the broader container roadmap items that wait on `g02.010`.
- Changes to `effigy-containers` unless a tiny adapter helper is needed.

## Acceptance Criteria

- `src/runner/container_command.rs` is replaced by a smaller module
  directory.
- Session, signal, and lifecycle logic no longer sit in one file.
- No user-facing behavior changes.
- Standard validation round passes for the batch.

## Next Task

Card `259` — finish the `/src` cleanup chain by shrinking the runner test
prelude surface.
