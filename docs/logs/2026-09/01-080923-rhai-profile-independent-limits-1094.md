# Rhai Profile-Independent Limits Closeout

Status: complete
Created: 2026-09-01
Roadmap: g08.039
Card: 1094
Spec: 112 (archived)
Guide: 061
Papercut: Rhai scripts can parse in release and fail in debug
Predecessor evidence: [`01-075717-rhai-profile-limits-papercut-planning.md`](./01-075717-rhai-profile-limits-papercut-planning.md)

## Summary

- every production Rhai host now builds through `configured_rhai_engine()`
- expression depths are explicit host constants: global `64`, function `32`
- debug and release focused suites report the same limits
- an over-stock-debug / within-32 function fixture parses and runs
- an over-32 function fixture still fails with the parser complexity guard
- first-party `.rhai` scripts compile under the configured engine
- the docs-context benchmark runs through the fixed host
- papercut, card, roadmap, and strict spec are closed; queue returns to
  catalog-pack acquisition planning under contract `043`

## Implementation Shape

`crates/effigy-rhai/src/lib.rs` adds `RHAI_MAX_EXPR_DEPTH` (`64`),
`RHAI_MAX_FUNCTION_EXPR_DEPTH` (`32`), and `configured_rhai_engine()`. The only
production `Engine` construction site is `execute_rhai_script_inner`, which now
calls that seam before module resolver and host API registration.

Focused proof lives in `crates/effigy-rhai/src/tests/engine_limits.rs`:

- left-associative `0 + 1 + ... + N` chains inside a function body
- `N=20` exceeds Rhai's stock debug function depth (`16`) and stays inside
  Effigy's explicit envelope (`32`)
- `N=40` exceeds the Effigy function limit and must fail

Guide `061` documents the shared envelope. `CHANGELOG.md` records the fix under
`[Unreleased] / Fixed`.

## Review Oracle

| # | Counterexample | Proof |
| --- | --- | --- |
| 1 | a function expression above `16` and within `32` still fails under debug Effigy | `function_expression_above_stock_debug_limit_runs_on_configured_engine` — debug asserts stock default is `16` and the `N=20` fixture fails on a raw engine, then succeeds through `execute_rhai_script` |
| 2 | configured limits differ between `cargo test` and `cargo test --release` | `configured_engine_reports_profile_independent_expression_limits` asserts `64` / `32` in both profiles; both focused suites passed |
| 3 | explicit limits accidentally become unlimited; over-32 compiles | `function_expression_above_effigy_limit_is_rejected` — `N=40` fails compile and runtime with `Expression exceeds maximum complexity` |
| 4 | a production path still constructs raw `Engine::new()` | repo-wide `Engine::new(` search finds only `configured_rhai_engine`; `execute_rhai_script_inner` is the sole production construction site |
| 5 | docs-context benchmark or another first-party `.rhai` stops parsing | `first_party_rhai_scripts_compile_on_configured_engine`; `effigy perf:docs-context-benchmark` |

## Vision Target Delta

- Primary tags: `MAINT`, `OPERATE`
- Movement: profile-dependent Rhai parser defaults -> one explicit finite
  envelope shared by debug and release hosts
- Remaining gap: five other Effigy papercuts remain open; catalog-pack
  acquisition under contract `043` is the next planning checkpoint; graph
  timeout/progress remains a planning question

## Validation Performed

| Check | Result |
| --- | --- |
| `cargo test -p effigy-rhai engine_limits` | passed (4/4) |
| `cargo test --release -p effigy-rhai engine_limits` | passed (4/4) |
| `./target/debug/effigy perf:docs-context-benchmark` | passed — all predeclared expectations held |
| `./target/debug/effigy qa` | passed — 3521/3521 tests run, 3521 passed, 1 skipped; docs and JSON contracts passed |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --all-targets -- -D warnings` | passed |
| `git diff --check` | passed |

## Risks

- Rhai may change how nesting depth is counted in a future minor; the add-chain
  fixture thresholds (`20` / `40`) are empirical for 1.x and are pinned by the
  focused suite rather than by public config.
- Call-stack and other runtime limits remain profile-dependent in the Rhai
  dependency; this lane intentionally did not touch them.

## Next Task

Return to catalog-pack acquisition planning under contract `043`. Keep S3
deferred; no release action or generation rollover.
