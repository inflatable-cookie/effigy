# 2026-04-17 02:40:00 BST — Effigy CLI Help Extraction

## Summary

Moved the root-owned CLI help surface into `effigy-cli`. Help topic rendering,
topic registry, and the shared help rendering helpers now live in
`crates/effigy-cli/src/help/` instead of `src/cli_help/topics/`. The root crate
keeps only a narrow adapter: a `HelpView` newtype that bridges the runner's
`Renderer` trait to `effigy-cli`'s `HelpRenderer` interface, plus the
`render_cli_header` helper that needs `crate::ui::theme`.

## Why This Batch

Per `g02.017` queue job #3, CLI help is a disjoint seam that had been fully
root-crate owned. The topics themselves are part of the CLI contract, not
runner concerns — they should sit beside the command grammar they describe.

## What Changed

- added `crates/effigy-cli/src/help/` with:
  - `mod.rs` — `HelpRenderer` trait (narrow: `text`, `section`, `notice`,
    `bullet_list`, `table`, `key_values`), `HelpResult` alias, and the
    `render_help` / `render_help_with_deferred_builtins` dispatch
  - `topics/mod.rs` plus 16 topic files (`bootstrap`, `changelog`, `container`,
    `contracts`, `demo`, `distribution`, `docs`, `doctor`, `general`, `init`,
    `migrate`, `release`, `tasks`, `test`, `watch`, `shared`) totalling
    `~1427` lines
- added `effigy-core` as an `effigy-cli` dependency so topics can use
  `NoticeLevel`, `TableSpec`, and `KeyValue` without touching root-crate UI
- slimmed `src/cli_help.rs` from `148 + 1383 = 1531` lines across the module
  tree to `187` lines of honest adapter code:
  - `HelpView<'a, R>` newtype that owns the orphan-rule-safe blanket bridge
    from `Renderer` to `HelpRenderer`
  - `render_help` / `render_help_with_deferred_builtins` re-exports that wrap
    the `effigy-cli` functions with the existing `UiResult` signature
  - `render_cli_header` + terminal width helpers (kept root-local because they
    depend on `crate::ui::theme::Theme` and `crossterm::terminal::size`)
- removed `src/cli_help/` subdirectory entirely
- updated a historical handoff log's reference to the moved `demo.rs` path so
  `docs check-links` stays green

## Design Notes

The orphan rule blocked `impl<R: Renderer> HelpRenderer for R` directly in the
root crate — both the trait and the type parameter are effectively foreign to
the compiler's orphan checker. A local `HelpView<'a, R: Renderer>` newtype
carries the bridge impl honestly. Callers wrap their renderer with
`&mut HelpView(renderer)` at the single call site (`src/cli_help.rs`); no
runtime cost beyond the wrapper.

## Churn Check

Real seam move. `~1427` lines of help content now live with the CLI contract
they describe. Root `cli_help.rs` is now 12% of its former size and every
remaining line is honest adapter / header theming.

## Vision Target Delta

- primary vision tags: `MAINT`, `CONTRACT`
- moved: CLI help topic ownership now lives with `effigy-cli`; root crate is a
  thin bridge + header
- remaining open: post-`229` boundary decision for the remaining `cli_help.rs`
  shell; downstream `g02.017` jobs (process runtime, UI/widget extraction)

## Validation

- `cargo test -p effigy-cli` — green
- `cargo test --lib help_and_flag_tests` — 20/20 green
- `cargo test --test cli_output_tests help_and_flags_tests` — 16/16 green
- `cargo test` — full workspace green (11 test suites)
- `cargo fmt --all -- --check` — clean
- `cargo run --bin effigy -- qa:docs` — passes
- `git diff --check` — clean

## Next Task

Execute
[`230-decide-post-cli-help-extraction-boundary.md`](../../../specs/batch-cards/230-decide-post-cli-help-extraction-boundary.md)
to classify the remaining `src/cli_help.rs` shell honestly.
