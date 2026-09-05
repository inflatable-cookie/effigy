# Dead-Code Residual Planning

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1045`

## Summary

Compiled the next dead-code scan precision tranche.

The remaining findings after `g08.009` still look mostly like graph precision
work, not deletion work. The next card is scoped to classify residuals and fix
one bounded false-positive class.

## Baseline

`target/debug/effigy scan dead-code --json` reports:

- findings: 1,178
- isolated files: 5
- unreferenced symbols: 1,173
- symbol kinds:
  - 875 functions
  - 173 structs
  - 92 methods
  - 31 enums
  - 2 traits

Largest residual path groups:

- `crates/effigy-manifest/src/config_sections.rs`: 36
- `src/runner/container_command/data.rs`: 29
- `crates/effigy-cli/src/help/registry.rs`: 27
- `crates/effigy-containers/src/manager.rs`: 26
- `crates/effigy-builtin/src/scan/execution/core/api.rs`: 24

## Decision

Open `g08.009` as graph-precision work.

Do not open a cleanup/deletion tranche yet. The residuals include test symbols,
trait/impl surfaces, dispatch helpers, and qualified-call patterns that the
graph still undercounts.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: `g08.009` reduced dead-code findings from 6,497 to 1,178 without
  repo-wide symbol suppression.
- Current: residuals are classified as another graph-precision tranche before
  cleanup.
- Remaining open: execute `1045`.

## Next Task

Run ready card `1045`.
