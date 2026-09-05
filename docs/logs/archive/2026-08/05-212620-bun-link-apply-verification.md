# Bun Link Apply And Verification

Status: complete
Created: 2026-08-05
Roadmap: `g08.021`
Batch: `1058`

## Summary

Shipped `effigy deps link bun`: exact-precondition application, full-closure
registration and consumer linking, immutable manifest/lock guards, bounded
rollback, canonical symlink verification, drift repair, and text/JSON reports.

## Changes

- held the machine registration-index lock across physical revalidation, Bun
  mutation, immutable-file checks, full verification, and index persistence
- passed explicit `--no-save` to every registration and consumer-link process
- linked the complete consumer closure in one invocation after real Bun
  `1.3.14` proof showed sequential package calls can re-resolve and replace an
  earlier local link
- checked consumer manifests, lockfiles, and local package manifests after
  every manager mutation and failed closed on any byte change
- preserved matching foreign registrations without claiming them and rolled
  back only Effigy-created registrations and consumer links
- verified every consumer symlink resolves to its canonical local package path
- made re-link repair complete link loss without duplicate desired/index state
- exposed exact process intents, outcomes, immutable evidence, verification,
  errors, and rollback through text and `effigy.deps.link.v1` JSON

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: Bun linking stopped at a pure plan -> one command now applies and
  proves a save-less full local closure before recording desired state
- Remaining gap: reversible Bun unlink, shared-registration release, and peer
  diagnostics remain in ready card `1059`

## Validation Performed

- focused Bun apply/precondition/rollback/drift fixtures
  - result: 6 passed
- real Bun `1.3.14` root-only and multi-package integration fixtures
  - result: 2 passed; canonical symlinks observed with no manifest or lockfile
    churn
- focused deps CLI text/JSON/dry-run tests
  - result: 7 passed
- `cargo test -p effigy-deps`
  - result: 58 unit tests, 2 real Bun integration tests, 3 real Cargo
    integration tests, and doc tests passed
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
  - result: passed
- `effigy qa:ci:fast`
  - result: 1,622 tests passed, 1 skipped; 1 leaky test reported
- `effigy qa:ci:json`
  - result: passed; JSON contract selection validated
- `effigy qa:docs`
  - result: passed
- `cargo fmt --all -- --check`
  - result: passed
- `git diff --check`
  - result: passed

## Risks

- Bun links remain ephemeral across installs; re-running link repairs complete
  loss, while mixed local/registry state remains a correctness failure
- safe physical unlink and duplicate peer diagnosis remain deliberately
  unavailable until `1059`

## Next Task

Execute ready batch card `1059`.
