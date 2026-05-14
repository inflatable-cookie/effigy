# g06 Baseline Size And Duplication

Date: 2026-05-14
Roadmap: `g06.001`
Card: `801`

## Summary

Captured the starting lean-down baseline for `g06`.

This log fixes the first execution order for the codebase reduction tranche:

1. `802` state command split
2. `803` release lib split
3. `804` shared fixture convergence
4. `805` CLI help/render dedupe
5. `806` typed contract-shape reuse
6. `807` compatibility branch deletion
7. `808` runner-private domain logic reduction

## Baseline Metrics

- Rust LOC across `src`, `crates`, `tests`, `skills`: `233,544`
- broader source/config LOC across the same tree: `236,893`
- god-file findings: `2`
- duplicate-block findings: `96`
- duplicate-block severity counts:
  - high: `8`
  - warning: `88`

## God-File Baseline

Current warning-level large files:

- `src/runner/state_command.rs` — `2150` total lines, `1990` code lines
- `crates/effigy-release/src/lib.rs` — `1622` total lines, `1520` code lines

No high or critical god files were reported.

## Duplicate Baseline

Highest-value remaining duplicate clusters:

- CLI help topic layout duplication in:
  - `crates/effigy-cli/src/help/topics/bootstrap.rs`
  - `crates/effigy-cli/src/help/topics/container.rs`
  - `crates/effigy-cli/src/help/topics/docs.rs`
  - `crates/effigy-cli/src/help/topics/release.rs`
- deploy provider fixture duplication across:
  - `src/tests/json_contract_tests/prelude.rs`
  - `src/tests/runner_tests/runner_core_tests/deploy_tests.rs`
  - `tests/cli_output_tests/json_envelope_tests/mod.rs`
- duplicated temp repo builders across:
  - `src/runner/container_command/lifecycle.rs`
  - `src/runner/container_command/shell_prep.rs`

## Execution Decision

The lane keeps the current order.

Why:

- `802` and `803` attack the two remaining warning-level god files first
- `804` targets fresh evidence that test support still owns too little shared
  concurrency/fixture behavior
- `805` targets the largest remaining high-severity duplicate cluster
- `806` through `808` stay later because they are more judgment-heavy and
  benefit from cleaner ownership seams first

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`
- moved in this report: no code reduction yet; baseline captured and first
  execution order locked
- remains open:
  - `state_command.rs` split
  - `effigy-release/src/lib.rs` split
  - high duplicate-block clusters
  - runner-private domain logic reduction

## Validation

Commands used:

```bash
find src crates tests skills -type f -name '*.rs' -print0 | xargs -0 wc -l | tail -n 1
find src crates tests skills -type f \( -name '*.rs' -o -name '*.rhai' -o -name '*.sh' -o -name '*.js' -o -name '*.ts' -o -name '*.mjs' -o -name '*.cjs' -o -name '*.json' -o -name '*.toml' -o -name '*.yml' -o -name '*.yaml' \) -print0 | xargs -0 wc -l | tail -n 1
cargo run --bin effigy -- scan god-files --json
cargo run --bin effigy -- scan duplicate-blocks --json
```
