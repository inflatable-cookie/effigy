# 086 Decide Rust-Native Scripting Boundary And Pilot Slice

Status: complete
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/archive/004-rust-native-scripting-strict-lane.md`

## Objective

Set the product boundary for Effigy-native Rhai scripting and choose the first
bounded pilot slice for Rust-first repos.

## In Scope

- define the scripting policy split:
  - Rust-first repos => Effigy-native scripting
  - web-oriented repos => Bun + TS
- define what Rhai v1 should and should not try to replace
- classify current non-Bun script surfaces in:
  - `effigy`
  - `keepsake`
  - `jetstream`
- make Jetstream's “full migration target” posture explicit
- choose the first pilot implementation slice

## Out Of Scope

- implementing Rhai support
- rewriting repo scripts in this batch
- changing web-oriented repos to match the Rust-first policy

## Acceptance Criteria

- the Rhai v1 host boundary is explicit
- the first pilot repo/order is explicit
- Jetstream's migration posture is explicit and honest
- a follow-up implementation card can execute without re-deciding the product
  boundary

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

The boundary settled cleanly. Use
[`087-implement-rhai-script-step-foundation.md`](./087-implement-rhai-script-step-foundation.md)
next.
