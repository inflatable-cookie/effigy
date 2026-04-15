# Modularization Lane Reopen For CLI And TUI Shell Seams

Date: 2026-04-15
Roadmap: `g02.010`
Card: `143`

## Summary

The previous `g02.010` pause boundary was too permissive.

The extracted product-domain crates are real, but the remaining CLI
shell/help/parse cluster and the TUI/browser runtime cluster are both still
large enough to justify more modularization work before `v0.3`.

## Decision

Reopen `g02.010`.

Queue `115` again and make the next move a bounded decision on the remaining
shell-facing seams.

## Why The Pause Was Too Soft

The remaining weight in `src/` is not just generic residue:

- `src/cli/parse/command_parsing.rs` is still large enough to reflect a real
  shell grammar boundary
- `src/lib.rs` still owns a broad top-level command and argument model surface
- `src/tui/demo_browser.rs` is still a large browser/runtime surface
- the wider `src/tui/` tree is still substantial enough to consider a bounded
  crate boundary instead of only local tidying

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: `paused modularization lane and active release lane` -> `reopened modularization lane focused on remaining shell seams`
- Remaining gap: `one more explicit CLI/TUI modularization decision before release closure can resume honestly`

## Next Task

Execute [`143-decide-cli-shell-and-tui-modularization-follow-up.md`](../../specs/batch-cards/143-decide-cli-shell-and-tui-modularization-follow-up.md).
