# Official Catalog-Pack Update 1107 Closeout

Status: complete
Created: 2026-09-02
Roadmap: `g08.048`
Spec: `115`
Card: `1107`
Branch: `worker/g08-048-official-catalog-pack-update-1107`

## Outcome

Public `effigy service pack update` now resolves the compiled official `stable`
channel on `ghcr.io/inflatable-cookie/effigy-catalog-pack` through the existing
`effigy-artifacts` OCI adapter, then sends only the immutable digest through
the existing acquire-validate-store-activate transaction.

- official repository (compiled, not overridable):
  `ghcr.io/inflatable-cookie/effigy-catalog-pack`
- channel: `stable`
- inspect reference: `oci://ghcr.io/inflatable-cookie/effigy-catalog-pack:stable`
- accepted digest:
  `sha256:91de584e77487765c24f53abb63413783a99c0a7926c25aee1289a3cf370d9f3`
- unpacked content identity:
  `sha256:9498d33f1eccbb91e971b55f5169830baca26326a8f802408a0432e733254974`
- pack version activated by the isolated smoke: `1.0.1`

`OfficialPackChannel::baseline().published` is `true`. A verified already-active
digest is a deterministic no-op: no pull, no store mutation. Resolution, pull,
compatibility, validation, and activation failures leave active, previous, and
channel identity unchanged. `PackUpdateCapability::for_this_build()` stays
`Absent`; `support/catalog-pack-update.toml` still omits
`oldest_update_capable_release`. Shipping the command on `main` is not a
released capability.

## Worker Preflight

Launcher worktree accepted:
`/Users/tom/.paseo/worktrees/310mya31/official-catalog-pack-update-1107` on
`worker/g08-048-official-catalog-pack-update-1107`, clean, registered. Fetched
origin with bounded non-interactive SSH; `HEAD == origin/main ==
a63b5d5bba70b515f0a7ca71522d20201e6ede39`; planning base
`6271b0ff129d006e47202b1b00def5ea7a395af8` is an ancestor; the tracked handoff
blob matches the dispatch file
`docs/handoffs/20260902-152515-official-catalog-pack-update-1107.md`. Sibling
link `../effigy-catalog-pack` resolves to
`/Users/tom/Dev/projects/effigy-catalog-pack`. No pack-repository files were
edited.

## Channel And Transaction Shape

- `OFFICIAL_PACK_REPOSITORY` is the real GHCR repository. Installed pack
  content, manifests, config, and environment cannot redirect it.
- `official_channel_tag_reference` is inspect-only (`:stable`).
  `plan_official_update` still goes through `PackCandidateSource::parse_oci`, so
  a mutable tag cannot become an install candidate.
- `ensure_official_channel_published` refuses unpublished channels before any
  inspect.
- `run_update` inspects `:stable` via `OciArtifactAdapter`, requires an exact
  `sha256:`-plus-64-lowercase-hex digest, plans that digest, then either
  returns `verified_active_digest` as `already-current` (decision and report
  snapshot taken under the durable store lock) or calls `install_pack` with
  `OciPackAcquirer` (same adapter as `service pack install`). A pulled
  descriptor digest that is absent, malformed, or different from the requested
  pin is refused before activation.
- JSON schema `effigy.service.pack.update.v1` reports `outcome` (`updated` or
  `already-current`), `channel`, `repository`, and `digest` inside the standard
  `effigy.command.v1` envelope.
- Text output names the channel, repository, and digest.
- `service pack update` parses with `--json` / `--repo` / `--help` and rejects
  a coordinate argument and `--path`.

## Acceptance And Review-Oracle Mapping

Spec `115` whole-lane rows 6 and 7, plus card `1107`:

