# 096 Implement Effigy Wrapper Convergence Boundary

Status: archived
Updated: 2026-04-15
Roadmap: `g02.004`
Spec: `docs/specs/archive/004-rust-native-scripting-strict-lane.md`

## Objective

Use one final Effigy-only Rhai batch to converge the remaining compatibility
wrapper surface, so the repo cleanly distinguishes:

- wrappers that should become minimal Rhai launchers
- scripts that remain honest permanent shell boundaries

## In Scope

- review the remaining Effigy script surface after the release-wrapper batch
- migrate any remaining low-risk compatibility wrappers that are still only
  shell glue
- leave intentional side-effecting or external-binary shell surfaces explicit
  and documented as permanent boundaries
- tighten docs so the remaining shell scripts are described as deliberate, not
  just “not migrated yet”

## Out Of Scope

- reopening the external pilot
- Keepsake work
- Jetstream work
- replacing release mutation/backstop scripts if they still need broader
  host-side helpers

## Acceptance Criteria

- the remaining Effigy shell surface is clearly split into:
  - Rhai-backed compatibility launchers
  - honest permanent shell boundaries
- docs no longer imply temporary status for scripts that are intentionally
  staying shell-backed
- the batch leaves one new explicit ready card

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Decide whether the Effigy Rhai dogfooding lane is complete enough to pause on a
clean internal boundary until the first external pilot is safe again.
