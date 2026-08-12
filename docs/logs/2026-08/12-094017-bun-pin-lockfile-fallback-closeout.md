# Bun Pin Lockfile Fallback Closeout

Status: complete
Created: 2026-08-12
Roadmap: g08.031
Batch: bun-pin-lockfile-fallback

## Summary

- added a pin-only text `bun.lock` fallback after `bun pm ls --all` process
  failure
- kept Bun link inventory process-authoritative
- added explicit fallback warnings and fail-closed JSONC validation
- proved the fix against five blocked consumers and one working control

## Changes

- `effigy-deps` parses Bun text locks with `jsonc-parser` `0.33.1`
- package identity comes from each record's first package specifier, not nested
  lock keys
- missing, malformed, non-object, or unidentifiable package records refuse
  without a manifest write
- text and JSON render `lockfile-enumeration-fallback` with the original Bun
  failure and selected lock path
- changelog, guide `077`, JSON example, contract `040`, and the strict spine
  now describe the behavior

## Consumer Proof

All commands used the current source binary, leading `--repo`, `--json`, and
`--dry-run`. Package manifests, `bun.lock`, `bun.lockb`, and Git status were
unchanged before and after every command.

| Consumer | Inventory | Outcome | Complete Poodle plan |
| --- | --- | --- | --- |
| contact-patch/cp-admin | text-lock fallback | dry-run | bridge-underlay add; Core and Svelte already applied |
| compli-me/front | text-lock fallback | dry-run | bridge-underlay add; Core and Svelte already applied |
| songsprout/bloom | text-lock fallback | dry-run | bridge-underlay add; Core and Svelte already applied |
| songsprout/greenhouse | text-lock fallback | dry-run | bridge-underlay add; Core and Svelte already applied |
| acowtancy/cream | text-lock fallback | dry-run | bridge-underlay add; Core and Svelte already applied |
| contact-patch/cp-front | `bun pm ls --all` | already-applied | Core and Svelte already applied |

The five hand-written override blocks were not the complete locked closure:
each affected lock also contains `@inflatable-cookie/poodle-bridge-underlay`.
The fallback correctly preserved full-closure planning instead of treating the
manual state as complete.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`, `AGENT`
- Movement: five real consumers could not use committed pinning -> all five
  now receive a complete warning-bearing no-write plan
- Remaining gap: None

## Validation Performed

- `cargo test -p effigy-deps`
  - result: 99 tests passed, including 93 unit and 6 real link integration
    tests
- focused runner pin tests
  - result: 3 passed
- `effigy qa:docs`
  - result: pass, including the plain-relative index-link regression
- `effigy qa:json`
  - result: pass; `effigy.deps.pin.v1` remains schema v1
- `cargo fmt --all -- --check`
  - result: pass
- `cargo clippy --all-targets -- -D warnings`
  - result: pass
- `effigy qa`
  - result: 3,261 tests passed, 1 skipped; docs and JSON contracts passed
- `git diff --check`
  - result: pass

## Risks

- Bun may change the text-lock package-record grammar; unsupported structure
  fails closed and reports the affected record
- fallback inventory is declared resolution, so it remains intentionally
  unavailable to physical link/status safety decisions

## Next Task

Await operator intent. No ready strict card or release action is implied.
