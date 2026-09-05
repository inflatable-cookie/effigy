# Cargo Unlink And Milestone Closeout

Status: complete
Created: 2026-08-05
Roadmap: `g08.020`
Batch: `1056`

## Summary

Shipped `effigy deps unlink cargo`: selected owned patches are removed, Cargo
re-resolves the persisted exact Git closure, tracked locks recover cleanly or
report remaining-link-only state, and unrelated drift is refused before any
write. The Cargo milestone is complete.

## Changes

- persisted exact workspace/package/Git-source tuples in repo-local desired
  state so unlink does not depend on a live local checkout
- applied exact config and ledger deltas with ownership-bounded empty-file and
  empty-directory cleanup
- changed managed Cargo output to real `[patch."<url>"]` tables so patches
  remain root-level after arbitrary foreign TOML tables
- re-ran Cargo metadata and tree after patch removal and required every former
  local crate to resolve from its exact committed Git source
- compared tracked locks with `HEAD`, ignored only package entries owned by
  active desired links, and rejected unrelated drift before the first write
- allowed multiple local libraries while preserving the first link's expected
  lock state; unlink reports remaining active-link drift explicitly
- added dry-run, no-op, unlinked, apply-failed, and verification-failed reports
  with `effigy.deps.unlink.v1` text/JSON exposure
- proved local edit visibility and clean remote recovery in real flat and
  nested Cargo consumers
- proved selected unlink preserves foreign config, unrelated `.cargo` files,
  and another active library byte-for-byte
- closed `g08.020`, activated `g08.021`, and readied Bun planning card `1057`

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: Cargo local linking required manual recovery -> link and unlink are
  now reversible, source-verified operations with owned lock-drift guards
- Remaining gap: Bun save-less link planning and mutation remain in `g08.021`

## Validation Performed

- `cargo test -p effigy-deps`
  - result: 44 unit tests, 3 real Cargo integration tests, and doc tests passed
- real flat and nested link/edit/unlink fixtures
  - result: passed; local edits compiled while linked, exact Git sources and
    tracked locks recovered after unlink
- multiple-library/foreign-state fixture
  - result: passed; selected unlink retained the other link and foreign files;
    unrelated lock drift was refused without config or ledger writes
- focused deps CLI text/JSON/no-op tests
  - result: passed
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
  - result: passed
- `effigy qa:ci:json`
  - result: passed; `effigy.deps.unlink.v1` selected and validated
- `effigy qa:ci:fast`
  - result: passed after updating the Cargo-operation help expectation
- `effigy qa:docs`
  - result: passed
- `cargo fmt --all -- --check`
  - result: passed
- `git diff --check`
  - result: passed

## Risks

- a missing local checkout can be removed from owned state, but committed Git
  re-resolution still requires Cargo to reach or already cache the remote
  source
- lock comparison deliberately permits only package tables named by active
  desired links; other lock changes must be resolved before mutation

## Next Task

Execute ready Bun planning card `1057`.
