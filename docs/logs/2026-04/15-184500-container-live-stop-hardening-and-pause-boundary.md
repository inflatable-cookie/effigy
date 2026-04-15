# Container Live Stop Hardening And Pause Boundary

Date: 2026-04-15
Roadmap: `g02.006`
Batch: `110`

## Summary

Closed the last deferred hardening edge in the container lane.

Shipped changes:

- attached container startup now installs stop handling before Colima start,
  compose bring-up, and health wait complete
- startup-phase stop requests now route through the same closeout path as
  attached live-session exits
- inherited log-follow subprocess trees now shut down as process groups instead
  of only killing the top-level compose wrapper

## Consumer Proof

Bounded consumer repo: `contact-patch`

Real proof:

- launched `target/debug/effigy dev:services --repo /Users/tom/Dev/projects/contact-patch`
- sent timed `SIGINT` during the non-PTY stream path
- Effigy exited `0`
- closeout reported:
  - started Colima profile
  - attached session finished `(signal)`
  - graceful shutdown applied
- immediate post-stop `container status --json` showed:
  - `colima_running: true`
  - empty compose status rows
  - `health: waiting`

That removes the last deferred live-stop warning from the lane.

## Decision

Pause `g02.006` on the current v1 container boundary.

The lane now has:

- first-class named/default container support
- attached session UX
- repo-owned task composition
- one real consumer proof
- real-machine stop and closeout hardening

The next valid move is not more container widening. It is `g02.007`
distribution release closure.

## Validation

- `cargo test --test cli_output_tests container`
- `cargo test --lib container`
- real `contact-patch` timed-`SIGINT` proof plus post-stop status check
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved: the container lane shifted from one deferred real-machine stop edge to
  a paused v1 boundary with that edge closed
- Remaining open: broader future product work only, not a hidden v1 stop or
  closeout gap

## Next Task

Activate `g02.007` next so the shipped optional distribution surface can close
through an Effigy release and rollout plan.
