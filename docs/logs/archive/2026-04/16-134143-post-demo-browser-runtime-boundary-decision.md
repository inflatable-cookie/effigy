# Post Demo Browser Runtime Boundary Decision

Date: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Summary

`162` closes the post-runtime browser checkpoint.

The browser seam is much smaller than it was before batches `147` through
`161`, but it is not yet clean enough to pause. `src/tui/demo_browser.rs`
still holds one real host/runtime loop boundary around the browser run loop,
Effigy command invocation bridge, refresh/resize/shutdown execution, and the
remaining integration-heavy shell tests.

That is still enough root-owned browser behavior to justify one more bounded
extraction batch before treating the remainder as honest shell-only work.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: baseline `post-runtime browser checkpoint undecided` -> current
  `browser seam judged not yet clean; one more host/runtime loop extraction is
  required`
- Remaining gap: `final demo-browser host/runtime loop boundary`

## Decision

Do not pause the demo-browser seam yet.

The next ready card is:

- `163-implement-effigy-demo-browser-host-runtime-loop-extraction.md`

## Evidence

- `src/tui/demo_browser.rs`: `1387` lines
- `crates/effigy-tui/src/demo_browser.rs`: `3633` lines

Remaining root shell concentration:

- `run_demo_browser_tui(...)`
- `DemoBrowserApp::run(...)`
- `execute_host_effect(...)`
- `execute_runtime_plan(...)`
- `shutdown_live_terminal_session(...)`
- `refresh_state(...)`
- `sync_active_terminal_resize_for_viewport(...)`
- `invoke_demo_json(...)`

## Validation Performed

- `cargo fmt --all`
- `cargo test demo_browser --lib`
- `cargo test -p effigy-tui`

## Next Task

Execute
`163-implement-effigy-demo-browser-host-runtime-loop-extraction.md`.
