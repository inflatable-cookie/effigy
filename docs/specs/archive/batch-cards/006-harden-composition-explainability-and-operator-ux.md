# 006 Harden Composition Explainability And Operator UX

Status: complete
Updated: 2026-04-11
Roadmap: `g02.002`
Spec: `docs/specs/archive/002-manifest-composition-and-override-strict-lane.md`

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

## Outcome

Completed in the `g02.002` follow-up implementation batch:

- conflict diagnostics now name both source fragments and include explicit
  override hints
- `effigy config --inspect` groups effective sources by fragment in text mode
- `effigy config --inspect --path <dotted.path>` provides a bounded source and
  override-history query surface in both text and JSON mode
- the narrower inspection surface stayed bounded instead of turning into a
  config editor

## Next Task

Activate `g02.003` planning next and use the now-real composition surface as a
dependency the demo-harness lane can rely on.
