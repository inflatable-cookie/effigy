# Release Readiness Audit

Status: complete for this audit batch; release blocked
Created: 2026-09-24
Roadmap: g10 planning required
Batch: pre-release-readiness-audit

## Summary

- Audited changes since `v0.12.1` across runtime code, public docs, help,
  changelog, support policy, and release gates. The current candidate is not
  ready to release.
- Consolidated repeated `[Unreleased]` category headings. The old analyzer
  counted only the last block of each category, omitting 36 entries. Recorded
  the removed command aliases as Breaking; the analyzed suggestion is now
  `0.13.0`, subject to operator confirmation.
- Made changelog validation reject duplicate categories and analysis count
  all entries even in a malformed file. Updated doctor runtime help with its
  budgets and override, and documented the support-policy release step.
- Corrected two existing format/Clippy failures without changing behavior.
- Captured four unresolved runtime findings under `docs/triage/`: committed
  docs-source consent/provenance, source handle/status identity, doctor
  subprocess deadlines, and nested skill stdio. Triage is not execution
  authority.

## Vision Target Delta

- Primary tags: `RELEASE`, `CONTRACT`, `OPERATE`, `ROUTE`.
- Movement: incomplete changelog count and stale help/procedure -> complete
  release inventory and validated local documentation/runtime help.
- Remaining gap: review and repair of the open runtime findings, exact-head CI,
  release support-policy/version preparation, and release execution.

## Validation Performed

- `effigy release status`: version `0.12.1`, seven configured gates.
- `effigy release gates`: stopped at the CI gate because no successful
  `ci.yml` dispatch exists for the exact candidate commit; no later release
  gate ran in that invocation.
- `effigy qa:ci:local`: passed after the format/Clippy corrections, including
  3,900 workspace tests passed and one skipped, docs, JSON, and released-surface
  checks.
- `cargo test -p effigy-changelog`: 50 passed.
- `effigy qa:docs`, `cargo fmt --all -- --check`, changelog validation, runtime
  `help doctor`, and `git diff --check`: passed.

## Risks

- The four triage notes need operator disposition before a release candidate
  can be called ready. The first, third, and fourth describe direct contract
  violations; source handle/status identity also needs an explicit behavior
  choice.
- The first release exposing public `service pack update` must update
  `support/catalog-pack-update.toml` and the capability marker with the version
  bump. The built-in prepare flow does not sync them.

## Next Task

- Settle the pre-release repair frontier with the operator. Then obtain a
  successful exact-head CI run and rerun all release gates before preparing
  any release mutation.
