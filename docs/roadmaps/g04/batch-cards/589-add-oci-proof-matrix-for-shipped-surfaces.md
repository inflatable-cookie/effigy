# 589 - Add OCI Proof Matrix For Shipped Surfaces

Lane: [`060-oci-artifact-closeout-and-proof-matrix-strict-lane.md`](../060-oci-artifact-closeout-and-proof-matrix-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Prove the OCI surfaces Effigy already ships at the command level instead of
relying mostly on parser and crate-local coverage.

## Scope

- add focused proof coverage for:
  - `artifact inspect oci://...`
  - `artifact stage oci://...`
  - `artifact capture --push`
  - `bootstrap --db-seed ...=oci://...`
  - `container data seed --db-seed ...=oci://...`
  - `container data dump ...=oci://...`
  - `container data dump ...=oci://... --push`
- prefer fake-adapter or runner-bound command proofs over full live registry
  dependence
- record any missing proof seams explicitly in the lane if they cannot be
  closed inside this batch

## Non-Goals

- no auth/remediation text redesign here unless a proof cannot be written
  without it
- no ledger/operation-record expansion
- no guide rewrite beyond tiny test-driven contract nits

## Exit Condition

This card is complete when the shipped OCI command surfaces have focused proof
coverage and any remaining unproven seam is explicit enough to hand to the next
card.

## Proof Map

- `artifact inspect oci://...`
  - covered by `runner::artifact_command::tests::inspect_oci_uses_adapter_and_redacted_descriptor`
- `artifact stage oci://...`
  - covered by `runner::artifact_command::tests::stage_oci_uses_adapter_pull_files_and_stages_metadata`
- `artifact capture --push`
  - covered by `runner::artifact_command::tests::capture_push_uses_adapter_and_reports_digest`
- `bootstrap --db-seed ...=oci://...`
  - currently covered through the shared runner seed-staging seam used by bootstrap:
    `runner::db_seed::tests::stage_db_seed_files_accepts_oci_artifact_refs`
- `container data seed --db-seed ...=oci://...`
  - currently covered through the same shared runner seed-staging seam:
    `runner::db_seed::tests::stage_db_seed_files_accepts_oci_artifact_refs`
- `container data dump ...=oci://...`
  - covered by `runner::container_command::data::tests::run_container_data_dump_reports_planned_oci_artifact_capture`
- `container data dump ...=oci://... --push`
  - covered by `runner::container_command::data::tests::run_container_data_dump_reports_pushed_oci_artifact_capture`

## Remaining Seam

Bootstrap and `container data seed` do not yet have separate command-shell OCI
adapter injection points. The unique OCI behavior for both flows currently
funnels through the shared runner seed-staging path in `runner::db_seed`, so
the shipped behavior is proven there, but not yet with dedicated end-to-end
runner hooks of their own.

## Validation

- `cargo test -p effigy-artifacts`
- targeted `cargo test -p effigy ... -- --nocapture` for artifact, bootstrap,
  and container data OCI proofs
- `git diff --check`

## Next Task

Continue with
[`590-harden-oci-auth-and-push-failure-remediation.md`](./590-harden-oci-auth-and-push-failure-remediation.md).
