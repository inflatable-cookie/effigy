# g08.011 - Discovery And Doctor Correctness

Status: Complete
Depends on: `g08.010`
Completed: 2026-06-10

## Goal

Stop test-fixture and example manifests from surfacing as live catalogs, and
guarantee `effigy doctor` reports green on a clean repository tree. Remediates
assessment finding 1.

Today, default catalog discovery skips only
`.git .effigy external node_modules vendor target .next`
([discovery.rs:217](../../../crates/effigy-routing/src/discovery.rs)). It has no
concept of a fixture/test manifest, so this repo's own
`tests/fixtures/graph-agent-benchmark/*/effigy.toml` are discovered as real
catalogs, miss-errors list them, and `doctor` exits non-zero on a clean tree
with a fixture-sourced `manifest.schema.unsupported_key` error. Any consumer
repo with example or fixture manifests hits the same class of bug, and a red
doctor on a clean tree poisons release gates and agent trust.

## Scope

- give discovery a principled way to exclude test/fixture manifests from
  ambient catalog discovery
- decide and document the rule: internal default skip-dir for `tests` vs an
  explicit fixture marker vs honoring a discovery-ignore convention — pick the
  least surprising option that does not silently hide a real nested catalog a
  consumer intends to expose
- ensure `effigy doctor` does not attribute a hard error to a manifest that
  discovery should never have treated as a live catalog
- prove `effigy doctor` is green on this repo's clean tree

## Guardrails

- do not break legitimate nested-catalog discovery for repos that intentionally
  nest real catalogs
- do not hard-code this repo's fixture paths; the rule must be repo-agnostic
- no CLI grammar changes
- no JSON schema id or version changes
- preserve the existing configured `[catalog.discovery] ignore` behavior

## Execution Plan

- [x] **Batch A — Decide and document the exclusion rule.** Decision: honor the
  existing `[catalog.discovery] ignore` convention rather than hard-coding
  `tests` into the internal skip list. Rationale: a hard-coded default skip
  would be surprising and could silently hide a real consumer catalog under
  `tests/`, violating the milestone guardrail. Discovery already provides two
  opt-outs (`ignore`, `[manifest] root = true`); the defect was that this repo
  never configured its own fixture tree. Documented in
  [`docs/guides/022-manifest-cookbook.md`](../../guides/022-manifest-cookbook.md).
- [x] **Batch B — Apply the rule + regression coverage.** Added
  `[catalog.discovery] ignore = ["tests"]` to this repo's `effigy.toml` and a
  named regression test
  (`discover_manifest_paths_excludes_ignored_fixture_tree`) in
  `effigy-routing` proving the fixture tree is excluded while an intentional
  nested catalog still resolves.
- [x] **Batch C — Doctor clean-tree guarantee + schema catch-up.** Verified
  `effigy doctor` returns `err:0` on this repo. Closing Batch B surfaced an
  adjacent bug: the doctor schema validator never recognized `[catalog.discovery]`
  even though `effigy-routing` consumes it, so configuring the opt-out itself
  tripped `manifest.schema.unsupported_key`. Added `catalog_section.rs` to the
  doctor schema validator (allows `alias` + `discovery`, validates
  `discovery.enabled` bool and `discovery.ignore` string array) plus
  accept/reject schema tests. The existing
  `validate_manifest_schema_accepts_current_repo_manifest` test now guards the
  real manifest including the new key.

## Additional Scope Handled

The assessment framed finding 1 as discovery scope only. In practice it had two
linked defects: (1) fixtures discovered as catalogs, and (2) the doctor schema
validator was out of sync with the discovery config the code already consumed.
Both are now fixed. No change was made to the general default skip list, by
design.

## Governing Contracts

- [`001-working-rules.md`](../../contracts/001-working-rules.md)

## Acceptance Criteria

- [x] `effigy doctor` on this repo's clean tree reports `err:0`
  (`summary ok:16 warn:0 err:0`, "No findings")
- [x] miss-errors no longer list `tests/fixtures/**` manifests as catalogs
  (now lists only `effigy (.../effigy.toml)`)
- [x] an intentional nested real catalog still resolves (proven by fixture)
- [x] the exclusion rule is documented and repo-agnostic
- [x] changelog `[Unreleased] > Fixed` records the discovery/doctor correction

## Evidence

- `effigy.toml`: added `[catalog.discovery] ignore = ["tests"]`
- `crates/effigy-routing/src/discovery.rs`: regression test
  `discover_manifest_paths_excludes_ignored_fixture_tree`
- `crates/effigy-doctor/src/manifest_schema/catalog_section.rs`: new validator
- `crates/effigy-doctor/src/manifest_schema/tests.rs`: accept/reject tests for
  `catalog.discovery`
- `docs/guides/022-manifest-cookbook.md`: documented the convention and the
  fixture-tree case
- validation: `cargo test -p effigy-routing -p effigy-doctor` green
  (7 + 52 tests); `cargo fmt --all -- --check` clean; clippy clean on both
  crates; live `effigy doctor` reports no findings

## Next Task

Open `g08.012` (Supply-Chain and CI Security Gates). Batch A (local `deny.toml`
+ baseline) is executable now; Batch C (CI workflow wiring) is gated on explicit
human workflow-edit approval.
