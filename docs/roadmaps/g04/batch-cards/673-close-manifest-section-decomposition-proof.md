# 673 - Close Manifest Section Decomposition Proof

Roadmap: [`../036-manifest-section-decomposition.md`](../036-manifest-section-decomposition.md)
Strict lane: [`../../../specs/072-manifest-section-decomposition-strict-lane.md`](../../../specs/072-manifest-section-decomposition-strict-lane.md)
Contract: [`../../../contracts/028-manifest-section-decomposition-contract.md`](../../../contracts/028-manifest-section-decomposition-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Close the manifest section decomposition lane after proving behavior stayed
stable.

## Scope

- record final module shape
- mark roadmap `036` complete
- mark strict lane `072` complete
- mark contract `028` accepted
- advance front doors to the next g04 roadmap item

## Outcome

- `bundles.rs` is now the bundle descriptor/default/template owner and facade
- `bundles/source.rs` owns bundle source selection, git/OCI materialization,
  cache identity, sync, inspect, and focused source tests
- `config_sections.rs` is now a public facade plus compatibility tests
- `config_sections/container.rs` owns container/system/workspace/data schema
- `config_sections/bundle.rs` owns `[bundle]` grammar and legacy migration
  errors
- `config_sections/common.rs` owns task defaults, isolation, package manager,
  scan, shell, and env schema config
- `config_sections/demo.rs` owns docs policy and demo config validation
- `config_sections/bootstrap.rs` owns bootstrap config
- `config_sections/distribution.rs` owns distribution config
- `config_sections/release.rs` owns release config

## Validation

```sh
cargo test -p effigy-manifest
cargo check --bin effigy
effigy scan god-files --json
git diff --check
```

All listed validation passed. The god-file scan still reports unrelated warning
findings for `src/runner/state_command.rs` and `crates/effigy-release/src/lib.rs`;
no manifest files remain in the scan findings.

## Next Task

Execute `g04.037` for deploy domain boundary hardening.
