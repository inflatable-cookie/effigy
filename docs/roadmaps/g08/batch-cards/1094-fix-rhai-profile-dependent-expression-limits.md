# 1094 - Fix Rhai Profile-Dependent Expression Limits

Roadmap: [`../039-rhai-profile-independent-limits-papercut.md`](../039-rhai-profile-independent-limits-papercut.md)
Spec: [`../../../specs/112-rhai-profile-independent-limits-strict-lane.md`](../../../specs/112-rhai-profile-independent-limits-strict-lane.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md)
Guide: [`../../../guides/061-rhai-script-steps-guide.md`](../../../guides/061-rhai-script-steps-guide.md)
Papercut: [`PAPERCUTS.md`](../../../../PAPERCUTS.md)

Status: Ready
Owner: `effigy-rhai` engine construction and first-party script policy
Created: 2026-09-01
Ready since: 2026-09-01 papercut triage on current `main`

## Purpose

Remove build-profile-dependent Rhai parsing while preserving the current
release support envelope and finite safety bounds.

## Observed Failure

Rhai defaults function-body expression depth to `16` in debug builds and `32`
in release builds. A first-party benchmark script parsed through the release
binary and failed through the documented `cargo run --bin effigy -- <task>`
fallback with `Expression exceeds maximum complexity`.

## Work

- centralize Effigy's Rhai engine construction if necessary so every script
  execution route receives the same parser limits
- set exact profile-independent expression limits: global `64`, function `32`
- retain all unrelated Rhai limits and host registrations
- add a non-vacuous fixture whose function expression exceeds the stock debug
  limit but remains within `32`
- prove an expression above `32` is still rejected
- add or extend first-party script compilation policy coverage where it gives a
  stable recurrence guard
- update guide `061`, `CHANGELOG.md`, the selected papercut, and one evidence
  log; close this roadmap/spec/card and return the queue to catalog-pack design

## Acceptance

- [ ] all production Rhai execution routes use the explicitly configured engine
- [ ] `max_expr_depth()` is `64` and `max_function_expr_depth()` is `32` in
      both debug and release test builds
- [ ] the over-16/within-32 function fixture parses and executes
- [ ] an over-32 function fixture fails with the parser complexity guard
- [ ] current first-party Rhai scripts still compile or execute under the host
- [ ] no call-stack, operation, collection, module, host API, S3, or CLI surface
      changes
- [ ] focused debug and release tests, `effigy qa`, fmt, clippy, and diff checks
      pass
- [ ] papercut and all planning/front-door closeout state are honest

## Review Oracle

Falsify these counterexamples before PR creation:

1. A function expression with depth greater than `16` but no greater than `32`
   still fails under a debug Effigy build.
2. The same configured engine reports a different expression limit under
   `cargo test` and `cargo test --release`.
3. Setting explicit limits accidentally makes expression depth unlimited; an
   over-32 function expression compiles.
4. One production execution path constructs a raw `Engine::new()` and bypasses
   the shared limits.
5. The docs-context benchmark or another checked-in first-party `.rhai` script
   stops parsing after the engine change.

## Validation

- focused `cargo test -p effigy-rhai` tests for engine limits and adversarial
  scripts
- matching focused `cargo test --release -p effigy-rhai` proof
- `effigy perf:docs-context-benchmark`
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Write one dated closeout log mapping the five oracle rows to exact tests or
commands, recording debug/release results, benchmark proof, full validation,
the papercut closure, and the return to catalog-pack acquisition planning.

## Stop Conditions

Stop if the fix needs configurable public limits, relaxed or unlimited runtime
guards, a call-stack or execution-budget change, Rhai/S3 extraction, a manifest
or CLI change, consumer migration, release work, or a graph/catalog-pack change.

## Next Task

Implement this card in the dispatched worker lane.
