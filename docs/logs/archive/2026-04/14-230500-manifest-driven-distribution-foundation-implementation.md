# Manifest-Driven Distribution Foundation Implementation

Date: 2026-04-14
Roadmap: `g02.005`
Spec: `docs/specs/005-optional-distribution-surface-strict-lane.md`
Card: `099-implement-manifest-driven-distribution-contract-foundation.md`

## Summary

Implemented the first optional `[distribution]` manifest contract and wired it
through the first genuinely reusable policy seams in the distribution command
family.

## What Shipped

- added optional manifest config:
  - `[distribution.package]`
  - `[distribution.preflight]`
  - `[distribution.metadata]`
- made `effigy distribution validate-metadata` read manifest-driven:
  - package name expectation
  - required docs
  - required files
- made `effigy distribution preflight` read manifest-driven:
  - docs task
  - smoke task
- made config/schema and doctor manifest validation understand the new
  `distribution` section
- added a front-door guide update showing the minimal manifest contract

## Deliberate Boundary

This batch did not widen into configurable workflow checks or full
`first-publish`/closeout policy. That is the next decision boundary, not
something to infer into this implementation slice.

## Validation

- `cargo test cli_distribution_preflight_uses_manifest_distribution_preflight_tasks --test cli_output_tests -- --nocapture`
- `cargo test cli_distribution_validate_metadata_uses_manifest_distribution_requirements --test cli_output_tests -- --nocapture`
- `cargo test validate_manifest_schema_accepts_docs_policy_bootstrap_distribution_and_release_sections -- --nocapture`
- `cargo test run_manifest_task_builtin_config_schema_prints_canonical_template -- --nocapture`
- `cargo test run_manifest_task_builtin_config_schema_minimal_prints_starter_template -- --nocapture`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved from: native distribution commands that were still mostly
  Effigy-self-hosting in policy
- moved to: an optional manifest-driven distribution foundation with the first
  real cross-repo policy hooks
- remains open:
  - broader manifest-driven coverage for more distribution commands
  - consumer-proof validation of the optional distribution surface
  - eventual workflow-bound glibc guard cutover when workflow edits are in scope

## Next Task

Execute `100-decide-post-distribution-foundation-slice.md` to choose the next
bounded move: widen internal manifest-driven distribution coverage, or prove
the optional surface in one concrete consumer repo.
