# 098 Decide Post-Native-Distribution Rhai Boundary

Status: complete
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/archive/004-rust-native-scripting-strict-lane.md`

## Objective

Decide whether the Rhai lane should now pause cleanly on the shipped Effigy
dogfooding boundary after native distribution cutover, or whether one more
internal batch is still justified before waiting on an external pilot repo.

## In Scope

- assess whether native Effigy commands now cover the meaningful internal
  release/distribution scripting surface
- record which shell boundaries remain intentional and why
- decide whether `scripts/check-linux-glibc-floor.sh` should stay deferred
  behind the workflow-approval boundary or whether a dedicated cutover card is
  justified next
- decide whether the lane should pause on the current Effigy-only proof
  boundary

## Out Of Scope

- touching `.github/workflows/` without explicit human approval
- reopening Keepsake while its repo boundary is unsafe
- touching Jetstream
- speculative new Rhai APIs without a concrete Effigy proving target

## Acceptance Criteria

- the lane has an explicit decision on pause vs one-more-internal-batch after
  native distribution cutover
- the remaining intentional shell boundaries are recorded honestly
- one clear next card exists after the decision

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Decision

The Rhai lane should pause cleanly here.

Effigy now has strong internal Rhai/native-distribution proof. The next
problem is no longer “one more internal scripting batch”; it is turning the
distribution surface into an optional cross-repo feature instead of an
Effigy-self-hosting one.

## Next Task

Open a new optional-distribution lane and implement the minimal
manifest-driven distribution contract foundation.
