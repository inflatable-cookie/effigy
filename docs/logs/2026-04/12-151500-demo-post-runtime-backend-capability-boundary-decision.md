# Demo Post-Runtime-Backend-Capability Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Card: `065-decide-demo-post-runtime-backend-capability-boundary`

## Summary

After landing backend identity and capability reporting, the next bounded demo
slice should implement one richer runtime backend projection instead of taking
another browser follow-up or pausing the lane.

## Decision

- do not take a browser consumer follow-up next
- do not pause terminal/runtime work yet
- implement one richer runner backend slice next:
  concurrent-runner-backed demo session projection
- preserve the no-nested-TUI rule

## Why

- the backend/capability contract now exists to support exactly this richer
  runtime step
- concurrent-runner-backed demos were already called out in the roadmap, but
  still lack implementation behind the boundary
- browser consumers already have enough surface; the sharper remaining gap is
  runner-owned projection

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `ROUTE`
- Moved: `post-backend-capability ambiguity -> explicit concurrent-runner session projection next`
- Remaining: implement the bounded concurrent-runner demo session projection
