# Deps Link Cargo: Version-Transition Lock Refresh and Lock Rollback

Status: open — blocking a consumer pre-release check; awaiting operator go
Created: 2026-09-07
Owner: chatterbox
Source: Swallowtail Chatterbox request (2026-09-07) on behalf of Bovine
Desktop; reproduction is Desktop-side
Contract: [`034`](../contracts/034-local-dependency-linking-contract.md)
Guide: [`077`](../guides/077-local-dependency-linking.md)
Code: `crates/effigy-deps/src/cargo_apply.rs` (`apply_cargo_link_plan`,
`verify_cargo_link`, `rollback_physical_changes`), `cargo.rs` (metadata
invocation)

## Issue

Bovine Desktop pins eight Swallowtail Git packages at tag `v0.4.3`.
`effigy deps link cargo <candidate>` writes `[patch]` entries to local
packages at `0.4.4`, then verifies. Cargo cannot apply a patch to a locked
`0.4.3` Git entry when the patch version differs, so all eight locals land
as `[[patch.unused]]`, metadata still resolves Git, Effigy correctly fails
and rolls back the config, but `Cargo.lock` keeps the unused entries.
Desktop recovered the baseline with `cargo update --workspace`.

## Known (verified in code, 2026-09-07)

- Verification runs `cargo metadata` **unlocked** (`inventory_cargo_consumer_roots(.., false, ..)`),
  so it can and does rewrite `Cargo.lock`; contract `034` acknowledges
  "Cargo verification/builds may rewrite lock entries" as expected
  link-owned drift.
- `rollback_physical_changes` restores only the planned config changes
  (`.cargo/config.toml`, managed blocks). No `Cargo.lock` snapshot exists,
  so a failed link leaves lock drift behind. Contract `034`'s lockfile
  rules cover dirty-lock refusal before link and byte-for-byte return on
  unlink, but say nothing about lock state after a **failed** link.
- Nothing in the plan compares the linked local package version with the
  locked source version, so a version transition is neither detected nor
  refreshed; the patch silently fails to apply.

## Requested (Swallowtail, priority order)

1. When the linked local version differs from the locked Git version,
   refresh the lock as part of link before verification: targeted
   `cargo update -p <pkg>` per patched package (or `--workspace` after
   patching).
2. Transactional lock rollback: snapshot every affected `Cargo.lock` before
   applying and restore it with the config on failure.

## Chatterbox recommendation (not operator-confirmed)

One bounded lane, both items, in the existing apply/verify/rollback seam:

- Plan detects a version transition per package (local `Cargo.toml` version
  versus the locked source version from locked metadata) and records it in
  the plan and report.
- Apply snapshots every affected `Cargo.lock` (tracked bytes) before the
  first write; rollback restores them alongside config, refusing only if
  the lock changed after Effigy's own write (same rule as config).
- For transitioned packages, apply runs `cargo update -p <name>` per
  package (targeted, not `--workspace`, to keep unrelated drift out) inside
  the transaction, then verification runs as today. Any `[[patch.unused]]`
  in metadata is a verification failure with the package names, not a
  silent Git resolution.
- Contract `034` lockfile-safety gains: version-transition refresh is
  link-owned drift; failed link restores affected locks byte-for-byte.
  Guide `077` documents the transition case.
- Unlink already proves byte-for-byte return; a transitioned link must
  still satisfy it (unlink re-resolves the Git source).

## Unknown

- Whether `cargo update -p` on a Git-sourced package with an active path
  patch needs `--precise` or the patch to be present first (ordering:
  write patch, then update); the worker proves it on a fixture with a
  tag-pinned Git dependency and a higher local version.
- Timing: not promised. If the operator confirms today, the last four
  bounded lanes each landed same day.

## Next Task

Operator decides whether to queue it now. On confirmation, Chatterbox
promotes roadmap `g09.008`, strict spec `123`, ready card `1116`, and tells
the Swallowtail Chatterbox the card id.
