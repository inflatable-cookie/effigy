# Dead-Code Rust Impl-Call Precision

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1049`

## Summary

Completed the Rust impl and associated-call precision batch for the dead-code
scan.

The scanner now resolves dot-qualified call targets such as `self.prepare()` to
their last path segment, and recognizes generic trait impl headers such as
`impl<W> Trait for Type`. This credits reachable impl methods without blanket
function suppression.

## Changes

- Added dot-qualified unresolved target fallback for dead-code symbol
  resolution.
- Extended Rust trait-impl source classification to `impl<...> Trait for Type`.
- Added regression fixtures proving:
  - generic trait impl methods do not report as standalone dead code
  - `self.method()` calls credit private impl helpers
  - unrelated private helpers still report

## Result

Before this slice, after `1048`, `target/debug/effigy scan dead-code --json`
reported:

- findings: 285
- isolated files: 5
- unreferenced symbols: 280
- function findings: 274

After this slice:

- findings: 196
- isolated files: 4
- unreferenced symbols: 192
- checked symbols: 2,283
- function findings: 186

The previous largest impl-heavy groups in `plain_renderer` and `storage` no
longer dominate the residual scan.

Largest remaining groups:

- `crates/effigy-manifest/src/bundles.rs`: 8
- `crates/effigy-builtin/src/config/docs/tasks.rs`: 8
- `crates/effigy-scan/src/render/graph.rs`: 7
- `src/runner/secrets_command.rs`: 6
- `crates/effigy-managed/src/render_support.rs`: 6

## Remaining Queue

The next residual classes are:

- renderer/config doc helper functions that may be called through array/vector
  assembly or repeated section builders
- private helper pockets that may now be real cleanup candidates
- four isolated files requiring manual inspection
- six remaining non-function findings requiring manual classification

## Validation

- `cargo test -p effigy scan_tests::dead_code -- --nocapture`
- `cargo build -p effigy`
- `cargo clippy -p effigy-builtin --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `target/debug/effigy graph index --json`
- `target/debug/effigy scan dead-code --json`
- `cargo test -p effigy scan_contract_tests::dead_code -- --nocapture`
- `cargo fmt --all -- --check`

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: residual dead-code scan had 285 findings after data-shape root
  handling, including 274 function findings.
- Current: residual findings are 196, with function findings reduced to 186.
- Remaining open: classify remaining helper pockets, isolated files, and
  non-function findings into real cleanup versus one more precision batch.

## Next Task

Planning checkpoint: decide the next `g08.009` residual dead-code batch.
