# Bun Pin Domain Foundation

Status: complete
Created: 2026-08-11
Roadmap: g08.031
Batch: card-1078-build-bun-pin-planner-and-manifest-transaction

## Summary

- Added typed Bun pin and unpin plans, outcomes, warnings, writes, verification,
  and operation reports inside `effigy-deps`.
- Reused read-only Bun inventory to select the unique named package closure,
  collapse duplicate consumer resolutions, and prefer direct matches.
- Added a byte-preserving root `package.json` transaction with compare-and-swap
  apply, atomic replacement, exact unpin matching, and no public command yet.
- Kept `bun.lock` and `bun.lockb` immutable and exposed the required portability
  warning when a relative `file:` value escapes the consumer checkout.

## Changes

- Pin planning writes every missing selected override or refuses the complete
  plan when one selected entry conflicts.
- Exact re-pin, no-match, and already-unpinned states produce no manifest write.
- Unpin removes only entries whose canonical package path matches the named
  library checkout; same-name values pointing elsewhere survive.
- The JSON editor preserves unrelated bytes, property order, indentation,
  newline style, and final-newline posture. Empty `overrides` objects disappear.
- Apply checks the exact planned manifest bytes and both optional Bun lockfiles
  before writing, then verifies all three after the atomic replacement.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`, `AGENT`
- Movement: baseline `committed Bun pinning existed only as an accepted contract`
  -> current `the domain can safely plan and apply the root-manifest transaction`
- Remaining gap: public CLI/JSON and link interlocks in card `1079`, then
  consumer proof and closeout in card `1080`

## Behavior Evidence

- Sixteen focused `bun_pin` tests cover deterministic full closure, direct and
  transitive collapse, relative paths, conflicts, exact re-pin, exact unpin,
  malformed and duplicate override keys, tabs, CRLF, inline JSON, missing final
  newline, interleaved removal, stale-manifest refusal, injected write failure,
  dry-run, no-match, unreadable immutable-file evidence, and lockfile
  concurrency.
- The apply fixture asserts exact text `bun.lock` and binary `bun.lockb` bytes
  before and after a successful manifest write.
- The affected analysis reached the Rust test surface and repository QA tasks;
  the focused crate board covers the owning domain. No CLI, runner, help,
  schema, workflow, release, library, or intermediate-repository mutation was
  added.
- The god-file scan reports no finding for either new `bun_pin` source module.

## Validation Performed

- `cargo test -p effigy-deps --no-fail-fast`
  - result: pass, 88 unit tests, 2 real Bun integration tests, 4 real Cargo
    integration tests, and doc tests
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
  - result: pass
- `cargo fmt --all -- --check`
  - result: pass
- changed-file `effigy graph affected --stdin --json`
  - result: pass; Rust tests and repository QA selected
- `effigy scan god-files --json`
  - result: pass for the new modules; no new finding
- `effigy qa:docs`
  - result: pass
- Swallowtail `effigy docs check index --policy-index roadmaps` and
  `effigy docs check next-action --policy roadmaps`
  - result: pass; the earlier plain-relative-index-link behavior remains green
- `git diff --check`
  - result: pass

## Risks

- The domain API is intentionally unavailable to operators until card `1079`
  wires grammar, rendering, JSON schema, root resolution, and link interlocks.
- The generated relative path is portable only when teammates and CI reproduce
  the checkout topology. The plan reports this when it escapes the consumer.

## Next Task

Execute ready card
[`1079`](../../roadmaps/g08/batch-cards/1079-wire-bun-pin-cli-json-and-link-interlocks.md).
