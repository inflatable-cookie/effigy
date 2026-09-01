# Catalog-Pack Acquisition Prototype Closeout

Status: complete
Created: 2026-09-01
Roadmap: g08.040
Batch: catalog-pack-acquisition-1095

## Summary

- Card `1095` landed the complete in-repository catalog-pack acquisition
  prototype under strict spec `113`, architecture `026`, and contract `043`.
- Catalog fragments now resolve through four layers — project override, user
  override, active installed pack, compiled baseline — with one selection
  implementation and one transport seam.
- The compiled baseline stayed permanent. A machine with no pack store, no
  `oras`, and no network resolves exactly what it resolved before.
- No official artifact was published, no concrete catalog asset moved, no
  release or workflow surface changed, and no public
  `effigy service pack update` exists.

## Changes

- `crates/effigy-catalog/src/pack/` — new pack domain: typed `pack.toml`
  manifest with identity, version, manifest schema version, and a semver Effigy
  requirement; deterministic sha256 content identity; candidate validation that
  reuses `ServiceSchema` rather than restating fragment rules; versioned
  user-state store with atomic `state.json` activation, install lineage, swap
  rollback, non-destructive reset, and retention; the acquire → validate →
  store → activate transaction behind the injectable `PackCandidateAcquirer`
  seam; layer selection with structured fallback reasons; and the fixed
  baseline-owned official channel.
- `crates/effigy-catalog/src/fragment.rs` — `FragmentSource::InstalledPack` and
  `CatalogResolver::with_installed_pack`, placing the pack layer below both
  overrides and above the compiled baseline in both `resolve` and `list`.
- `crates/effigy-cli` — `service pack status|install|rollback|reset` grammar,
  `--path`, exactly-one-candidate enforcement, and typed help. No `update`
  shape parses.
- `src/runner/service_command/pack.rs` — runner edge: the OCI acquirer built on
  the existing `effigy-artifacts` adapter (no second transport client), text and
  JSON rendering for the four shapes, and the `catalog.pack-health` doctor
  finding.
- `src/runner/doctor_ports.rs` — pack health surfaced through the existing
  `runtime_diagnostics` port, before the container-policy early returns, so it
  reports on repos with no container policy.
- `crates/effigy-containers` — all catalog resolver construction routed through
  `effigy_catalog::pack::layered_resolver`, keeping this crate's existing
  `~/.effigy` discovery (and its test override) while layer order lives in one
  place.
- Docs: `067` catalog layers and pack sections, `071` packaging note, `025`
  command matrix row, `017` payload schemas, `CHANGELOG.md`.

## Vision Target Delta

- Primary tags: `feature-placement`, `catalog-ownership`, `offline-first`
- Movement: baseline `concrete catalog definitions are only ever compiled in`
  -> current `concrete definitions can be independently versioned and installed
  while the compiled baseline stays permanent and offline-complete`
- Remaining gap: no official pack is published, so the acquisition path has no
  default source and `update` remains absent. Concrete-asset cutover is
  unstarted by design.

## Review Oracle Falsification

Each row names the exact test that would fail if the counterexample were true.

1. **Empty user state, no `oras`, no network changes baseline behavior.**
   - `effigy-catalog` `pack::tests::empty_user_state_selects_the_compiled_baseline`
   - `effigy` `runner::service_command::tests::catalog_list_reports_bundled_fragments`
   - `effigy` `runner::service_command::tests::baseline_list_reports_no_store_selection_in_json`
     (asserts `postgres` still reports `bundled`)
   - `effigy` `runner::service_command::tests::catalog_extract_defaults_to_project_override_dir`
   - `effigy-catalog` integration `pack_layer::compose_assembly_through_a_pack_layer_matches_baseline_assembly_shape`
     (baseline assembly is the control arm)
   - Whole existing `effigy-catalog` integration suite (57 tests) unchanged.
2. **An installed pack outranks a project or user override.**
   - `effigy-catalog` `pack::tests::active_pack_outranks_baseline_but_not_project_or_user_overrides`
   - `effigy-catalog` integration `pack_layer::project_and_user_overrides_still_outrank_an_installed_pack`
3. **A pulled-but-invalid candidate still activates, or damages prior content.**
   - `effigy-catalog` `pack::tests::failed_candidate_leaves_active_selection_and_prior_content_untouched`
     (incompatible and malformed candidates; asserts prior install's content id
     is byte-identical afterwards)
   - `effigy-catalog` `pack::tests::install_rejects_a_candidate_with_no_usable_fragment`
   - `effigy-catalog` `pack::tests::install_leaves_no_staging_residue`
   - `effigy` `runner::service_command::pack::tests::a_pulled_but_incompatible_candidate_leaves_the_active_selection_alone`
     (the fake adapter confirms the pull happened first)
4. **Unhealthy active state falls back silently or without repair.**
   - `effigy-catalog` `pack::tests::deleted_active_content_falls_back_visibly_to_the_baseline`
   - `effigy-catalog` `pack::tests::corrupted_active_manifest_falls_back_visibly_to_the_baseline`
   - `effigy-catalog` `pack::tests::newly_incompatible_active_pack_falls_back_visibly_to_the_baseline`
   - `effigy-catalog` `pack::tests::unreadable_store_state_falls_back_visibly_to_the_baseline`
   - `effigy` `runner::service_command::tests::unhealthy_active_pack_warns_in_text_and_reports_a_reason_in_json`
   - `effigy` `runner::service_command::pack::tests::deleted_active_content_yields_a_doctor_finding_with_one_repair_command`
     (asserts the remediation names exactly one command)
   - `effigy` `runner::service_command::pack::tests::unreadable_store_state_still_lets_status_report_instead_of_failing`
