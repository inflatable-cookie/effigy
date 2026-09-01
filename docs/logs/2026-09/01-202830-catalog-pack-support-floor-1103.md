# Catalog-Pack Support Floor 1103 Closeout

Status: complete
Created: 2026-09-01
Roadmap: g08.048
Batch: 1103-establish-catalog-pack-support-floor
Handoff: `20260901-201505-catalog-pack-support-floor-1103.md`

## Summary

- Card `1103` lands Effigy's machine-readable catalog-pack update support floor
  and one typed local validator.
- The committed file is `support/catalog-pack-update.toml` with
  `schema_version = 1`, `as_of_release = "0.12.1"`, and
  `required_versions = ["0.12.1"]`. `oldest_update_capable_release` is omitted.
- Current Cargo workspace release is `0.12.1`. The parser requires
  `as_of_release` to equal that version and membership in `required_versions`.
- Owner: `effigy-catalog::support_policy`. Pack selection, acquisition, and
  activation do not read the file. Validation takes a TOML string, the current
  release, and an explicit update-capability flag. No network client exists on
  that path.
- `PackUpdateCapability::for_this_build()` is a support-policy-owned fact and
  returns `Absent`. Official artifact/channel publication is independent and
  does not require `oldest_update_capable_release`. Tests inject
  `PackUpdateCapability::Present` for the future oldest-field invariant without
  claiming that a released Effigy exposes public `service pack update`.

## Committed policy

```toml
schema_version = 1
as_of_release = "0.12.1"
required_versions = ["0.12.1"]
```

Parser owner: `crates/effigy-catalog/src/support_policy/`.
Network-free boundary: `CatalogPackUpdatePolicy::parse` has no path, client, or
runtime pack arguments. `load_from_repo_root` reads only the local committed
file.

## Review oracle → proof

1. An empty, duplicate, malformed, or current-release-missing required set
   passes — falsified by
   `support_policy::tests::empty_required_set_is_rejected`,
   `duplicate_required_versions_are_rejected`,
   `malformed_required_version_is_rejected`, and
   `current_release_missing_from_required_set_is_rejected`.
2. `as_of_release` differs from the current Effigy release and passes —
   falsified by `as_of_release_must_equal_the_current_release` and
   `malformed_as_of_release_is_rejected`.
3. The oldest field is present before public update, or later disagrees with
   the minimum required version, and passes — falsified by
   `oldest_field_is_forbidden_before_public_update`,
   `this_build_does_not_claim_released_public_update`,
   `artifact_publication_alone_does_not_require_the_oldest_field`,
   `future_update_capable_state_requires_oldest_equal_to_minimum_required`,
   `future_update_capable_state_rejects_missing_oldest_field`, and
   `future_update_capable_state_rejects_oldest_that_disagrees_with_minimum`.
4. Unknown fields or an unsupported schema silently pass — falsified by
   `unknown_fields_are_rejected` and `unsupported_schema_version_is_rejected`.
5. Local validation depends on GitHub/network access or mutates runtime pack
   state — falsified by
   `parse_accepts_only_local_document_current_release_and_capability` and
   `pack_runtime_modules_do_not_reference_the_support_floor`. The isolation
   test scans crate-root symbols
   (`CatalogPackUpdatePolicy`, `PackUpdateCapability`, `SupportPolicyError`,
   `CATALOG_PACK_UPDATE_POLICY_FILE`, `SUPPORTED_CATALOG_PACK_UPDATE_SCHEMA`,
   `current_effigy_release`) plus `support_policy` and `catalog-pack-update`
   across pack selection/acquisition/activation owners: pack domain modules,
   runner `service_command/pack.rs`, and `effigy-containers` `lib.rs`. It does
   not scan `lib.rs` (legitimate re-exports) or `pack/tests.rs`.
6. The pack repository or installed content can redefine the required set —
   falsified by docs in `067`, `071`, architecture `026`, and contract `043`,
   plus `committed_file_matches_this_crate_release_without_oldest_field`
   asserting the file lives at `support/catalog-pack-update.toml` in this
   repository.
7. The card changes workflows, public commands, pack assets, snapshot
   ownership, release state, or publication state — falsified by the
   changed-file inventory below: no `.github/workflows/`, no pack command
   grammar, no catalog asset move, no snapshot, no tag/publish/visibility.

## Changed-file inventory

- `support/catalog-pack-update.toml`
- `crates/effigy-catalog/src/support_policy/mod.rs`
- `crates/effigy-catalog/src/support_policy/tests.rs`
- `crates/effigy-catalog/src/lib.rs`
- `docs/guides/067-catalog-services-reference.md`
- `docs/guides/071-catalog-service-authoring.md`
- `docs/architecture/026-feature-placement-and-command-surface.md`
- `docs/architecture/010-package-map.md`
- `docs/contracts/043-feature-placement-and-surface-migration-contract.md`
- `CHANGELOG.md`
- `PAPERCUTS.md`
- card `1103`, roadmap `g08.048`, spec `115`, and front-door Next Task
  surfaces
- this log

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`
- Movement: no machine-readable Effigy support floor → committed schema `1`
  file at `0.12.1` with a typed local validator and a tested future
  oldest-field invariant
- Remaining gap: card `1104` pack-repository consumption by resolved
  commit/blob; remote release existence and candidate compatibility; public
  update still absent

## Review repair

PR 78 review on `f1c9025f` required two in-bounds repairs:

1. `for_this_build()` no longer derives released-update capability from
   `OfficialPackChannel::published`. Support-policy owns `Absent` until a
   released Effigy exposes public update. The publication counterexample
   constructs a published channel and still forbids the oldest field.
2. Isolation scan covers crate-root symbols and activation/selection
   consumers, not only `support_policy` / `catalog-pack-update` in pack
   domain sources.

## Validation Performed

- `cargo test -p effigy-catalog --lib support_policy` — 17 passed
- `cargo fmt --all -- --check` — pass
- `cargo clippy --all-targets -- -D warnings` — pass
- `git diff --check` — pass
- `effigy qa:docs` — pass (links, json-examples, indexes, forbidden, headings,
  contains, workflow-paths, vision next-action)
- `effigy qa` — 3688 passed, 1 skipped; docs and JSON-contract checks passed

## Next Task

Update card `1104` to Ready after the card `1103` support floor is on pushed
`main`. Do not create the pack repository before then.
