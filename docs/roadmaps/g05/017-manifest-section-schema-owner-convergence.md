# g05.017 - Manifest Section Schema Owner Convergence

Status: Planned
Depends on: `g05.016`

## Goal

Give `[manifest]` one canonical serde and validation owner so root manifests,
included fragments, and bundle defaults cannot drift when new fields are added.

## Evidence

- `crates/effigy-manifest/src/composition.rs` defines `ManifestSectionConfig`
  with `include`, `extend`, `minimum_effigy_version`, and `root`
- `crates/effigy-manifest/src/bundles.rs` defined a second
  `BundleManifestSectionConfig` for bundle defaults and drifted until the
  `minimum_effigy_version` bug surfaced
- this is the same shape family under the same TOML key, so keeping duplicate
  owners creates guaranteed maintenance risk

## Scope

- move the canonical `[manifest]` section shape into a reusable owner in
  `effigy-manifest`
- reuse that owner from root composition and bundle-defaults composition
- keep only the intentionally relevant fields active per call site
- keep current validation and error meaning stable where practical

## Non-Goals

- no new `[manifest]` user-facing fields
- no bundle-system redesign
- no include/extend precedence rewrite

## Acceptance Criteria

- one canonical `[manifest]` section owner exists
- root composition and bundle defaults both use it
- adding a new shared `[manifest]` field only requires one schema-owner update
- regression tests cover root, included fragment, and bundle-defaults paths

## Suggested Validation

- `cargo test -p effigy-manifest minimum_effigy_version`
- `cargo test -p effigy-manifest decodelabs_bundle`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Open the implementation lane for canonical `[manifest]` owner extraction and the
bundle/root reuse pass.
