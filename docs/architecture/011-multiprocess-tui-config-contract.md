# Multiprocess TUI Config Contract

## Purpose

Define the internal tuning contract for the multiprocess TUI runtime so performance and UX behavior can be adjusted in one place.

## Source of Truth

- `src/tui/multiprocess/config.rs`

## Current Knobs

- `MAX_LOG_LINES`: maximum retained non-vt line buffer per process.
- `MAX_EVENTS_PER_TICK`: upper bound of process events drained per render loop tick.
- `VT_PARSER_ROWS`: vt parser row capacity.
- `VT_PARSER_COLS`: vt parser column capacity.
- `VT_PARSER_SCROLLBACK`: vt parser scrollback capacity.
- `EVENT_DRAIN_WAIT`: per-drain non-blocking wait duration for process events.
- `INPUT_POLL_WAIT`: key input poll interval for UI responsiveness.
- `SHUTDOWN_GRACE_TIMEOUT`: graceful shutdown timeout before force stop.

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
