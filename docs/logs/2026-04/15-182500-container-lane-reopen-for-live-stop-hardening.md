# Container Lane Reopen For Live Stop Hardening

Date: 2026-04-15
Roadmap: `g02.006`

## Decision

Reopen `g02.006`.

## Why

The previous pause boundary no longer matches the actual delivery order.

The container lane must be fully finished before release closure work starts.
That means the deferred real-machine `colima nerdctl` live-stop edge is not a
future nice-to-have anymore. It is the last blocking batch inside `006`.

## Outcome

- `g02.006` is active again
- `g02.007` stays queued
- `110-harden-real-machine-container-live-stop-and-closeout.md` is the new
  ready card

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`
- Moved: the container lane shifted from paused back to active because the
  deferred live-stop edge is now treated as required completion work
- Remaining open: one bounded real-machine hardening batch on the
  `colima nerdctl` stop/closeout path

## Next Task

Execute `110-harden-real-machine-container-live-stop-and-closeout.md`.
