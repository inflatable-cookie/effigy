# 672 - Split Remaining Config Sections

Roadmap: [`../036-manifest-section-decomposition.md`](../036-manifest-section-decomposition.md)
Strict lane: [`../../../specs/072-manifest-section-decomposition-strict-lane.md`](../../../specs/072-manifest-section-decomposition-strict-lane.md)
Contract: [`../../../contracts/028-manifest-section-decomposition-contract.md`](../../../contracts/028-manifest-section-decomposition-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Split the remaining unrelated config-section grammar out of
`config_sections.rs` so it becomes a facade instead of a catch-all file.

## Scope

- extract package manager, task defaults, shell, isolation, scan, and env schema
  sections where dependency seams are clean
- extract docs/demo policy, bootstrap, distribution, and release sections where
  dependency seams are clean
- preserve public re-exports from `effigy_manifest`
- keep demo validation behavior and error text stable
- avoid moving tests unless the move is low-risk and improves locality

## Non-Goals

- no manifest grammar changes
- no release behavior changes
- no docs policy behavior changes
- no demo system redesign
- no command behavior changes

## Acceptance

- `config_sections.rs` is reduced to a bounded facade plus any section-local
  code that genuinely needs to remain
- public manifest type imports stay compatible
- package manager, demo, bootstrap, distribution, release, and scan parsing
  behavior is unchanged
- representative manifest tests still pass

## Outcome

- added section-owned config modules:
  - `config_sections/bundle.rs`
  - `config_sections/common.rs`
  - `config_sections/demo.rs`
  - `config_sections/bootstrap.rs`
  - `config_sections/distribution.rs`
  - `config_sections/release.rs`
- kept `config_sections.rs` as a public facade plus existing compatibility tests
- preserved public manifest re-exports and TOML grammar
- reduced `config_sections.rs` from 1,944 lines at lane start to 745 lines
- removed `effigy-manifest` manifest files from god-file scan findings

## Validation

```sh
cargo test -p effigy-manifest
cargo check --bin effigy
effigy scan god-files --json
git diff --check
```

## Next Task

Execute `673` to close the manifest section decomposition proof.
