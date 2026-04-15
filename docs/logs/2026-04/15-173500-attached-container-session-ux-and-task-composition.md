# Attached Container Session UX And Task Composition

Date: 2026-04-15
Roadmap: `g02.006`
Batch: `108`

## Summary

Widened the first Colima container foundation into a real attached operator
surface and one repo-owned task-composition path.

Shipped changes:

- attached `effigy container up` sessions now branch into:
  - an Effigy multi-tab session on interactive terminals
  - a stream-mode overview plus primary-service log follow on
    non-interactive runs
- added task-level `container_session = "..."` support so repos can expose a
  named/default container session through ordinary task names without
  embedding raw compose commands
- widened task/reference/schema/display handling so container-session tasks are
  treated as runnable task surfaces rather than malformed no-`run` tasks
- updated the container guide and config schema examples to document the new
  task-composition path

## Consumer Proof

Bounded consumer repo: `contact-patch`

Consumer widening:

- added `tasks."dev:services".container_session = "default"` alongside the
  existing `services` container registry
- exercised the repo-owned task path on the real machine through:
  - `target/debug/effigy dev:services --repo /Users/tom/Dev/projects/contact-patch`

Real proof result:

- the repo-owned task path now launches honestly through Effigy instead of raw
  compose shell glue
- the stream fallback showed container identity, owner task, shutdown policy,
  health, ports, and live service logs in one runner-owned session
- the remaining honest gap is live stop proof on the real
  `colima nerdctl compose` path:
  non-interactive external-stop behavior remained less trustworthy than the
  targeted runtime tests, so the batch leaves that edge explicit instead of
  claiming full real-machine parity

## Validation

- `cargo test --test cli_output_tests container`
- `cargo test --lib command_kind_and_name_maps_command_variants`
- `cargo test --lib run_manifest_task_builtin_config_schema_prints_canonical_template`
- real consumer proof launch via `target/debug/effigy dev:services --repo /Users/tom/Dev/projects/contact-patch`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`
- `git -C /Users/tom/Dev/projects/contact-patch diff --check`

## Churn

This batch stayed within the intended `108` boundary.

The only meaningful widening happened after the first live proof failure:

- `container_session = "default"` needed to resolve through the manifest
  default alias instead of being treated as a literal container name

That was a bounded contract correction, not a lane change.

## Vision Target Delta

- Tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `attached container bring-up was mostly live log-follow plus raw
  command entrypoints` to `attached sessions now have an Effigy-owned session
  shape and repos can expose named container sessions through ordinary task
  aliases`
- Remains open: real-machine live-stop hardening for the `colima nerdctl`
  proof path, if that edge is judged too operator-critical to defer

## Next Task

Execute `109-decide-post-container-session-and-task-composition-boundary.md`
to decide whether the remaining live-stop proof edge is small enough to pause
or still needs one more bounded hardening batch.
