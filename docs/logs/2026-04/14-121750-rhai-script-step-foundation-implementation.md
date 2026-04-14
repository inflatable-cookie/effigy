# Rhai Script Step Foundation Implementation

Date: 2026-04-14  
Roadmap: `g02.004`  
Batch card: `087-implement-rhai-script-step-foundation.md`

## Summary

Shipped the first bounded Effigy-native Rhai scripting surface.

The foundation now supports:

- Rhai-backed run steps with `rhai = "scripts/example.rhai"`
- a narrow v1 host API for:
  - logging
  - args/context access
  - env reads
  - path helpers
  - file operations
  - JSON/TOML helpers
  - structured subprocess execution
  - task invocation

The first pilot migration also landed:

- `link:local` now runs through a file-backed Rhai script instead of shell
  glue

## Implementation Notes

- added hidden internal `__rhai-step` execution plumbing
- extended run-array step schema to accept `rhai` as a file-backed script path
- added a small script runtime under `src/runner/script_command.rs`
- kept the host API intentionally narrow
- avoided shell parsing in the Rhai subprocess surface
- documented the new scripting front door in:
  - `docs/guides/061-rhai-script-steps-guide.md`
  - `docs/guides/022-manifest-cookbook.md`
  - `docs/guides/059-manifest-composition-guide.md`

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `EFFIGY_LOCAL_BIN_DIR="$(mktemp -d)" cargo run --bin effigy -- link:local`
- `git diff --check`

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `ADOPT`
- Movement:
  - Rust-native scripting moved from boundary-only planning to a shipped
    Effigy runtime surface with one real pilot migration.
- Remaining open:
  - choose the next migration slice after the foundation:
    - more Effigy shell-glue migration
    - Keepsake pilot
    - or the first Jetstream orchestration migration boundary

## Next Task

Execute `088-decide-post-rhai-foundation-migration-slice.md`.
