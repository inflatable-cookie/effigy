# Consumer-Driven Distribution Gap Widening

Date: 2026-04-15
Roadmap: `g02.005`
Spec: `docs/specs/005-optional-distribution-surface-strict-lane.md`
Batch Card: `docs/roadmaps/g02/batch-cards/104-implement-consumer-driven-distribution-gap-widening.md`

## Summary

Widened the optional distribution surface only where the `pilot-repo-e`
consumer proof exposed concrete remaining Effigy-shaped assumptions.

## Shipped

- added optional `[distribution.publish]` booleans:
  - `verify-tag-install`
  - `verify-binary-json-tasks`
- made `distribution first-publish` skip the `release verify-install` probe
  when `verify-tag-install = false`
- made `distribution first-publish` skip binary `--json tasks` probes when
  `verify-binary-json-tasks = false`
- made `distribution validate-artifacts` derive expected logs from those same
  publish verification toggles
- made `distribution validate-metadata` stop inheriting Effigy's
  workflow/docs/package-quality gate when a repo has adopted `[distribution]`
  without explicit metadata policy
- widened metadata parsing to accept `[workspace.package]` fallback when a root
  `[package]` table is absent
- updated the distribution guide and manifest schema output to reflect the
  widened boundary

## Consumer Proof Rerun

Reran the `pilot-repo-e` proof after the widening with:

- `distribution validate-metadata --repo ~/Dev/projects/convergence --tag v0.1.0`
- local install logs captured under `/tmp/effigy-convergence-distribution-proof-v2`
- `distribution validate-artifacts --repo ~/Dev/projects/convergence --artifacts-dir /tmp/effigy-convergence-distribution-proof-v2`
- `distribution generate-closeout --repo ~/Dev/projects/convergence --tag v0.1.0 --artifacts-dir /tmp/effigy-convergence-distribution-proof-v2 --output /tmp/effigy-convergence-distribution-closeout-v2.md`

The rerun passed cleanly with `pilot-repo-e` using:

- `verify-tag-install = false`
- `verify-binary-json-tasks = false`

## Outcome

The named consumer-proof gaps from `104` are now removed or moved behind
explicit manifest policy.

The remaining open question is narrower than before:

- full `first-publish` orchestration still assumes a published Cargo install
  path

That is now an explicit product limit rather than a hidden Effigy-shaped
assumption leaking through metadata validation or artifact expectations.

## Validation

- `cargo test validate_artifacts_respects_optional_tag_and_json_checks --lib`
- `cargo test validate_metadata_skips_effigy_defaults_when_manifest_is_adopted --lib`
- `cargo test cli_distribution_validate_metadata_skips_effigy_defaults_for_manifest_adopters --test cli_output_tests`
- `cargo test cli_distribution_validate_artifacts_respects_publish_optional_checks --test cli_output_tests`
- `cargo run --manifest-path /Users/tom/Dev/projects/effigy/Cargo.toml --bin effigy -- distribution validate-metadata --repo /Users/tom/Dev/projects/pilot-repo-e --tag v0.1.0`
- `cargo run --manifest-path /Users/tom/Dev/projects/effigy/Cargo.toml --bin effigy -- distribution validate-artifacts --repo /Users/tom/Dev/projects/pilot-repo-e --artifacts-dir /tmp/effigy-convergence-distribution-proof-v2`
- `cargo run --manifest-path /Users/tom/Dev/projects/effigy/Cargo.toml --bin effigy -- distribution generate-closeout --repo /Users/tom/Dev/projects/pilot-repo-e --tag v0.1.0 --artifacts-dir /tmp/effigy-convergence-distribution-proof-v2 --output /tmp/effigy-convergence-distribution-closeout-v2.md`

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `RELEASE`
- Moved: the `pilot-repo-e` consumer gaps from metadata validation and
  artifact/publish probe expectations are now resolved through the optional
  manifest contract instead of hidden Effigy defaults
- Remaining open: whether the full `first-publish` orchestration path still
  needs one more proof on a published consumer before the lane can pause

## Next Task

Execute `docs/roadmaps/g02/batch-cards/105-decide-post-consumer-gap-widening-boundary.md`
to decide whether the widened optional distribution surface can now pause on a
trustworthy boundary.
