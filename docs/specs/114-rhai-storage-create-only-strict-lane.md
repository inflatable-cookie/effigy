# 114 Rhai Storage Create-Only Strict Lane

Status: ready
Owner: Effigy orchestrator
Created: 2026-09-01
Roadmap: [`g08.044`](../roadmaps/g08/044-rhai-storage-create-only.md)
Contract: [`044`](../contracts/044-rhai-storage-create-only-contract.md)

## Outcome

Expose one atomic create-if-absent option through the retained Rhai S3 write
surface so Bovine can close its collision boundary without check-then-write.

## Evidence

Bovine PR 32 stopped at exact head
`7cb06ed5dd78c0e9a87213a2092c7c47257f1c19`: `storage::head` followed by
unconditional `storage::put` cannot prevent two writers from both observing an
absent key. The vendored client supports `if_none_match("*")`; the Rhai wrapper
does not expose it.

## Decisions

- Public spelling: optional boolean `create_only` on `storage::put`.
- Provider behavior: bind `If-None-Match: *` to the PutObject request.
- Collision behavior: one stable redacted Rhai error; no fallback write.
- Compatibility: omitted/false behavior is byte-for-byte and request-for-
  request unchanged.

## Runway

- [`1099`](../roadmaps/g08/batch-cards/1099-add-rhai-storage-create-only.md)

## Stop Conditions

Stop if the vendored client cannot attach the condition to the same request,
the error cannot be made stable without weakening redaction, the change needs
a new provider abstraction, or validation requires live object storage.

## Continuation

After merge, resume Bovine PR 32 on its preserved worker, update it to use the
new option, and return Effigy's queue to official catalog-pack publication
planning.
