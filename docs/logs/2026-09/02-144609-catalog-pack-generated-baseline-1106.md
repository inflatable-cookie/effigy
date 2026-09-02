# Catalog-Pack Generated Baseline 1106 Closeout

Status: complete
Created: 2026-09-02
Roadmap: `g08.048`
Spec: `115`
Card: `1106`
Branch: `worker/g08-048-generated-catalog-baseline-1106`

## Outcome

Effigy's embedded concrete catalog is no longer an editable authority. The
checked-in `crates/effigy-catalog/catalog/` tree is now a byte-for-byte
generated copy of the official pack repository's canonical `pack/` root at the
accepted `v1.0.1` publication input:

- snapshot source: annotated `v1.0.1`, tag object
  `2bb561109dfe8ec1346779370e2e9f428ef5ddd2`, peeled commit
  `5ef0ec2b64612c7803cc6105a65ea462862a0b21` (catalog-pack PR `#4` merge
  `7427421a3bebf207ce9979c47f60609d1b276713`)
- snapshot: 42 regular files, 88,600 bytes, including `pack.toml` — exact
  `pack/` copy, `diff -r` identical after generation
- typed lock: `crates/effigy-catalog/catalog-pack.lock.toml` (schema 1)
  recording source repository, peeled source commit, source commit time,
  source tag and tag object, pack id/version, and the two distinct identity
  facts below
- OCI manifest digest: `sha256:91de584e77487765c24f53abb63413783a99c0a7926c25aee1289a3cf370d9f3`
- unpacked content identity: `sha256:9498d33f1eccbb91e971b55f5169830baca26326a8f802408a0432e733254974`

## Worker Preflight

Launcher worktree accepted:
`/Users/tom/.paseo/worktrees/310mya31/generated-catalog-baseline-1106` on
`worker/g08-048-generated-catalog-baseline-1106`, clean, registered. Fetched
origin; `HEAD == origin/main == fa0d1d7e85c21ce5ecec5c5da0effdba5d8804ec`;
planning base `2c547261d71c236d3237681557411a1b5bcf772b` is an ancestor; the
tracked handoff blob matches the dispatch file. Sibling link
`effigy-catalog-pack` resolves to the canonical pack checkout. Orientation
checks run: git identity/status/worktree checks, Effigy crate and pack-repo
script surveys, byte diffs of both trees (41 pre-existing files already
byte-identical; only `pack.toml` was missing).

## Deterministic Offline Reproduction (design proof)

An independent Python reconstruction of the canonical OCI artifact from the
pack tree at the peeled commit plus recorded provenance (created
`2026-09-02T11:49:10Z`, commit `5ef0ec2b…`, tag object `2bb56110…`) produced
content identity `sha256:9498d33f…` and manifest digest
`sha256:91de584e…` — byte-identical to the published facts. This closed the
open tension: the typed lock carries the full annotation provenance, so the
deterministic OCI manifest digest is recomputable offline from the snapshot
alone plus the lock.

## Offline QA Proofs (ordinary QA stays offline)

All below are `cargo test` cases in `crates/effigy-catalog`, which run under
`effigy test`, `effigy qa`, and `cargo test --workspace`; no case touches the
network.

- `committed_snapshot_verifies_against_committed_lock` — the committed
  snapshot proves against the committed lock with pinned markers: 42 files,
  88,600 bytes, content identity `9498d33f…`, manifest digest `91de584e…`.
  The Rust deterministic-manifest rebuild of the committed snapshot equals the
  published digest.
- `committed_snapshot_regeneration_is_deterministic` — composing a fresh lock
  from the committed snapshot plus recorded provenance reproduces the
  committed lock exactly (`compose_baseline_lock` is the regeneration seam).
- Byte drift: `byte_drift_in_snapshot_content_is_rejected` (edited config
  byte) and `added_snapshot_file_is_rejected` (stray file) both fail with
  `ContentIdentityMismatch`.
- Manifest drift: `missing_manifest_is_rejected` and
  `manifest_pack_id_drift_is_rejected`.
- Version drift: `manifest_version_drift_is_rejected_before_hashing` and
  `lock_version_drift_is_rejected_against_snapshot_manifest`.
- Content-identity drift: `lock_content_identity_drift_is_rejected`.
- Lock/manifest-digest drift: `lock_manifest_digest_drift_is_rejected`.
- Lock parse: `committed_lock_rejects_unknown_fields_and_foreign_repository`,
  `unsupported_lock_schema_is_rejected`.
- Digest determinism: `fixture_recomputed_digest_is_stable_and_digest_driven`
  (stable recompute; digest moves when content moves even if the lock is
  stale).