5. **Installed content redirects the fixed official channel.**
   - `effigy-catalog` `pack::tests::official_update_ignores_update_sources_declared_by_pack_content`
   - `effigy-catalog` `pack::tests::published_official_channel_plans_a_baseline_owned_candidate`
   - `effigy` `runner::service_command::pack::tests::installed_content_cannot_redirect_the_fixed_official_channel`
     (a pack declaring `[update] source = "oci://attacker.invalid/..."` is
     installed and activated first, then the resolved reference is checked)
6. **Rollback or reset is wrong or unrecoverable.**
   - `effigy-catalog` `pack::tests::rollback_after_two_installs_selects_the_previous_validated_pack`
     (asserts the selected content id, and that a second rollback returns)
   - `effigy-catalog` `pack::tests::reset_selects_baseline_and_still_allows_rollback`
   - `effigy-catalog` `pack::tests::rollback_without_lineage_fails_deterministically`
   - `effigy` `runner::service_command::pack::tests::rollback_and_reset_are_deterministic_and_keep_content_recoverable`
7. **A normal command invokes the OCI adapter or probes the network.**
   - `effigy` `runner::service_command::pack::tests::ordinary_catalog_work_never_invokes_the_oci_transport`
     — structural: `effigy-catalog` declares no artifact/transport dependency and
     its resolution/selection/store sources spawn no process, so catalog paths
     cannot reach `oras`; and a recording adapter observes zero calls while
     `list`, `resolve`, and `pack status` run against an active pack.
   - `effigy` `runner::service_command::pack::tests::oci_install_rejects_a_tag_only_reference_before_any_transport_call`
     (adapter call count is zero for an unpinned reference)
   - The adapter is constructed in exactly one place: the `install` arm of
     `run_service_pack`.
8. **Help or JSON advertises `service pack update`, or assets/release move.**
   - `effigy` `tests::parse_tests::catalog_and_container_option_tests::parse_service_pack_has_no_public_update_command`
   - `effigy-catalog` `pack::tests::official_update_plan_stays_closed_until_publication`
   - `git diff --stat` touches no `.github/`, no `crates/effigy-catalog/catalog/`,
     no installer, release archive, Homebrew, or S3 surface.
   - `effigy service pack status` and `--json service pack status` were smoke-run
     against an isolated `HOME`: install, list, corrupt, reset, and rollback all
     behaved as specified.

## Store And Selection Fixtures

- Every pack test drives an isolated Effigy user-state home through
  `effigy_catalog::pack::with_test_effigy_home`, so nothing reads or writes a
  developer's real `~/.effigy`.
- Store layout: `~/.effigy/catalog-packs/v1/{state.json,installs/<id>/,staging/}`.
- `state.json` is rewritten by temp-file-plus-rename, so a reader sees the whole
  previous selection or the whole new one.
- Install identifiers are content-addressed (`<pack>-<version>-<sha256[..16]>`),
  so a repeat install lands in the same directory and never partially
  overwrites a neighbour.

## Validation Performed

- command: `cargo test -p effigy-catalog`
  - result: pass — lib 82 tests (22 new under `pack::tests`), integration 57
    tests (4 new under `pack_layer`, 53 pre-existing unchanged)
- command: `cargo test -p effigy-artifacts`
  - result: pass — the adapter seam is consumed, not modified
- command: `cargo test -p effigy --lib runner::service_command`
  - result: pass — 15 tests (11 new pack rows, 4 service rows)
- command: `cargo test --workspace`
  - result: pass
- command: `effigy qa`
  - result: pass
- command: `cargo fmt --all -- --check`
  - result: pass
- command: `cargo clippy --all-targets -- -D warnings`
  - result: pass
- command: `git diff --check`
  - result: clean

## Incidental Repairs

Two flakes surfaced during full-suite validation and were repaired in this
batch because they blocked an honest QA gate:

- `effigy-doctor` `workflow::tests::runtime_diagnostic_findings_are_included_before_summary`
  hardcoded the doctor check count as `20`. Adding `catalog.pack-health` made it
  `21`. It now asserts against `ALL_CHECK_IDS.len()`.
- `effigy-containers` `tests::policies::validate_compose_backend_runtime_*` read
  `HOME` and `EFFIGY_COMPOSE_BACKEND` without taking `crate::test_env_lock()`,
  while sibling tests in `colima/tests.rs` and `compose/tests.rs` hold that lock
  while swapping those variables globally. Both tests now take the lock. The
  broader unguarded-env pattern in that crate is logged in `PAPERCUTS.md`, not
  fixed here.

`PAPERCUTS.md` also gained an entry for `service list` counting bundled
`README.md` and `compose.override.example.yml` as fragments — pre-existing, out
of this card's scope.

## Risks

- The official repository constant is the RFC 2606 placeholder
  `packs.invalid/effigy/default-catalog` with `published = false`. It is not a
  chosen coordinate; the publication lane must replace it deliberately.
- Retention keeps three installs beyond pinned ones. A workflow that installs
  many packs quickly will prune older content; lineage for one deterministic
  rollback is always preserved.
- Fallback is visible on `service list` and `service pack status` and in
  `doctor`. Container, system, and workspace commands resolve the baseline
  correctly but do not restate the warning inline.

## Next Task

Return to planning for official catalog-pack publication and concrete-asset
cutover under contract `043`. That lane needs a real OCI coordinate and explicit
workflow-edit authority; it is not ready.
