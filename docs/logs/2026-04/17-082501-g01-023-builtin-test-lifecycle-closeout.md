# 2026-04-17 Builtin Test Lifecycle Closeout

## Vision Target Delta

- Primary tags: `TEST`, `CONTRACT`, `OPERATE`
- Moved from `g01.023 still treated as a live v0.3 blocker` to `g01.023 closed on shipped behavior and test evidence`
- Remaining open: `g02.010` final `/src` cleanup, `g01.027` live built-in release closeout

## Summary

`g01.023` was stale-open.

The core behavior is already shipped:

- richer `[test.suites.<name>]` config in `crates/effigy-manifest/src/test_config.rs`
- lifecycle-aware builtin test planning and execution under `src/runner/builtin/test/**`
- plan/result rendering for `suite-env`, `suite-env-files`, `setup-steps`,
  `teardown-steps`, and `teardown-policy`
- user-facing guides in:
  - `docs/guides/013-testing-orchestration.md`
  - `docs/guides/048-built-in-test-suite-lifecycle-and-env.md`

The roadmap stayed open because the planning state had not caught up with the
implementation and docs.

## Evidence

Code and docs already describe the shipped contract:

- `crates/effigy-manifest/src/test_config.rs`
- `src/runner/builtin/test/execution.rs`
- `src/runner/builtin/test/planning/resolve/target_config.rs`
- `src/runner/builtin/test/render/plan_text.rs`
- `src/runner/builtin/test/render/results.rs`
- `docs/guides/013-testing-orchestration.md`
- `docs/guides/048-built-in-test-suite-lifecycle-and-env.md`

Existing targeted tests already cover the main acceptance surface:

- parser/schema rejection for invalid suite fields
- suite env and env_file application
- setup/teardown execution and teardown-on-failure policy
- lifecycle-aware plan and JSON output

## Outcome

`g01.023` is now marked `Complete`.

Front-door blocker lists now only treat these as live pre-`v0.3` blockers:

1. `g02.010` remaining `/src` cleanup and reconciliation
2. `g01.027` release orchestration live closeout

