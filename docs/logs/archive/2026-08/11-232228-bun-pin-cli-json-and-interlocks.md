# Bun Pin CLI, JSON, And Interlocks

Status: complete
Created: 2026-08-11
Roadmap: g08.031
Batch: card-1079-wire-bun-pin-cli-json-and-link-interlocks

## Summary

- Added public `deps pin bun` and `deps unpin bun` grammar with dry-run,
  selected-repo path resolution, standard leading globals, and direct Cargo
  rejection.
- Added deterministic committed-state text output and
  `effigy.deps.pin.v1` JSON for pin and unpin.
- Refused pin when the consumer carries overlapping Effigy-managed Bun link
  state. Refused link when a canonical committed override already selects a
  local package.
- Kept link save-less, pin manifest-owned, unpin link-free, install
  operator-owned, and every intermediate repository outside the mutation
  boundary.

## Changes

- CLI help and the built-in task description now distinguish machine-local
  links from committed Bun pins.
- Reports include operation, manager, roots, manifest, dry-run, outcome,
  package actions, actual writes, warnings, verification, errors, and next
  actions. Applied manifest changes point to a separate `bun install`.
- `pin cargo` and `unpin cargo` fail before any Cargo patch planning.
- Matching committed overrides produce `committed-pin-active` through the
  existing link schema with no process, state, registration, or manifest
  mutation.
- The new schema is registered in the contract index and documented with a
  validated example.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`, `AGENT`
- Movement: baseline `safe committed pinning existed only as a domain API` ->
  current `operators and automation have explicit command, JSON, and overlap
  contracts`
- Remaining gap: disposable Soundcheck/Poodle proof, public workflow guidance,
  full QA, and lane closeout in card `1080`

## Behavior Evidence

- Twenty-nine focused Effigy tests cover grammar, help, Cargo rejection,
  relative `--repo` resolution, text/JSON parity, install next actions, and the
  committed-pin link interlock.
- Ninety `effigy-deps` unit tests cover the domain transaction and both overlap
  directions. Six real manager integration tests remain green.
- The schema checker selected `effigy.deps.pin.v1` and validated its command.
  Existing dependency link/unlink schemas stayed selected and green.
- Swallowtail's exact roadmap index and next-action policy checks still pass,
  preserving plain relative index-link behavior.
- Affected analysis selected Rust tests plus repository JSON, docs, released
  surface, and QA tasks. Neither new pin module triggers the god-file scan.

## Validation Performed

- `cargo test -p effigy deps_ --no-fail-fast`
  - result: pass, 29 focused tests
- `cargo test -p effigy-deps --no-fail-fast`
  - result: pass, 90 unit tests, 2 real Bun integration tests, 4 real Cargo
    integration tests, and doc tests
- `effigy qa:json`
  - result: pass; `effigy.deps.pin.v1` selected and validated
- `effigy qa:docs`
  - result: pass
- `effigy qa:released-surface`
  - result: pass, 9 tests
- focused CLI task rendering and completion-surface tests
  - result: pass, 15 tests
- focused workspace Clippy for `effigy`, `effigy-deps`, `effigy-cli`,
  `effigy-contracts`, and `effigy-core`
  - result: pass with warnings denied
- `cargo fmt --all -- --check`
  - result: pass
- changed-file `effigy graph affected --stdin --json`
  - result: pass; Rust, JSON, docs, released-surface, and QA coverage selected
- `effigy scan god-files --json`
  - result: no finding for the new pin modules
- Swallowtail `effigy docs check index --policy-index roadmaps` and
  `effigy docs check next-action --policy roadmaps`
  - result: pass
- `git diff --check`
  - result: pass

## Risks

- The command contract is green, but end-to-end package identity after an
  operator-run install has not yet been proved against the motivating
  multi-repository graph. Card `1080` owns that proof.
- Relative pins that escape the consumer require CI and teammates to reproduce
  the same checkout topology; reports retain the portability warning.

## Next Task

Execute ready card
[`1080`](../../roadmaps/g08/batch-cards/1080-prove-bun-pin-consumer-workflow-and-closeout.md).
