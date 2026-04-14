# Distribution Policy Widening For Publish And Closeout Implementation

Date: 2026-04-14
Roadmap: `g02.005`
Spec: `docs/specs/005-optional-distribution-surface-strict-lane.md`
Batch Card: `docs/specs/batch-cards/101-implement-distribution-policy-widening-for-publish-and-closeout.md`

## Summary

Widened the optional `[distribution]` manifest contract so publish, summary,
and closeout commands now consume more repo-owned policy instead of hardcoding
Effigy-shaped defaults.

## Shipped

- added optional `[distribution.publish]`
  - `binary-name`
  - `registry-label`
- added optional `[distribution.closeout]`
  - `owner`
  - `related`
  - `next-step`
- made `distribution first-publish` use manifest-driven package/binary/registry
  identity
- made `distribution validate-artifacts` derive the non-Homebrew log
  expectations from the configured registry label
- made `distribution write-summary` record package, binary, and registry
  identity in the summary contract
- made `distribution generate-closeout` honor manifest-driven closeout defaults
  and render generic closeout wording instead of Effigy-specific roadmap text
- updated manifest schema validation and config schema examples
- updated the distribution front-door guide

## Validation

- `cargo test cli_distribution_generate_closeout_json_writes_report --test cli_output_tests -- --nocapture`
- `cargo test cli_distribution_write_summary_json_writes_contract_file --test cli_output_tests -- --nocapture`
- `cargo test validate_manifest_schema_accepts_docs_policy_bootstrap_distribution_and_release_sections -- --nocapture`

## Outcome

The optional distribution surface is now materially less self-hosting-biased
across the publish/summary/closeout path. The next valid move is a decision on
whether one consumer-proof adoption is now honest enough, or whether one final
internal policy gap still needs to be closed first.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `RELEASE`
- Moved: manifest-driven distribution policy widened from metadata/preflight
  only to publish identity, summary identity, artifact expectation shape, and
  closeout defaults
- Remaining open: explicit decision on whether the widened surface is now ready
  for one bounded consumer proof

## Next Task

Execute `docs/specs/batch-cards/102-decide-post-distribution-policy-widening-slice.md`
to decide whether the widened optional distribution surface is now honest
enough for one consumer-proof adoption batch.
