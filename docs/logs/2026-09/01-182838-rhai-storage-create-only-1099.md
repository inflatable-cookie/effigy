# Rhai Storage Create-Only 1099 Closeout

Status: complete
Created: 2026-09-01
Roadmap: g08.044
Batch: 1099-add-rhai-storage-create-only
Contract: `docs/contracts/044-rhai-storage-create-only-contract.md`
Spec: `docs/specs/archive/114-rhai-storage-create-only-strict-lane.md` (archived in this batch)
Handoff: `20260901-165314-rhai-storage-create-only-worker.md`

## Summary

- `storage::put` accepts an optional boolean `create_only`. When true, the
  request carries S3 `If-None-Match: *` on the same PutObject call that carries
  the bytes. No preliminary HEAD, retry, lock, or unconditional fallback is
  added. Omitted or false preserves the previous unconditional request and
  response behavior byte-for-byte.
- A create-only PUT that fails with HTTP 412 surfaces one stable diagnostic,
  `storage::put create_only failed: key "<key>" already exists in bucket
  "<bucket>"`, interpolated only with caller-supplied bucket and key. The
  diagnostic is fixed text; it never renders provider error fields.
- Non-412 failures on a create-only request keep the existing error rendering.
  The vendored client's `Error::Display` may include provider-supplied code,
  message, request id, and host id for those paths; that behavior is unchanged
  by this batch and out of card `1099` scope.
- Contract `044` surfaces touched: `crates/effigy-rhai` storage host
  (`src/host_api/storage.rs`), storage tests (`src/tests/storage.rs`), surface
  catalog description (`src/surface.rs`), focused guide section
  (`docs/guides/068-rhai-host-surface-audit.md`), changelog, this evidence log,
  and card/roadmap/spec closeout. Strict spec `114` is archived into
  `docs/specs/archive/` as part of the same batch.

## Reproduction

`execute_rhai_script_unconditional_head_then_put_lets_second_writer_replace_winner`
runs a deterministic local TCP fixture that serves two writers in race order:
both `storage::head` checks return 404 (both writers observe absence before
either write lands), then both unconditional PUTs are accepted. The fixture
records request bodies and proves the second writer's bytes (`loser-bytes`)
replace the first writer's bytes (`winner-bytes`) with both calls reporting
success. The same test asserts neither PUT carries an `If-None-Match` header.
This is the failure Bovine PR 32 proved at its exact head
`7cb06ed5dd78c0e9a87213a2092c7c47257f1c19`; the test keeps it documented
against future regression.

## Review oracle → proof

Card `1099` oracle rows, falsified by committed tests in
`crates/effigy-rhai/src/tests/storage.rs`:

1. Two create-only requests for the same absent key can both succeed —
   falsified by `execute_rhai_script_create_only_yields_exactly_one_winner_and_redacts_collision`.
   The fixture atomically records the first conditional PUT (200 with ETag) and
   returns a precondition failure to the second. The winner script succeeds;
   the loser script fails with the stable diagnostic. The request log has
   exactly two PUTs.
2. A create-only request replaces bytes or metadata already at the key —
   falsified by `execute_rhai_script_create_only_over_occupied_key_refuses_without_mutation`.
   The fixture seeds the key with an ordinary PUT, refuses the attacker's
   create-only PUT, and the recorded object still holds the seeded body and
   `x-amz-meta-writer: seed` metadata after the loser returns.
3. A precondition failure is retried as an unconditional PutObject — falsified
   by the request-log length assertions in rows 1 and 2 (exactly one PUT per
   create-only call; no second request exists) and by the single `send()` in
   `storage_put` with no retry path.
4. Omitting `create_only` changes existing request or response behavior —
   falsified by the reproduction test (unconditional head-then-put still
   last-write-wins with no precondition header), by
   `execute_rhai_script_ordinary_put_over_occupied_key_still_replaces`
   (seed put with omitted option and replacement put with `create_only: false`
   both keep the replacement contract and response map), and by the pre-existing
   `execute_rhai_script_routes_storage_operations_through_s3_adapter` full
   contract test.
5. The error leaks a signed URL, authorization material, or response body —
   falsified by `fixture_precondition_failed_response`, a hostile 412 whose
   body and headers carry `X-Amz-Signature=...`, `AKIAIOSFODNN7EXAMPLE`,
   a secret-key-shaped string, and hostile request/host ids.
   `assert_no_hostile_material` proves none of them appear in the Rhai
   diagnostic, and the diagnostic matches the fixed text.
6. The surface catalog or user guide describes a shape the runtime does not
   accept — falsified by agreement checks: the surface catalog description in
   `crates/effigy-rhai/src/surface.rs`, the guide section in
   `docs/guides/068-rhai-host-surface-audit.md`, the changelog entry, and the
   runtime all use the same `create_only` spelling; the runtime accepts the
   boolean (proven by rows 1-2) and rejects non-boolean values
   (`execute_rhai_script_rejects_non_bool_create_only`).

Additional proof: `execute_rhai_script_create_only_sends_condition_on_the_put_itself`
captures the single request a create-only call emits and asserts it is a PUT
carrying `if-none-match: *`, with no HEAD — the condition rides the write.

## Consumer unblock

The option closes Effigy's side of the collision boundary that paused Bovine
PR 32: Bovine can now express create-if-absent through `storage::put` without
check-then-write. Bovine PR 32 is not repaired or merged by this batch; it
remains paused and separately owned by the Bovine orchestrator, and resumption
is the post-merge step in the roadmap `Next Task`.

## Validation

All commands run from the clean worker worktree
`worker/rhai-storage-create-only` at the exact head of the batch:

- focused `cargo test -p effigy-rhai storage` — 9 passed, 0 failed
- full board `effigy test` (cargo nextest, full workspace) — 3649 passed,
  1 skipped, re-run green after the final source edit
- `effigy qa` — test suite, docs checks, and JSON contract checks all ok
- `effigy doctor` — summary ok:18 warn:2 err:0 (warnings: pre-existing
  god-file scan findings and an on-demand graph index refresh notice; both
  non-blocking and not introduced by this batch)
- `cargo fmt --all -- --check` — clean (formatter applied to new code)
- `cargo clippy -p effigy-rhai --all-targets -- -D warnings` — clean
- `git diff --check` — clean

## Vision Target Delta

- Primary vision tags: CONTRACT (contract `044` implemented), OPERATE
  (retained consumer surface repaired), MAINT (surface inventory, guide,
  changelog, and evidence refreshed).
- Baseline -> current: Rhai `storage::put` was unconditional only; it now
  carries an additive atomic create-if-absent option with a stable redacted
  collision diagnostic.
- Remaining open: Bovine PR 32 resumption after merge; S3 placement/removal
  decisions stay gated by contract `043` and are untouched here.
