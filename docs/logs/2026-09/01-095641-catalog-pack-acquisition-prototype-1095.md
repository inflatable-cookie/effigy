# Catalog-Pack Acquisition Prototype Closeout

Status: complete
Created: 2026-09-01
Roadmap: g08.040
Batch: catalog-pack-acquisition-1095

## Summary

- Card `1095` landed the complete in-repository catalog-pack acquisition
  prototype under strict spec `113`, architecture `026`, and contract `043`.
- Orchestrator review of head `19cf30fb1` requested changes on six findings, and
  re-review of head `3906ea85e` requested changes on three more. All nine are
  repaired in the same PR; both rounds are recorded in
  [Review Repair Round](#review-repair-round) and folded into the oracle
  mapping below.
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
  requirement; deterministic sha256 content identity over the whole tree;
  symlink-hostile traversal that rejects anything but regular files and
  directories with valid UTF-8 names — including the root and `pack.toml`,
  classified before either is read; candidate validation that reuses
  `ServiceSchema` rather than restating fragment rules; versioned user-state store with atomic `state.json`
  activation, an advisory cross-process lock over durable mutation, full
  install lineage with no pruning, swap rollback, and non-destructive reset;
  the acquire → validate → store → activate transaction behind the injectable
  `PackCandidateAcquirer` seam, with acquisition outside the lock and landing
  plus activation inside it; selection that re-proves the active pack on every
  use; and the fixed baseline-owned official channel.
- `crates/effigy-catalog/src/pack/fallback.rs` — the single place a baseline
  fallback reaches the operator, wired into `layered_resolver` so every
  catalog-backed consumer announces a source change exactly once per process,
  on stderr, in text or `effigy.catalog-pack.fallback.v1` form.
- `crates/effigy-catalog/src/pack/verify.rs` — one proof that stored content is
  still what the record says it is. Selection, `rollback`, and the `doctor`
  repair recommendation all run it, so none of them can trust a stale record
  while another re-proves the bytes.
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
   - `effigy-catalog` `pack::tests::reinstalling_identical_content_repairs_corrupt_storage_instead_of_reusing_it`
     (a reinstall over tampered stored bytes reports `repaired-corrupt` and
     leaves the recorded identity restored, rather than reactivating them)
   - `effigy-catalog` `pack::tests::a_file_symlink_inside_a_pack_is_rejected`,
     `..::a_directory_symlink_inside_a_pack_is_rejected`,
     `..::a_symlink_cycle_is_rejected_rather_than_traversed`,
     `..::a_symlinked_required_file_cannot_smuggle_content_in`
   - Root and manifest no-follow:
     `..::a_symlinked_stored_root_with_identical_bytes_is_never_reported_reused`,
     `..::a_post_install_symlinked_manifest_is_rejected_before_its_target_is_read`,
     `..::a_symlinked_pack_root_is_refused_by_direct_validation`
   - Identity is injective over accepted paths:
     `..::distinct_non_utf8_entry_names_are_rejected_rather_than_lossily_merged`,
     `..::a_pack_carrying_a_non_utf8_entry_name_is_refused_on_disk`,
     `..::distinct_accepted_trees_never_share_a_content_identity`
   - `effigy` `runner::service_command::pack::tests::a_pulled_but_incompatible_candidate_leaves_the_active_selection_alone`
     (the fake adapter confirms the pull happened first)
4. **Unhealthy active state falls back silently or without repair.**
   - `effigy-catalog` `pack::tests::deleted_active_content_falls_back_visibly_to_the_baseline`
   - `effigy-catalog` `pack::tests::corrupted_active_manifest_falls_back_visibly_to_the_baseline`
   - `effigy-catalog` `pack::tests::newly_incompatible_active_pack_falls_back_visibly_to_the_baseline`
   - `effigy-catalog` `pack::tests::unreadable_store_state_falls_back_visibly_to_the_baseline`
   - Post-install corruption, each with its own reason:
     `pack::tests::edited_non_manifest_bytes_fall_back_visibly`,
     `..::a_deleted_referenced_fragment_falls_back_visibly`,
     `..::a_removed_compose_fragment_falls_back_visibly`,
     `..::a_swapped_manifest_identity_falls_back_visibly`,
     `..::a_swapped_manifest_version_falls_back_visibly`,
     `..::a_broken_state_cross_reference_falls_back_instead_of_looking_empty`,
     `..::a_healthy_install_verifies_clean`
   - Propagation to ordinary consumers:
     `effigy-containers` `tests::catalog_pack_fallback::container_catalog_resolution_reports_a_baseline_fallback`,
     `..::the_container_boundary_announces_a_fallback_once_per_process`,
     `..::a_healthy_pack_supplies_container_content_without_a_notice`
   - Binary-level text and JSON proof:
     `tests/catalog_pack_cli_tests.rs::an_unhealthy_pack_warns_visibly_in_both_text_and_json`
     (asserts the stderr notice in both modes, that the baseline fragment is
     what the operator actually gets, and that the stdout envelope is untouched)
     and `..::a_healthy_machine_emits_no_fallback_notice`
   - `effigy` `runner::service_command::tests::unhealthy_active_pack_warns_in_text_and_reports_a_reason_in_json`
   - `effigy` `runner::service_command::pack::tests::deleted_active_content_yields_a_doctor_finding_with_one_repair_command`
     (asserts the remediation names exactly one command)
   - The advertised repair is honest:
     `..::doctor_recommends_reset_when_the_rollback_target_no_longer_verifies`,
     `..::doctor_recommends_rollback_only_when_the_target_actually_verifies`
   - `effigy` `runner::service_command::pack::tests::unreadable_store_state_still_lets_status_report_instead_of_failing`
5. **Installed content redirects the fixed official channel.**
   - `effigy-catalog` `pack::tests::official_update_ignores_update_sources_declared_by_pack_content`
   - `effigy-catalog` `pack::tests::published_official_channel_plans_a_baseline_owned_candidate`
   - `effigy` `runner::service_command::pack::tests::installed_content_cannot_redirect_the_fixed_official_channel`
     (a pack declaring `[update] source = "oci://attacker.invalid/..."` is
     installed and activated first, then the resolved reference is checked)
6. **Rollback or reset is wrong, unrecoverable, or deletes installed content.**
   - `effigy-catalog` `pack::tests::rollback_after_two_installs_selects_the_previous_validated_pack`
     (asserts the selected content id, and that a second rollback returns)
   - `effigy-catalog` `pack::tests::reset_selects_baseline_and_still_allows_rollback`
   - `effigy-catalog` `pack::tests::rollback_without_lineage_fails_deterministically`
   - `effigy-catalog` `pack::tests::every_successfully_installed_entry_is_retained`
     (five installs, all records and all content directories survive)
   - `effigy-catalog` `pack::tests::rollback_and_reset_delete_no_installed_content`
   - `effigy` `runner::service_command::pack::tests::rollback_and_reset_are_deterministic_and_keep_content_recoverable`
   - `tests/catalog_pack_cli_tests.rs::concurrent_installs_from_separate_processes_keep_every_record`
     (also asserts `service pack status` reports every install)
   - Rollback re-proves its target before mutating state:
     `pack::tests::rollback_refuses_a_tampered_previous_target_and_preserves_state`,
     `..::rollback_refuses_a_partially_deleted_previous_target`,
     `..::rollback_refuses_a_previous_target_that_is_no_longer_compatible`,
     `..::rollback_refuses_a_symlinked_previous_target`,
     `..::rollback_still_succeeds_when_the_previous_target_verifies`,
     `..::rollback_target_health_reports_the_defect_it_refuses_on`,
     `..::a_healthy_rollback_target_verifies_through_the_shared_proof`
   - `effigy` `runner::service_command::pack::tests::rollback_refuses_an_unhealthy_target_and_leaves_the_selection_alone`
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
  `effigy_catalog::pack::with_test_effigy_home` (or an explicit `PackStore`),
  so nothing reads or writes a developer's real `~/.effigy`.
- Store layout:
  `~/.effigy/catalog-packs/v1/{state.json,.lock,installs/<id>/,staging/}`.
- `state.json` is rewritten by temp-file-plus-rename, so a reader sees the whole
  previous selection or the whole new one.
- Install identifiers are content-addressed and carry the **full** sha256
  digest (`<pack>-<version>-<64 hex>`). The identifier is what decides whether
  an existing directory is "the same content", so a prefix would let two trees
  claim one path.
- A repeat install never trusts an existing directory: `land_content`
  re-hashes it against the recorded identity, reuses it only on a match, and
  otherwise replaces it with the freshly validated candidate, setting the
  displaced tree aside under `.corrupt-*` rather than deleting it.
- Retention is settled by planning: nothing is pruned. `install`, `rollback`,
  and `reset` have no deletion authority.
- Durable mutation is serialized by an advisory `flock` on `.lock` (`fs2`, the
  idiom already used by `effigy-codegraph` and `effigy-deps`). Acquisition runs
  outside it; landing plus the state transition run inside it.

## Validation Performed

- command: `cargo test -p effigy-catalog`
  - result: pass — lib 116 tests (56 under `pack::tests`), integration 57 tests
    (4 under `pack_layer`, 53 pre-existing unchanged)
- command: `cargo test -p effigy-artifacts`
  - result: pass — the adapter seam is consumed, not modified
- command: `cargo test -p effigy --lib runner::service_command`
  - result: pass — 18 tests (14 pack rows, 4 service rows)
- command: `cargo test -p effigy-containers`
  - result: pass — 230 tests, 3 new under `tests::catalog_pack_fallback`
- command: `cargo test -p effigy-doctor`
  - result: pass — 69 tests
- command: `cargo test --test catalog_pack_cli_tests`
  - result: pass — 3 end-to-end rows (visible fallback in text and JSON,
    healthy machine stays quiet, cross-process concurrent installs)
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

## Review Repair Round

Two review rounds. Head `19cf30fb1` requested changes on six findings; head
`3906ea85e` accepted those repairs and requested changes on three more. Each is
recorded below with the proof that would fail if the repair regressed.

### Round one — head `19cf30fb1`

1. **Retention was invented in implementation.** `MAX_RETAINED_INSTALLS` and
   `PackStore::prune` are removed outright; nothing in the store deletes
   installed content. Canonical planning now settles this (architecture `026`,
   contract `043`, roadmap `g08.040`, spec `113`, card `1095`), and the code
   follows it rather than the other way round. Proof:
   `every_successfully_installed_entry_is_retained`,
   `rollback_and_reset_delete_no_installed_content`, and the CLI-level
   `concurrent_installs_from_separate_processes_keep_every_record`.
2. **Active-pack health was not proven after installation.** Selection now runs
   the same validation an install candidate faces — manifest, compatibility,
   and fragments — then cross-checks the stored manifest against the install
   record (id, version, compatibility requirement, manifest schema version) and
   re-hashes the whole tree against the recorded content identity. A store
   pointer with no record behind it is `fallback-state-corrupt` rather than
   collapsing to "nothing installed". Reinstall re-verifies existing content and
   repairs a mismatch instead of reactivating it. Seven new selection tests plus
   the reinstall-repair test cover it.
3. **Fallback was silent for ordinary catalog consumers.** The notice moved into
   `effigy_catalog::pack::fallback` and is emitted from `layered_resolver`, the
   one boundary every catalog-backed command passes through — including the
   container, system, workspace, and task paths that have no selection payload
   of their own. It goes to stderr, once per process, as text or as
   `effigy.catalog-pack.fallback.v1`, so no existing stdout contract changes.
4. **Store mutation had no cross-process serialization.** Landing plus state
   transition, rollback, and reset now hold an advisory `flock`; acquisition
   stays outside it. Writing the concurrency proof also exposed a real defect
   the lock alone would not have fixed: staging and temp-file names were built
   from `(pid, coarse nanos)`, and two threads reading the same clock tick
   shared a staging directory and validated each other's payload. All such
   names now carry a process-wide counter. Proof:
   `the_store_lock_is_actually_exclusive`,
   `concurrent_installs_do_not_lose_lineage`,
   `concurrent_rollback_and_reset_keep_state_self_consistent`, and the
   genuinely multi-process
   `concurrent_installs_from_separate_processes_keep_every_record`.
5. **Pack traversal followed symlinks.** Every entry is now inspected with
   `symlink_metadata`; symlinks and non-regular files are rejected before
   hashing, copying, or validation, including for required files like
   `compose.fragment.yml`. Four Unix counterexamples cover file symlinks,
   directory symlinks, cycles, and a symlinked required file.
6. **Truncated install identity plus blind reuse.** The identifier carries the
   full 64-character digest, and reuse re-verifies stored content before
   activation. Proof: `install_identity_carries_the_full_content_digest` and the
   reinstall-repair test.

### Round two — head `3906ea85e`

The re-review accepted round one and the documented stderr diagnostic design,
and found three remaining execution misses.

7. **Symlink rejection was still incomplete at the two root reads.**
   `content_id` began with `collect_files(root, root)` and never classified the
   root, so `read_dir` followed a symlinked install root; `land_content` used
   `install_dir.is_dir()`, which follows the same link, so a link to a
   byte-identical tree was reported `reused-verified` and activated, only for
   selection to reject it afterwards. Separately, `validate_pack` called
   `PackManifest::load` before proving `pack.toml` was a regular file, so
   post-install corruption could have it read through a manifest symlink.

   `content_id` and `validate_pack` now classify the root before any read;
   `validate_pack` proves the manifest is a regular file before opening it; and
   `land_content` classifies the install path without following, moving a
   symlink or non-directory occupant aside instead of adopting it. `rename`
   moves the link itself, never its target. Proof:
   `a_symlinked_stored_root_with_identical_bytes_is_never_reported_reused`
   (the decoy is asserted byte-identical first, so the test would pass
   vacuously if it were not),
   `a_post_install_symlinked_manifest_is_rejected_before_its_target_is_read`
   (the impostor manifest would have parsed and matched the record, so reading
   through it would have surfaced as `fallback-content-changed`; getting
   `fallback-invalid-pack` is the proof it was refused first), and
   `a_symlinked_pack_root_is_refused_by_direct_validation`.

8. **Content identity was not injective over accepted paths.**
   `normalized_path_bytes` used `to_string_lossy`, so two distinct non-UTF-8
   names could normalize to the same replacement text and, with identical file
   bytes, produce the same content id and full install id.

   Non-UTF-8 entry names are now rejected as unsupported portable pack content —
   the clean contract, since fragment directory names become catalog service
   names and packs travel through OCI layers and archives that assume text
   paths. Path components are additionally length-prefixed rather than joined
   with a separator, so no two component sequences can encode alike. Proof:
   `distinct_non_utf8_entry_names_are_rejected_rather_than_lossily_merged`
   (asserts the two names are distinct yet lossily equal, then that both are
   refused), `a_pack_carrying_a_non_utf8_entry_name_is_refused_on_disk`, and
   `distinct_accepted_trees_never_share_a_content_identity`.

   The on-disk test returns early where the filesystem itself refuses such a
   name — APFS does, which is a stronger guarantee than ours — so the
   platform-independent assertion carries the proof on macOS and both run on
   filesystems that permit the name.

9. **Rollback and doctor trusted stale records rather than validated content.**
   `PackStore::rollback` checked only that a record existed and its path
   reported `is_dir`, and `pack_health_finding` recommended rollback whenever a
   `previous_record` existed. A previous install that had since been partially
   deleted, tampered with, symlinked, or become incompatible would be activated
   by an advertised one-step repair that reported success.

   The selection-time proof moved into `pack::verify::verify_installed_pack`,
   and selection, `rollback`, and the doctor recommendation all run that same
   function. `rollback` runs it before touching state and returns
   `RollbackTargetUnhealthy` on failure, leaving `active`, `previous`, and
   lineage exactly as they were. `doctor` names `rollback` only when the target
   passes, and `reset` otherwise. Proof: the four refusal tests
   (tampered, partially deleted, incompatible, symlinked), each asserting state
   is unchanged; `rollback_still_succeeds_when_the_previous_target_verifies`;
   `rollback_target_health_reports_the_defect_it_refuses_on`; and the runner-level
   `doctor_recommends_reset_when_the_rollback_target_no_longer_verifies`,
   `doctor_recommends_rollback_only_when_the_target_actually_verifies`, and
   `rollback_refuses_an_unhealthy_target_and_leaves_the_selection_alone`.

### Where each proof lives, and why

The container/system/workspace propagation proof is a unit test in
`effigy-containers`, not an assertion against the built binary. Under
`cargo test`, feature unification enables `effigy-containers/test-support`,
which pins that crate's `~/.effigy` resolution to a synthetic home so container
tests can never touch a developer's real one — so the harness binary cannot
observe a `HOME`-based pack store on the container path. The binary-level test
covers `service list` in both render modes, where `HOME` is honoured, and the
crate-level test covers the container boundary. A `cargo build` binary was also
driven by hand end to end to confirm the released shape behaves the same.

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
- Nothing is ever pruned, so a machine that installs many packs accumulates
  store content without bound. That is the settled decision for this lane;
  garbage collection is a later explicit operator choice.
- Selection re-hashes the active pack on every resolver construction. A pack is
  the same order of size as the compiled baseline (~200 KB across ~50 small
  files), so this is well under a millisecond and is paid rather than cached —
  a cache's invalidation would itself be a correctness surface. A materially
  larger pack format would need that revisited.
- Pack entry names must be valid UTF-8. That is a real constraint on what a
  pack may contain, taken deliberately so content identity stays injective; a
  pack built from a tree with non-UTF-8 names will be refused rather than
  silently given an ambiguous identity.
- The fallback notice goes to stderr. A consumer that captures only stdout will
  still get correct baseline behaviour but will not see the warning; the
  `service` surfaces and `doctor` carry the same facts in stdout payloads.

## Next Task

Return to planning for official catalog-pack publication and concrete-asset
cutover under contract `043`. That lane needs a real OCI coordinate and explicit
workflow-edit authority; it is not ready.
