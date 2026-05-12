# 671 - Split Container Config Section

Roadmap: [`../036-manifest-section-decomposition.md`](../036-manifest-section-decomposition.md)
Strict lane: [`../../../specs/072-manifest-section-decomposition-strict-lane.md`](../../../specs/072-manifest-section-decomposition-strict-lane.md)
Contract: [`../../../contracts/028-manifest-section-decomposition-contract.md`](../../../contracts/028-manifest-section-decomposition-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Move container/system/workspace/data config grammar out of the catch-all
`config_sections.rs` file.

## Scope

- extract container environments, systems, workspaces, services, DNS, exec
  aliases, lifecycle, host process, host mount, and data target config structs
- preserve public re-exports from `effigy_manifest`
- keep `TaskManifest` field names and TOML grammar unchanged
- move related config-section tests with the new owner where practical
- keep validation functions in the least surprising owner

## Non-Goals

- no container runtime behavior changes
- no task execution binding redesign
- no manifest grammar changes
- no public API break
- no deployment or state parser work

## Acceptance

- `config_sections.rs` no longer owns the container/system/data schema cluster
- downstream callers still import the same public manifest types
- container config tests still pass
- execution binding tests still pass
- `config_sections.rs` god-file pressure is materially reduced

## Outcome

- added `crates/effigy-manifest/src/config_sections/container.rs`
- moved container, system, workspace, DNS, exec alias, lifecycle, host process,
  host mount, and data target schema types into the new owner
- preserved public re-exports through `config_sections.rs` and
  `effigy_manifest`
- left the existing parse tests in place for now; they still exercise the
  re-exported public types and can move in a later cleanup if needed
- reduced `config_sections.rs` from 1,944 lines to 1,542 lines

## Validation

```sh
cargo test -p effigy-manifest
cargo check --bin effigy
git diff --check
```

## Next Task

Execute `672` for the remaining config-section splits.
