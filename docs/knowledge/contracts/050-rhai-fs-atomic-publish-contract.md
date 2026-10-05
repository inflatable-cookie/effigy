# 050 Rhai Filesystem Atomic Publish Contract

Owner: Rhai filesystem mutation boundary
Created: 2026-09-29
Architecture: [`026`](../architecture/026-feature-placement-and-command-surface.md)

## Purpose

Let a Rhai script publish a complete file payload to an absent destination
without a concurrent reader observing a partial winner, and without a losing
publisher overwriting the winner.

## Contract

- `fs::write_file_if_absent(path, contents)` publishes `contents` as a UTF-8
  file at `path` only when no entry currently names `path`.
- `fs::copy_if_missing(source, destination)` publishes a complete independent
  copy of `source` at `destination` only when no entry currently names
  `destination`. Source bytes are streamed into the staged file; the helper
  never loads the whole payload into memory solely to stage it, and never
  hard-links the source inode to the destination. A later write to `source`
  cannot change a published copy. Permission bits follow the source, matching
  the documented `fs::copy` metadata behavior.
- The payload is staged as a fresh file in the destination's directory so it
  shares the destination's filesystem, then published through the filesystem's
  atomic no-clobber link operation. The destination name appears only after the
  whole payload is written.
- The call returns `true` when it published the payload and `false` when the
  destination was already occupied. Exactly one concurrent publisher wins; a
  losing publisher returns `false` and never alters the winner. `copy_if_missing`
  reports `true` only when its complete copy is the published destination.
  Occupancy is decided from the destination name before the source is opened,
  so an occupied destination still returns `false` when the source is missing
  or unreadable.
- An existing destination counts as occupied whether it is a regular file, a
  directory, or a symlink, including a dangling symlink. Publication never
  follows or replaces a symlink destination.
- Missing parent directories are created, matching `fs::write_file` and the
  other write helpers.
- A failed staged write, a failed staged copy, or a failed link removes the
  staged file and leaves the destination absent.
- A staged file may be briefly visible in the destination directory under a
  reserved `.<name>.effigy-publish-...tmp` name and is removed after a
  successful or colliding publication. A process killed mid-publication can
  leave a staged file behind; that file is not the destination payload.
- The contract covers visibility atomicity, not durability. The helper does not
  `fsync` the payload or its directory.
- Filesystems that cannot hard-link fail with the underlying OS error. The
  helper never falls back to a non-atomic write or an overwriting copy.
- `fs::copy(source, destination)` and `fs::move_path(source, destination)` keep
  their existing behavior. `copy` replaces an existing destination. `move_path`
  delegates to `std::fs::rename`, atomically replaces whatever currently names
  `destination`, and performs no identity check. Neither is a conditional or
  create-if-absent operation.

## Review Oracle

Reject the implementation if any counterexample survives:

1. Two publishers of the same absent destination both report success, or both
   payloads can be observed at the destination.
2. A reader observes bytes at the destination before the winning payload is
   complete.
3. A losing publisher replaces or truncates the winner, including a
   `copy_if_missing` caller that observed absence then copied after another
   writer published.
4. An occupied destination, including a dangling symlink, is replaced, or
   `copy_if_missing` errors on a missing or unreadable source instead of
   returning `false` for that occupied name.
5. A failed staged write, staged copy, or link leaves a destination or staged
   file behind.
6. The helper silently falls back to a non-atomic write or overwriting copy on
   a filesystem that cannot publish atomically.
7. `copy_if_missing` aliases the source inode, so a later source write changes
   the published destination.
8. The surface catalog or user guide describes a shape or guarantee the runtime
   does not provide, or claims identity-checked semantics for `move_path`, or
   claims create-if-absent semantics for `copy`.

## Validation

- focused `effigy-rhai` publish, collision, symlink, and failure tests
- a concurrent-publisher test proving one complete winner and no partial read
- `test:rhai:copy-if-missing` and `check:rhai:copy-if-missing`
- existing Rhai filesystem tests
- `effigy test --plan`
- `effigy qa`
- `effigy doctor`
- `git diff --check`

## Boundary

This contract does not authorize conditional or identity-checked rename
(`renameat2`, `renamex_np`), durable/fsync publication, cross-filesystem
staging, a general transaction or locking API, a `copy` or `move_path`
behavior change, or release work.
