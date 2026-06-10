# Release Prepare Cargo Lock Sync And Parity

Status: complete
Created: 2026-03-11
Roadmap: g01.027
Batch: release-prepare-cargo-lock-sync-and-parity

## Summary

- Added real Cargo-based `release.sync-files` execution for `Cargo.lock`.
- Release prepare plan/apply now surface `Cargo.lock` as a sync-file mutation.
- Added fixture-level parity validation against `scripts/prepare-release.sh --apply`.

## Changes

- Extended release config resolution to recognize `sync-files = ["Cargo.lock"]`
  for Cargo-based release version sources and reject unsupported sync targets.
- Refactored release mutation application so prepare can handle both direct file
  writes and sync-file operations while recording only the files that actually
  changed before writing `.release-prepared.json`.
- Added `cargo check --quiet`-based lockfile syncing during
  `effigy release prepare --yes`, plus planned sync-file preview entries during
  `effigy release prepare --plan` and `effigy release simulate`.
- Updated Effigy’s own root `effigy.toml` to configure `sync-files = ["Cargo.lock"]`.
- Added CLI coverage for planned sync-file mutations, applied Cargo.lock sync,
  and a Cargo-fixture parity test against the legacy prepare script.

## Vision Target Delta

- Primary tags: `RELEASE`, `SELFHOST`, `PARITY`
- Movement: baseline `Effigy could prepare version/changelog changes but ignored configured sync-files and could not replace Cargo.lock regeneration in the existing release flow` -> current `Effigy can now sync Cargo.lock during prepare and proves semantic parity with the legacy prepare script on a Cargo fixture`
- Remaining gap: `exact formatter-preserving parity for Cargo.toml/changelog, tag-install validation migration, and broader real-repo parallel validation remain open`

## Validation Performed

- command: `cargo test --lib current_repo_release_config_matches_self_hosting_gate_script_baseline -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_prepare_ -- --nocapture`
  - result: pass
- command: `cargo fmt --all -- --check`
  - result: pass
- command: `git diff --check`
  - result: pass

## Risks

- `release.sync-files` currently supports `Cargo.lock` only. Additional sync
  targets still need explicit implementation rather than assuming shell parity.
- The parity proof is semantic for changelog/version outcomes and exact for
  `Cargo.lock`; byte-for-byte formatting preservation remains a separate open
  concern tracked by the roadmap’s formatter-preservation items.

## Next Task

- Implement the next meaningful `g01.027` batch by migrating the remaining
  release-gate delta from `scripts/check-release-gates.sh`, specifically the
  optional tag-install validation path, and then decide whether that should be
  modeled as manifest-configurable gates or a separate built-in release
  verification surface.
