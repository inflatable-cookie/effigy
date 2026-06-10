# Demo Terminal Human UX Recovery

Date: 2026-04-12
Roadmap: `g02.003`
Recovery mode: `replan-after-change`
Lane posture: `strict-ready`

## Summary

Recovered the demo terminal lane after operator feedback invalidated the
browser-first terminal input follow-up. The lane now treats direct attached
terminal sessions as the next human-facing slice, while keeping `demo input`
as secondary automation/client infrastructure.

## What Became Stale

- ready card `048` assumed browser-side text entry should land next
- that assumption no longer matches the agreed UX boundary for humans

## Recovery Result

- marked `048` superseded instead of continuing execution from it
- promoted attached human terminal interaction as the next ready slice
- kept the no-nested-TUI rule and runner-owned session contract as the
  authority chain

## Why

- `demo input --text ...` is useful contract infrastructure
- it is poor default UX for humans doing iterative terminal debugging
- direct attachment is the honest human path; structured forwarding remains the
  honest client/automation path

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute [`049-implement-demo-attached-terminal-run-mode.md`](../../../specs/batch-cards/049-implement-demo-attached-terminal-run-mode.md)
to make direct attached terminal sessions the default human path for demos that
need interactive terminal IO.
