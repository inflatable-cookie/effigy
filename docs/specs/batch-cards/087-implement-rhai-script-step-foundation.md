# 087 Implement Rhai Script Step Foundation

Status: complete
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/004-rust-native-scripting-strict-lane.md`

## Objective

Add the first bounded Effigy-native Rhai execution surface for Rust-first repos
so small shell-glue tasks can move into a native script step without requiring
external Bun or shell runtimes.

## In Scope

- add a minimal Rhai-backed script step to task execution
- support script sources as:
  - file-backed `.rhai` source referenced through `rhai = "path/to/script.rhai"`
- ship a narrow v1 host API for:
  - logging
  - args access
  - env read
  - path helpers
  - file read/write/exists/create-dir
  - JSON/TOML parse + stringify helpers
  - structured subprocess execution without shell parsing
- document the v1 limits clearly
- prove the surface by migrating one small Effigy shell-glue task

## Out Of Scope

- broad shell-script replacement across repos
- Jetstream Python-analysis migration
- Electron/frontend build-tool migration
- arbitrary shell emulation
- network APIs

## Acceptance Criteria

- Effigy can execute a bounded Rhai script step natively
- the host API is explicit and small
- one real Effigy shell-glue task is migrated as pilot proof
- docs explain how Rust-first repos should use the new surface

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Next card:

- [`088-decide-post-rhai-foundation-migration-slice.md`](./088-decide-post-rhai-foundation-migration-slice.md)
