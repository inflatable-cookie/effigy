# Multiprocess TUI Config Contract

## Purpose

Define the internal tuning contract for the multiprocess TUI runtime so performance and UX behavior can be adjusted in one place.

## Source of Truth

- `src/tui/multiprocess/config.rs`
- runtime toggle: `EFFIGY_TUI_DIAGNOSTICS=1|true` (enables diagnostics summary/traces)

## Current Knobs

- `MAX_LOG_LINES`: maximum retained non-vt line buffer per process.
- `MAX_EVENTS_PER_TICK`: upper bound of process events drained per render loop tick.
- `VT_PARSER_ROWS`: vt parser row capacity.
- `VT_PARSER_COLS`: vt parser column capacity.
- `VT_PARSER_SCROLLBACK`: vt parser scrollback capacity.
- `EVENT_DRAIN_WAIT`: per-drain non-blocking wait duration for process events.
- `INPUT_POLL_WAIT`: key input poll interval for UI responsiveness.
- `SHUTDOWN_GRACE_TIMEOUT`: graceful shutdown timeout before force stop.

Managed child startup runs in configured sequence while the TUI event loop
renders and accepts input. Waiting tabs identify configured start delays;
starting, running, and failed states follow supervisor lifecycle events.
Cancelling a session interrupts a pending delay before cleanup signals the
children already recorded by that supervisor. This preserves each
`start_after_ms` delay and spawn order without postponing the first frame.

## Invariants

- `MAX_EVENTS_PER_TICK` should stay finite to prevent event-starvation of UI input.
- `INPUT_POLL_WAIT` should remain short enough for responsive key handling.
- `SHUTDOWN_GRACE_TIMEOUT` must be long enough for common dev servers to flush and exit cleanly.
- `MAX_LOG_LINES` only affects non-vt fallback logs; vt sessions are governed by parser scrollback settings.

## Change Guidance

When changing these constants:

1. Validate with `cargo test -q`.
2. Smoke-test `effigy dev` with high output throughput and shell interaction.
3. Verify shutdown summaries still render and terminal state restores correctly.
