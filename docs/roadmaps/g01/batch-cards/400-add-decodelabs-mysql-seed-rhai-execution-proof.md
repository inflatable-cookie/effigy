# 400 - Add DecodeLabs Mysql Seed Rhai Execution Proof

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Prove the DecodeLabs mysql seed bug class against the Rhai execution surface.

## Scope

- add a focused synthetic proof around Rhai `exec::run(...)`
- model a repo-owned SQL seed file under a DecodeLabs-like bundle path
- call mysql through `run_in = "container"` with `container`, `service`, and
  `stdin_file`
- assert that the callback sees a container exec request, not a host
  `process::run(...)`
- assert that `stdin_file` remains the repo-relative seed path supplied by the
  script
- keep the proof synthetic; no live container, mysql, or external project

## Exit Condition

This card is complete when the Rhai proof fails if mysql seed execution drifts
back to host process execution or loses the structured `stdin_file`.

## Closeout

Added a synthetic DecodeLabs mysql seed Rhai proof in
`crates/effigy-rhai/src/tests.rs`.

The proof models a repo-owned bundle SQL path and requires
`exec::run(["mysql", ...], #{ run_in: "container", container: "web", service:
"db", stdin_file: ... })` to route through `container_exec_with_options`.

It asserts:

- repo root passed to the container callback is the target repo
- mysql command is preserved as structured argv
- container and service routing are preserved
- `stdin_file` remains the repo-relative seed path from the script
- result route reports container execution

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-rhai decodelabs_mysql_seed -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-rhai -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next Task

Add the Underlay generated-compose path proof.
