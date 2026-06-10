# 2026-04-17 02:55:00 BST — Post CLI Help Extraction Boundary Decision

## Summary

CLI help pauses cleanly.

After `229`, `src/cli_help.rs` is `187` lines, every one of them honest
adapter/bridge/theming work:

- `HelpView<'a, R: Renderer>` newtype + `HelpRenderer` impl — the orphan-rule
  bridge from the runner's `Renderer` to `effigy-cli`'s narrow `HelpRenderer`
- `ui_error_to_io` / `io_error_to_ui` — error-surface converters
- `render_help` / `render_help_with_deferred_builtins` — thin wrappers that
  delegate to `effigy-cli::help::*` with the existing `UiResult` signature
- `render_cli_header` + `fit_cli_header_path` + `cli_header_terminal_cols` +
  `truncate_path_for_header` — the themed CLI header, which depends on
  `crate::ui::theme::Theme` and `crossterm::terminal::size`

No domain logic remains. All help topic content, topic registration, and
shared rendering helpers live in `crates/effigy-cli/src/help/`.

## Why This Decision

Further extraction would mean pulling `Theme` + `crossterm` into `effigy-cli`
just to relocate the CLI header. That trades a clean disjoint adapter shell
for a coupled CLI crate with terminal-probing dependencies it does not need.
That would be fake completeness work, not real boundary progress.

The orphan-rule bridge (`HelpView`) is also a legitimate root-crate concern:
it exists precisely because the root owns `Renderer` and `effigy-cli` owns
`HelpRenderer`, and the bridge between them must live at one of those
endpoints.

## Decision

- pause CLI help on the current boundary
- keep `effigy-cli` as the owner of help topics, help-topic registry, and
  shared help rendering helpers
- move the active lane to the next `g02.017` queue job

## Churn Check

Real boundary. `~1427` lines of help content moved to `effigy-cli` under `229`,
and the `187` lines left are the minimum viable adapter surface.

## Vision Target Delta

- primary vision tags: `CONTRACT`, `MAINT`
- moved: CLI help now paused on an honest adapter shell with crate-domain
  ownership fully aligned
- remaining open: pick the next `g02.017` queue job and execute it

## Validation

- `cargo test` — full workspace green (11 suites)
- `cargo run --bin effigy -- qa:docs` — passes
- `git diff --check` — clean

## Next Task

Execute
[`231-decide-next-src-shell-cleanup-priority-after-cli-help-pause-boundary.md`](../../../specs/batch-cards/231-decide-next-src-shell-cleanup-priority-after-cli-help-pause-boundary.md)
to pick the next `/src` cleanup priority after pausing CLI help.
