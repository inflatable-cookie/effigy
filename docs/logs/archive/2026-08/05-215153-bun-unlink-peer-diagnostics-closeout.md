# Bun Unlink And Peer Diagnostics Closeout

Status: complete
Created: 2026-08-05
Roadmap: g08.021
Batch: 1059

## Summary

- completed reversible Bun unlink for exact full consumer closures
- corrected the mechanism boundary: remove exact consumer symlinks directly;
  use package-directory `bun unlink --no-save` only for owned registration
  release
- added shared/local/missing/duplicate peer resolution diagnostics with exact
  paths and dedupe remediation

## Changes

- revalidate ledger, registration index, immutable files, registrations, and
  consumer symlinks under the registration lock before mutation
- preserve foreign, shared, stale, conflicting, and unverifiable registrations
- restore registrations and the complete consumer closure after failed apply
- expose Bun unlink plans, outcomes, verification, retention, errors, and
  rollback through text and `effigy.deps.unlink.v1` JSON
- prove root-only and multi-package edit/link/unlink/re-link round trips against
  Bun `1.3.14` without manifest or lockfile churn

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: baseline save-less Bun link apply without reversible unlink or peer
  safety -> current exact unlink, registration ownership release, rollback, and
  duplicate-peer diagnosis
- Remaining gap: shared status/doctor health integration under `g08.022`

## Validation Performed

- `cargo test -p effigy-deps`: 65 unit, 2 real Bun integration, 3 Cargo
  integration, and doc tests passed
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`: passed
- `cargo clippy --bin effigy --all-targets -- -D warnings`: passed
- `effigy qa:ci:fast`: 1,623 tests passed; JSON contracts passed
- `effigy qa:ci:json`: 25 selected command contracts passed
- `effigy qa:docs`: links, examples, indexes, policy, and next-action checks
  passed

## Risks

- Bun consumer symlinks remain ephemeral after `bun install`; status and doctor
  drift reporting is the next milestone
- framework peer layouts can still require consumer-level hoisting or dedupe;
  duplicate physical resolutions now fail verification with both paths

## Next Task

- Execute ready card `1060` under `g08.022`.
