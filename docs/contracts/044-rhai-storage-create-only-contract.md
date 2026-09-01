# 044 Rhai Storage Create-Only Contract

Status: active
Owner: Rhai object-storage mutation boundary
Created: 2026-09-01
Architecture: [`026`](../architecture/026-feature-placement-and-command-surface.md)
Roadmap: [`g08.044`](../roadmaps/g08/044-rhai-storage-create-only.md)

## Purpose

Let a retained Rhai storage consumer create an object without racing another
writer and overwriting a key that became occupied after a preliminary read.

## Contract

- `storage::put(options)` accepts optional boolean `create_only`.
- Omitted or `false` preserves the current unconditional PutObject behavior.
- `true` sends the provider's atomic create-if-absent condition with the same
  request that carries the bytes. For the current S3 provider this is
  `If-None-Match: *`.
- A key occupied before or during the request is never overwritten. The Rhai
  call fails with a stable diagnostic that names the create-only collision
  without exposing credentials, signed URLs, or response bodies.
- Other provider, bucket, key, body/path, content-type, metadata, response-map,
  and error behavior remains unchanged.
- The option is additive. It adds no implicit HEAD, retry, lock, or fallback to
  an unconditional write.

## Review Oracle

Reject the implementation if any counterexample survives:

1. Two create-only requests for the same absent key can both succeed.
2. A create-only request replaces bytes or metadata already at the key.
3. A precondition failure is retried as an unconditional PutObject.
4. Omitting `create_only` changes the existing request or response behavior.
5. The error leaks a signed URL, authorization material, or response body.
6. The surface catalog or user guide describes a shape the runtime does not
   accept.

## Validation

- focused `effigy-rhai` request-header and collision tests
- a local deterministic HTTP fixture proving one winner and preserved bytes
- existing Rhai storage tests
- `effigy test --plan`
- `effigy qa`
- `effigy doctor`
- `git diff --check`

## Boundary

This contract does not authorize S3 extraction, a new provider abstraction,
general conditional writes, version matching, retries, release work, or a live
consumer/storage mutation.
