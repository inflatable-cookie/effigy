# 022 - Binary Entrypoint Hardening

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-03
Depends on: —

## Problem

The binary entrypoints have two gaps:

1. `src/bin/effigy.rs` ignores the return value of `effigy::run_cli`. If the
   runner returns an error, there is no graceful exit-code propagation or
   last-resort error printing at the binary boundary.

2. `src/bin/effigy-qa.rs` panics with `.expect("failed to launch Effigy QA task")`
   if `cargo` is not on `PATH`. A QA wrapper should fail gracefully, not panic.

## Goal

Harden both binary entrypoints so they handle failure gracefully and propagate
exit codes correctly.

## Scope

- capture and propagate the return value of `run_cli` in `src/bin/effigy.rs`
- print a minimal error message on non-zero exit if `run_cli` does not already
  handle it
- replace the `.expect()` in `src/bin/effigy-qa.rs` with a proper error message
  and non-zero exit
- verify exit codes under failure scenarios (e.g., missing `cargo`, bad args)
- add a short note to `AGENTS.md` about the binary entrypoint contract

## Non-Goals

- restructuring the runner error type
- adding logging frameworks to the binary layer
- changing `run_cli`'s internal behavior

## Exit Condition

This milestone is complete when:

- `effigy` exits with a non-zero code and prints an error on runner failure
- `effigy-qa` exits with a non-zero code and a readable message when `cargo` is
  missing, instead of panicking
- both behaviors are confirmed by manual or automated test

## Next Task

If this lane is promoted, start by inspecting `run_cli`'s return type to decide
how to propagate it.
