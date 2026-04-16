# Post Demo Browser Host Runtime Loop Boundary Decision

Date: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Summary

`164` closes the browser seam.

After `163`, the remaining production shell in
`src/tui/demo_browser.rs` is only the TUI launch wrapper plus the direct
Effigy demo-command bridge. That is honest adapter work. The browser seam can
pause.

The broader `/src` review still says `g02.010` remains active. The next large
runner-heavy seam is `src/runner/demo_command.rs`, with
`src/runner/release_command.rs` still behind it as a later shell review
candidate.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: baseline `browser seam still under active extraction` -> current
  `browser seam paused; next modularization pressure moves back into runner`
- Remaining gap: `demo runner runtime/persistence extraction from
  src/runner/demo_command.rs`

## Decision

Pause the browser seam.

Keep `g02.010` active.

The next ready card is:

- `165-implement-effigy-demo-runner-runtime-and-persistence-follow-up-extraction.md`

## Evidence

- `src/tui/demo_browser.rs` production entrypoints:
  - `run_demo_browser_tui(...)`
  - `invoke_demo_json(...)`
- largest remaining `/src` seams:
  - `src/runner/release_command.rs`: `5581`
  - `src/runner/demo_command.rs`: `4636`
  - `src/runner/distribution_command.rs`: `1352`
  - `src/runner/container_command.rs`: `1276`

## Next Task

Execute
`165-implement-effigy-demo-runner-runtime-and-persistence-follow-up-extraction.md`.