Drift result: `cargo test -p effigy-catalog baseline` — 22 passed, 0 failed
(baseline tests plus focused pack neighbors).

Live tamper demonstrations against the committed files (run 2026-09-02, both
restored immediately afterwards): one digest character edited in the
committed `catalog-pack.lock.toml` failed
`committed_snapshot_verifies_against_committed_lock`; one byte appended to the
committed snapshot's `README.md` failed the same test with
`ContentIdentityMismatch { recorded: sha256:9498d33f…, found:
sha256:d3e5bab4… }`. Each restored file re-passed the test.

## Online Provenance Proof (explicit read-only, one-off)

- Attestation: `gh attestation verify oci://ghcr.io/inflatable-cookie/
  effigy-catalog-pack@sha256:91de584e… --owner inflatable-cookie` passed
  (exit 0): sigstore bundle, predicate `https://slsa.dev/provenance/v1`,
  subject `ghcr.io/inflatable-cookie/effigy-catalog-pack` with digest map
  `{"sha256": "91de584e77487765c24f53abb63413783a99c0a7926c25aee1289a3cf370d9f3"}`
  — exact digest binding.
- Anonymous exact-byte pull (credentials-less token exchange): manifest pulled
  by digest; body sha256 equals the digest; 42 layers whose sorted titles
  match the committed snapshot inventory exactly; every blob matched its
  recorded size and sha256 and was byte-identical to the matching committed
  snapshot file.
- Published manifest annotations agree with the lock: content-id
  `9498d33f…`, source-commit `5ef0ec2b…`, source-tag `v1.0.1`, source-tag-object
  `2bb56110…`, created `2026-09-02T11:49:10Z`, version `1.0.1`, source
  `https://github.com/inflatable-cookie/effigy-catalog-pack`; config is the
  fixed empty config (`sha256:44136fa3…`, size 2).

## Behavior And Validation

- `cargo test -p effigy-catalog`: 163 lib + 57 integration + doc tests passed.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.
- Final `cargo test --workspace` through `effigy qa`, plus the `qa:docs` and
  `qa:json` checks: see `## Final Validation Round` below.
- No public command was added; `pack/channel.rs` (placeholder coordinate,
  `published: false`) is untouched; ordinary service/container/system/workspace
  use and bootstrap never contact a registry. Layering, selection,
  fallback/doctor, and assembly integration tests passed unchanged.

## Final Validation Round

- `cargo test --workspace`: full workspace ran green except one
  non-reproducible `effigy --lib` failure observed once under concurrent load;
  two isolated reruns and the `effigy qa` round all passed 1473
  `effigy --lib` tests.
- `effigy qa` task round: passed, including `qa:docs` and `qa:json`. One
  pre-existing flake (`cli_container_attached_session_handles_sigint_during_
  startup`) failed in the first round and in isolation, and was reproduced on
  the clean base with this lane's changes stashed (PAPERCUTS entry added); the
  rerun and final round passed all targets.
- `git diff --check`: clean.

## Mutation Boundary

Card `1106` created no source tag, package, attestation, channel movement, or
Effigy release. The only external reads were the read-only provenance pulls
above and read-only git fetches. Cards `1107` and `1108` remain blocked;
readiness refresh belongs to the orchestrator after this PR merges.

## Review Oracle Mapping

- Reject hand-editable snapshot authority: snapshot is generated (lock header,
  changelog, module docs) and any direct edit fails `cargo test` (drift tests
  above).
- Reject incomplete lock identity: lock records repository, commit, created,
  tag/tag-object, pack id/version, and both distinct identities; parse is
  typed with `deny_unknown_fields` and field validation.
- Reject network in ordinary QA/use: all drift proofs are filesystem-only
  `cargo test` cases; runtime code is untouched.
- Reject content/OCI identity conflation: both facts are recorded and checked
  separately (`content_identity` vs `oci_manifest_digest`), and the digest
  rebuild derives its content-id annotation from the snapshot, so each remains
  independently anchored.
- Reject asset drift: byte, manifest, version, content-identity, and lock
  drift each have dedicated failing counterexamples.
- Reject behavior requiring an installed pack: compiled-baseline resolution,
  list, extract, compose, and fallback tests pass unchanged with the added
  `pack.toml` embedded asset (fragment listing filters on
  `<name>/service.toml`).

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`
- Movement: editable embedded catalog authority -> exact generated snapshot
  with typed provenance lock, offline drift rejection, and deterministic
  manifest identity recompute
- Remaining gap: public `service pack update` cutover (1107) and narrow
  generated-baseline proposal automation (1108), both still blocked on this
  card per sequence

## Next Task

Orchestrator refreshes ready-frontier status for cards `1107` and `1108` after
this PR's accepted review and merge. Effigy release authority remains
separate.
