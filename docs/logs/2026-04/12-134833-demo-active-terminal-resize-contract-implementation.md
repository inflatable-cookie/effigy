# Demo Active Terminal Resize Contract Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`062-implement-demo-active-terminal-resize-contract.md`](../../specs/batch-cards/062-implement-demo-active-terminal-resize-contract.md)

## Summary

Implemented runner-owned terminal size and resize contract fields, added
`effigy demo resize`, and wired browser-consumed demo terminals through the
runner-owned resize handoff when the active session exposes it.

## Vision Target Delta

- move from `active demo terminals expose transport/input but geometry is still
  implicit` toward `active demo terminals expose runner-owned size and resize
  semantics`
- keep browser terminal work consuming runner-owned session semantics instead
  of inventing browser-local geometry state
- remaining gap: choose the next bounded follow-up after resize semantics
  landed

## Changes

- added `effigy demo resize <DEMO_ID> --cols <COLS> --rows <ROWS>`
- extended active demo/session JSON and text surfaces with terminal size,
  resize availability, resize command metadata, and resize handoff paths
- persisted detached-session resize handoff state under `.effigy/demo/active/`
- updated `effigy demo browser` to report terminal-tab viewport changes through
  the runner-owned resize surface when available
- updated help, changelog, and regression coverage

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Outcome

Opened ready card [`063-decide-demo-post-terminal-resize-contract-boundary.md`](../../specs/batch-cards/063-decide-demo-post-terminal-resize-contract-boundary.md).

## Next Task

Execute [`063-decide-demo-post-terminal-resize-contract-boundary.md`](../../specs/batch-cards/063-decide-demo-post-terminal-resize-contract-boundary.md)
to choose the next bounded slice after active demo terminal resize semantics
landed.