| Oracle / acceptance | Named proof |
| --- | --- |
| `stable` resolves to a digest through the existing artifact boundary | `official_update_inspects_the_stable_tag_and_pulls_only_the_digest`; live `oras manifest fetch --descriptor` of `:stable` equals `sha256:91de584e…` |
| text/JSON/help report channel and resolved digest | runner JSON assertions; `parse_service_pack_update_accepts_json_and_rejects_coordinate_args`; help render contains `effigy service pack update`; live `--json service pack update` envelope |
| verified already-active digest is a deterministic no-op | `verified_active_digest_is_a_noop_only_when_the_active_oci_content_still_proves`; `verified_already_active_digest_is_a_deterministic_noop`; `verified_noop_snapshot_is_taken_under_the_store_lock`; live isolated no-op (`state.json` sha256 unchanged) |
| channel identity is an exact OCI digest | `parse_oci_digest_accepts_only_exact_sha256_lowercase_hex`; `official_update_plan_rejects_malformed_digest_claims`; `malformed_channel_digest_claims_do_not_enter_the_install_transaction` |
| pulled descriptor digest is bound to the requested pin | `oci_install_rejects_a_mismatched_adapter_digest_before_activation`; `oci_install_rejects_an_absent_or_malformed_adapter_digest_before_activation`; `mismatched_pull_digest_does_not_activate_or_become_a_future_noop`; `absent_or_malformed_pull_digest_does_not_activate` |
| corrupt same-digest is repaired, not a no-op | `corrupt_already_active_digest_is_repaired_rather_than_treated_as_a_noop`; `a_local_active_install_is_never_an_official_digest_noop` |
| every resolution/pull/compatibility/validation/activation failure preserves active, previous, and channel metadata | `channel_resolution_failure_preserves_active_previous_and_channel_identity`; `tag_resolution_without_a_digest_does_not_enter_the_install_transaction`; `pull_failure_after_digest_resolution_leaves_store_state_untouched`; `incompatible_official_update_candidate_leaves_the_active_selection_alone`; unpublished plan still refused by `unpublished_official_channel_still_refuses_a_plan` |
| installed content cannot redirect the official coordinate | `installed_content_cannot_redirect_the_fixed_official_channel`; domain hostile-pack coordinate still plans `ghcr.io/inflatable-cookie/effigy-catalog-pack` |
| ordinary commands remain network-silent | `ordinary_catalog_work_never_invokes_the_oci_transport`; live `service pack status` and `service list` against the isolated store (no update) |
| mutable-tag activation rejected | `official_update_plan_rejects_a_mutable_tag_as_the_digest`; inspect without a digest never enters `install_pack` |
| no second transport client | `OciPackAcquirer` wraps `effigy-artifacts::OciArtifactAdapter`; tests inject `RecordingOciAdapter` |
| public surface succeeds against the card `1105`/`1106` artifact | live isolated update (below) |
| rollback/reset and recovery unchanged | `rollback_and_reset_are_deterministic_and_keep_content_recoverable`; `rollback_refuses_an_unhealthy_target_and_leaves_the_selection_alone`; advertised-repair tests; `reset_reports_the_recovery_path_in_json` |
| representative catalog consumers | `catalog_pack_cli_tests` (`an_unhealthy_pack_warns_visibly_in_both_text_and_json`, healthy machine, concurrent installs); `effigy-containers` `catalog_pack_fallback` (container/system/workspace boundary) |
| support floor unchanged | `PackUpdateCapability::for_this_build()` still `Absent` in `support_policy` tests; `support/catalog-pack-update.toml` unedited |

## Live Isolated Smoke

Read-only channel proof:

```text
oras manifest fetch --descriptor \
  ghcr.io/inflatable-cookie/effigy-catalog-pack:stable
```

returned digest
`sha256:91de584e77487765c24f53abb63413783a99c0a7926c25aee1289a3cf370d9f3`.

Isolated home: `/tmp/effigy-pack-update-1107-6qqoZr` (empty store before the
first update). Binary: worktree `target/debug/effigy`.

First `HOME=… --json service pack update`:

- envelope `effigy.command.v1`, result schema `effigy.service.pack.update.v1`
- `outcome`: `updated`
- `channel`: `stable`
- `repository`: `ghcr.io/inflatable-cookie/effigy-catalog-pack`
- `digest`: `sha256:91de584e77487765c24f53abb63413783a99c0a7926c25aee1289a3cf370d9f3`
- `installed.pack_version`: `1.0.1`
- `installed.content_id`:
  `sha256:9498d33f1eccbb91e971b55f5169830baca26326a8f802408a0432e733254974`
- `previous`: `null` (compiled baseline was the prior selection)
- store: `…/.effigy/catalog-packs/v1/state.json` with `active` install id
  `effigy-default-catalog-1-0-1-9498d33f…`

Repeated update (same home, 2026-09-02 15:50):

- `outcome`: `already-current`
- `stored_content`: `null`
- `state.json` sha256 before and after:
  `d96469c72ef2822413dcd557ebe8c13d876d5729ab4f517a69ff66dcad7c3e1f`
  (byte-identical; `installed_at_unix` stayed `1788360298`)

Ordinary commands against that store: `--json service pack status` reports
`reason: active-pack`, version `1.0.1`, content `9498d33f…`; `--json service
list` returns `effigy.service.list.v1` with 14 fragments. Failure atomicity is
the runner unit table above (no live GHCR outage was forced).

## Mutation Boundary

This card created no source tag, package, attestation, `stable` movement, S3
change, workflow edit, or Effigy release. The only external registry work was
the read-only `:stable` descriptor fetch and the user-invoked isolated update
plus repeated no-op. Shared spec/roadmap/contract/front-door next-task prose
was not edited; card `1108` stays orchestrator-integrated.

