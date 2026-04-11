# 006 Harden Composition Explainability And Operator UX

Status: ready
Updated: 2026-04-11
Roadmap: `g02.002`
Spec: `docs/specs/002-manifest-composition-and-override-strict-lane.md`

## Objective

Make the first composition surface easier to trust in day-to-day use by
improving explainability, diagnostics, and focused inspection.

## In Scope

- tighten conflict diagnostics so they name the path and both source fragments
- improve `effigy config --inspect` rendering around effective sources and
  overridden paths
- decide and, if justified, add one narrower inspection/query surface under
  `effigy config` for source/path lookup
- prove the improved explainability in text and JSON output contracts

## Out Of Scope

- init or migrate support for composed manifests
- broad repo fragment migrations
- demo-harness design or implementation
- rich config editing or visual diff tooling

## Acceptance Criteria

- composition conflicts are easier to act on without reading the loader code
- operators can identify where an effective value came from without scanning the
  full rendered manifest by hand
- the inspection surface remains bounded and consistent across text and JSON

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa:docs`

## Stop Conditions

- the narrower inspection/query surface starts turning into a config editor
- explainability needs imply a much broader config-debugging tool than this
  bounded batch can honestly support

## Next Task

Implement this batch, then leave the next move explicit as either one final
composition polish/proof wave or activation of `g02.003` planning on top of a
more legible composition surface.
