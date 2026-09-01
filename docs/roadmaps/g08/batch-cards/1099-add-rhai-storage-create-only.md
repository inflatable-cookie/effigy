# 1099 - Add Rhai Storage Create-Only

Roadmap: [`../044-rhai-storage-create-only.md`](../044-rhai-storage-create-only.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/044-rhai-storage-create-only-contract.md`](../../../contracts/044-rhai-storage-create-only-contract.md)

Status: Complete
Owner: `crates/effigy-rhai` storage host surface
Created: 2026-09-01
Ready since: 2026-09-01 Bovine consumer collision proof
Completed: 2026-09-01
Evidence: [`../../../logs/2026-09/01-182838-rhai-storage-create-only-1099.md`](../../../logs/2026-09/01-182838-rhai-storage-create-only-1099.md)

## Purpose

Make an absent-key assertion atomic with the object write exposed to Rhai.

## Observed Failure

The current surface exposes `head` and unconditional `put`. A consumer can
check for absence, but another writer can occupy the key before the PUT. Both
writers can therefore pass the check and one overwrites the other.

## Work

- reproduce the two-writer check-then-put race with a deterministic local HTTP
  fixture
- parse optional boolean `create_only` on `storage::put`
- attach S3 `If-None-Match: *` to the same PutObject request when true
- map occupied-key/precondition failure to one stable redacted diagnostic
- retain the existing omitted/false request and response contract
- update the Rhai surface inventory, focused guide, changelog, and one dated
  evidence log

## Acceptance

- [x] two same-key create-only requests yield exactly one successful write
- [x] winner bytes and checksum metadata remain after the loser returns
- [x] an already occupied key refuses without mutation
- [x] no HEAD, retry, lock, or unconditional fallback is introduced
- [x] omitted and false retain existing behavior
- [x] errors contain no signed URL, credential material, or response body
- [x] catalog, docs, changelog, and runtime use the same option spelling
- [x] focused storage tests, `effigy test --plan`, `effigy qa`,
      `effigy doctor`, and `git diff --check` pass

## Review Oracle

1. Drive two requests against a fixture that atomically records the first
   same-key conditional PUT and returns a precondition failure to the second.
   Exactly one call may report success.
2. Seed the key first, call create-only with different bytes/metadata, and
   prove the seeded object remains unchanged.
3. Run an ordinary PUT over an occupied key and prove existing replacement
   behavior remains.
4. Capture the create-only request and prove the condition is on that PUT, not
   implemented by a preliminary HEAD.
5. Return a hostile precondition response containing a signed query and secret-
   shaped body; prove neither appears in the Rhai diagnostic.

## Validation

- focused `cargo test -p effigy-rhai storage`
- repository-owned Rhai surface/docs checks selected by changed-file impact
- `effigy test --plan`
- `effigy qa`
- `effigy doctor`
- `git diff --check`

## Evidence Requirement

Write one dated log mapping each oracle row to exact proof and recording the
consumer unblock. Do not claim Bovine PR 32 is repaired or merged.

## Stop Conditions

Stop if the condition cannot be attached atomically to PutObject, if stable
redacted collision classification requires exposing provider secrets, if the
change expands to general conditional/versioned writes or a provider framework,
or if proof requires live storage.

## Next Task

Return the PR for exact-head orchestrator review. After merge, the orchestrator
resumes Bovine PR 32 on its preserved worker identity.