## Rust Quality Closeout

Profile `docs/contracts/rust-quality-profile.json`: `strict`; deviations empty;
toolchain `1.97.1` from `rust-toolchain.toml`. Applicable rules:
`RUST-READ-001`, `RUST-API-001`, `RUST-ERR-001`. No unsafe, async, or MSRV
trigger. `snapshot_verified_active` owns the lock-hold test seam; it is not a
pass-through. Public additions on this repair: `parse_oci_digest`,
`PackError::OciDigestInvalid`, `VerifiedActiveDigest` (`Debug`/`Clone`/`Eq`).
Mechanical closeout ran through `northstar-rust-quality closeout` (snapshot
`52274a82…`) with cargo check/clippy `-D warnings` on `effigy-catalog` and
`effigy`, plus focused `pack::` tests (92 catalog + 29 runner). Compact result
lives outside the worktree under git metadata. Check, clippy, and test records
are `warning` solely because Cargo reports a pre-existing future-incompat for
`proc-macro-error2 v2.0.1` (zero diagnostics in this tranche). Human review:
`RUST-READ-001`, `RUST-API-001`, and `RUST-ERR-001` compliant.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`
- Movement: unpublished placeholder official coordinate with no public update
  command -> compiled GHCR repository, explicit `service pack update`, digest
  acquisition, verified no-op, and failure atomicity against the accepted
  `v1.0.1` artifact
- Remaining gap: card `1108` (pack-repo proposal automation) and an Effigy
  binary release that may record `oldest_update_capable_release`; both remain
  outside this PR

## Exact-Head Review Repair (reviewed head `7de3cc5b6f9f827a75fc3586d510fd6824b7c06b`)

Exact-head review of PR 84 required three `execution-miss` repairs on this
branch:

1. Channel identity is an exact OCI digest. `resolve_official_digest` and
   `plan_official_update` now require `sha256:` plus 64 lowercase hexadecimal
   characters. `sha256:short`, uppercase 64-hex, and `prefix@sha256:<64 hex>`
   fail at resolution with zero pull/store calls and preserved state.
2. The pulled descriptor digest is bound to the requested pin.
   `require_acquired_digest_matches` refuses an absent, malformed, or different
   digest before activation. `build_record` records the requested pin, not the
   adapter's claim. A mismatched report cannot become a future
   `already-current` no-op.
3. The verified no-op holds `PackStore::lock` across active-record selection,
   verification, and the snapshot used to render the report, then releases it
   before network acquisition. A concurrent rollback cannot commit during that
   window; the snapshot `active`/`previous` stay the locked decision.

Named proofs: `parse_oci_digest_accepts_only_exact_sha256_lowercase_hex`,
`official_update_plan_rejects_malformed_digest_claims`,
`malformed_channel_digest_claims_do_not_enter_the_install_transaction`,
`oci_install_rejects_a_mismatched_adapter_digest_before_activation`,
`oci_install_rejects_an_absent_or_malformed_adapter_digest_before_activation`,
`mismatched_pull_digest_does_not_activate_or_become_a_future_noop`,
`absent_or_malformed_pull_digest_does_not_activate`,
`verified_noop_snapshot_is_taken_under_the_store_lock`.

Shared spec/roadmap/contract/front-door next-task prose remains untouched.

## Final Validation Round

Repair-head round (after the three execution-miss fixes):

- `cargo fmt --all -- --check`: clean.
- `git diff --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean (same pre-existing Cargo
  future-incompat note as above; clippy itself denied warnings).
- Focused catalog `pack::` suite: 92 passed.
- Focused runner pack suite: 29 passed.
- `catalog_pack_cli_tests`: 3 passed.
- `effigy-containers` `catalog_pack_fallback`: 3 passed.
- `effigy-catalog` `support_policy`: 17 passed, including
  `this_build_does_not_claim_released_public_update`.
- `parse_service_pack_update_accepts_json_and_rejects_coordinate_args`: passed.
- `effigy qa`: passed. `effigy test` 3722 passed, 1 skipped; `qa:docs` (links,
  json-examples, index, forbidden, vision headings/contains/workflow-paths,
  next-action) passed; `qa:json` passed. The known
  `cli_container_attached_session_handles_sigint_during_startup` flake did not
  fire in this round.
- `effigy doctor --json`: `ok: true`, zero error findings. Seven pre-existing
  warning-level `scan.god-files` findings remain. No `scan.generated-in-src`
  error.

## Next Task

Orchestrator reviews this worker PR against current pushed `main` and
integrates shared milestone/spec/contract/front-door prose with parallel card
`1108` after accepted exact-head review and merge. This PR does not merge
itself and does not authorize an Effigy release.
