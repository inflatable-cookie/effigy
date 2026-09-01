# 1103 - Establish The Catalog-Pack Support Floor

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contracts: [`001`](../../../contracts/001-working-rules.md), [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Complete
Owner: Effigy support-policy data and validation
Created: 2026-09-01
Ready since: 2026-09-01 operator approval
Completed: 2026-09-01
Evidence: [`../../../logs/2026-09/01-202830-catalog-pack-support-floor-1103.md`](../../../logs/2026-09/01-202830-catalog-pack-support-floor-1103.md)

## Purpose

Create the machine-readable Effigy compatibility authority that official pack
publication must consume before it can mutate a package or channel.

## Work

- add `support/catalog-pack-update.toml` with schema version `1`,
  `as_of_release = "0.12.1"`, and nonempty
  `required_versions = ["0.12.1"]`
- omit `oldest_update_capable_release` before Effigy publicly exposes update
- add one typed parser/validator and focused failure proofs
- document that only an Effigy support-policy or release PR may change the file
- keep validation local and network-free; publication owns remote release and
  latest-release checks later
- close with one evidence log and move the strict lane to blocked card `1104`

## Acceptance

- [x] the committed file parses through one typed owner and rejects unknown keys
- [x] schema version, semantic versions, nonempty/duplicate-free required set,
      current release membership, and `as_of_release` agreement are validated
- [x] the pre-update state rejects any `oldest_update_capable_release`
- [x] a future update-capable state requires the oldest field to equal the
      minimum required semantic version
- [x] validation needs no network and cannot affect runtime pack selection
- [x] docs identify Effigy as sole policy owner and the pack repository as a
      read-only consumer by resolved commit/blob
- [x] focused tests, repository docs QA, full Effigy QA, fmt, clippy, and diff
      checks pass

## Review Oracle

Falsify these counterexamples before PR creation:

1. An empty, duplicate, malformed, or current-release-missing required set passes.
2. `as_of_release` differs from the current Effigy release and passes.
3. The oldest field is present before public update, or later disagrees with the
   minimum required version, and passes.
4. Unknown fields or an unsupported schema silently pass.
5. Local validation depends on GitHub/network access or mutates runtime pack state.
6. The pack repository or installed content can redefine the required set.
7. The card changes workflows, public commands, pack assets, snapshot ownership,
   release state, or publication state.

## Validation

- focused support-policy parser and failure tests
- `effigy qa:docs`
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Write one dated closeout log mapping every oracle row to exact proof. Record the
committed schema values, current Cargo release match, network-free boundary,
changed-file scope, and exact validation results.

## Stop Conditions

Stop if this needs GitHub API access in normal QA, a release mutation, workflow
edit, public update command, pack-repository creation, generated snapshot change,
or a second compatibility owner.

## Next Task

Update card `1104` to Ready after the card `1103` support floor is on pushed
`main`. Do not create the pack repository before then.
